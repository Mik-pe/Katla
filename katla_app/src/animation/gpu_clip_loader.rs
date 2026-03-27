use std::collections::HashMap;

use katla_gfx::{AnimChannelInfo, AnimClipHeader, JointInfo, SkeletonAnimParams};

use super::clips::{AnimationSampler, ChannelPath};
use super::components::{AnimatedModel, AnimationPlayer};
use super::samplers::Interpolation;
use super::skin::{Skeleton, Skin};

/// Prepared GPU animation data, ready for upload to PoseComputeBuffers.
pub(crate) struct GpuAnimData {
    /// Clip headers (one per clip)
    pub clip_headers: Vec<AnimClipHeader>,
    /// Channel infos (all channels from all clips, packed sequentially)
    pub channel_infos: Vec<AnimChannelInfo>,
    /// Keyframe timestamps (all from all samplers, packed sequentially)
    pub keyframe_times: Vec<f32>,
    /// Keyframe values (all from all samplers, packed sequentially)
    pub keyframe_values: Vec<f32>,
    /// Joint hierarchy info (one per joint)
    pub joint_infos: Vec<JointInfo>,
    /// Total joint count
    pub joint_count: usize,
}

/// Prepare animation data for GPU upload from ECS components.
///
/// Takes the AnimatedModel (clips), Skin (joints + IBM), and Skeleton (parent indices)
/// and produces flat arrays matching the GPU buffer layout.
pub(crate) fn prepare_gpu_anim_data(
    animated_model: &AnimatedModel,
    skin: &Skin,
    skeleton: &Skeleton,
) -> GpuAnimData {
    let joint_infos = build_joint_infos(skin, skeleton);
    let joint_count = joint_infos.len();

    // Build a lookup: GLTF node index -> skeleton joint index
    let node_to_joint: HashMap<usize, u32> = skin
        .joints
        .iter()
        .enumerate()
        .map(|(joint_idx, &node_idx)| (node_idx, joint_idx as u32))
        .collect();

    // Pre-compute clip ordering: iterate animations HashMap in insertion order
    // and build clip_headers + channel_infos + keyframe arrays
    let mut clip_headers = Vec::new();
    let mut channel_infos = Vec::new();
    let mut keyframe_times = Vec::new();
    let mut keyframe_values = Vec::new();

    for clip in animated_model.animations.values() {
        let channel_offset = channel_infos.len() as u32;

        for channel in &clip.channels {
            let target_joint = node_to_joint
                .get(&channel.target_node)
                .copied()
                .unwrap_or(0xFFFFFFFF);

            let path_type = match channel.path {
                ChannelPath::Translation => 0,
                ChannelPath::Rotation => 1,
                ChannelPath::Scale => 2,
                ChannelPath::Weights => 3,
            };

            let interpolation = match channel.sampler.interpolation {
                Interpolation::Linear => 0,
                Interpolation::Step => 1,
                Interpolation::CubicSpline => 2,
            };

            let keyframe_count = channel.sampler.inputs.len() as u32;
            let time_offset = keyframe_times.len() as u32;
            let value_offset = keyframe_values.len() as u32;

            // Pack sampler inputs
            keyframe_times.extend_from_slice(&channel.sampler.inputs);

            // Pack sampler outputs based on path and interpolation
            let values_floats = pack_sampler_values(&channel.sampler, channel.path);
            keyframe_values.extend_from_slice(&values_floats);

            channel_infos.push(AnimChannelInfo {
                target_joint,
                path_type,
                time_offset,
                value_offset,
                keyframe_count,
                interpolation,
                _pad: [0; 2],
            });
        }

        clip_headers.push(AnimClipHeader {
            duration: clip.duration,
            channel_offset,
            channel_count: (channel_infos.len() - channel_offset as usize) as u32,
            _pad: 0,
        });
    }

    GpuAnimData {
        clip_headers,
        channel_infos,
        keyframe_times,
        keyframe_values,
        joint_infos,
        joint_count,
    }
}

/// Build per-frame SkeletonAnimParams for a single skeleton entity.
pub(crate) fn build_skeleton_params(
    player: &AnimationPlayer,
    clip_name_to_index: &HashMap<String, u32>,
    joint_offset: u32,
    joint_count: u32,
) -> SkeletonAnimParams {
    let clip_index = player
        .current_clip
        .as_ref()
        .and_then(|name| clip_name_to_index.get(name).copied())
        .unwrap_or(0);

    let target_clip_index = player
        .target_clip
        .as_ref()
        .and_then(|name| clip_name_to_index.get(name).copied())
        .unwrap_or(0);

    let mut flags: u32 = 0;
    if player.playing {
        flags |= 1;
    }
    if player.loop_animation {
        flags |= 2;
    }
    if player.blending {
        flags |= 4;
    }

    SkeletonAnimParams {
        clip_index,
        target_clip_index,
        current_time: player.time,
        target_time: player.target_time,
        blend_weight: player.blend_weight,
        joint_offset,
        joint_count,
        flags,
    }
}

fn build_joint_infos(skin: &Skin, skeleton: &Skeleton) -> Vec<JointInfo> {
    skin.joints
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let parent_index = skeleton
                .parent_indices
                .get(i)
                .copied()
                .flatten()
                .unwrap_or(0xFFFFFFFF) as u32;

            let ibm = skin
                .inverse_bind_matrices
                .get(i)
                .copied()
                .unwrap_or(katla_math::Mat4::identity());

            // Decompose rest pose local transform into T/R/S for GPU defaults
            let local = skeleton
                .local_transforms
                .get(i)
                .copied()
                .unwrap_or(katla_math::Mat4::identity());
            let decomposed = local.decompose();
            let (rx, ry, rz, rw) = decomposed.rotation.xyzw();

            // Clamp scale to avoid zero-scale producing NaN in the shader
            let min_scale = 1e-6f32;

            JointInfo {
                inverse_bind_matrix: ibm.to_array(),
                parent_index,
                _pad: [0; 3],
                rest_translation: [
                    decomposed.position.x(),
                    decomposed.position.y(),
                    decomposed.position.z(),
                ],
                _pad2: 0,
                rest_rotation: [rx, ry, rz, rw],
                rest_scale: [
                    decomposed.scale.x().max(min_scale),
                    decomposed.scale.y().max(min_scale),
                    decomposed.scale.z().max(min_scale),
                ],
                _pad3: 0,
            }
        })
        .collect()
}

fn pack_sampler_values(sampler: &AnimationSampler, path: ChannelPath) -> Vec<f32> {
    match path {
        ChannelPath::Translation => {
            if let Some(ref translations) = sampler.translations {
                translations
                    .iter()
                    .flat_map(|v| v.iter().copied())
                    .collect()
            } else {
                Vec::new()
            }
        }
        ChannelPath::Rotation => {
            if let Some(ref rotations) = sampler.rotations {
                rotations.iter().flat_map(|q| q.iter().copied()).collect()
            } else {
                Vec::new()
            }
        }
        ChannelPath::Scale => {
            if let Some(ref scales) = sampler.scales {
                scales.iter().flat_map(|v| v.iter().copied()).collect()
            } else {
                Vec::new()
            }
        }
        ChannelPath::Weights => {
            if let Some(ref weights) = sampler.weights {
                weights.clone()
            } else {
                Vec::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::clips::{AnimationChannel, AnimationClip};
    use crate::animation::samplers::Interpolation;
    use katla_math::Mat4;

    fn make_test_skin_and_skeleton() -> (Skin, Skeleton) {
        let joints = vec![10, 11, 12];
        let ibms = vec![Mat4::identity(), Mat4::identity(), Mat4::identity()];
        let skin = Skin::new("test_skin", joints, ibms);

        let skeleton = Skeleton::with_parents("test_skeleton", vec![None, Some(0), Some(1)]);

        (skin, skeleton)
    }

    #[test]
    fn test_prepare_gpu_anim_data_empty() {
        let (skin, skeleton) = make_test_skin_and_skeleton();
        let animated_model = AnimatedModel {
            animations: HashMap::new(),
            sequences: HashMap::new(),
        };

        let data = prepare_gpu_anim_data(&animated_model, &skin, &skeleton);

        assert_eq!(data.clip_headers.len(), 0);
        assert_eq!(data.channel_infos.len(), 0);
        assert_eq!(data.keyframe_times.len(), 0);
        assert_eq!(data.keyframe_values.len(), 0);
        assert_eq!(data.joint_count, 3);
    }

    #[test]
    fn test_prepare_gpu_anim_data_single_clip() {
        let (skin, skeleton) = make_test_skin_and_skeleton();

        let sampler = AnimationSampler::new_translation(
            vec![0.0, 1.0],
            vec![[0.0, 1.0, 0.0], [1.0, 2.0, 3.0]],
            Interpolation::Linear,
        );

        let channel = AnimationChannel {
            target_node: 10,
            path: ChannelPath::Translation,
            sampler,
        };

        let clip = AnimationClip {
            name: "Walk".to_string(),
            duration: 1.0,
            channels: vec![channel],
        };

        let mut animations = HashMap::new();
        animations.insert("Walk".to_string(), clip);

        let animated_model = AnimatedModel {
            animations,
            sequences: HashMap::new(),
        };

        let data = prepare_gpu_anim_data(&animated_model, &skin, &skeleton);

        assert_eq!(data.clip_headers.len(), 1);
        assert_eq!(data.clip_headers[0].duration, 1.0);
        assert_eq!(data.clip_headers[0].channel_offset, 0);
        assert_eq!(data.clip_headers[0].channel_count, 1);

        assert_eq!(data.channel_infos.len(), 1);
        assert_eq!(data.channel_infos[0].target_joint, 0); // node 10 -> joint 0
        assert_eq!(data.channel_infos[0].path_type, 0); // Translation
        assert_eq!(data.channel_infos[0].keyframe_count, 2);
        assert_eq!(data.channel_infos[0].interpolation, 0); // Linear

        assert_eq!(data.keyframe_times.len(), 2);
        assert_eq!(data.keyframe_times[0], 0.0);
        assert_eq!(data.keyframe_times[1], 1.0);

        assert_eq!(data.keyframe_values.len(), 6); // 2 keyframes * 3 floats (vec3)
        assert_eq!(data.keyframe_values[0], 0.0);
        assert_eq!(data.keyframe_values[1], 1.0);
        assert_eq!(data.keyframe_values[2], 0.0);
        assert_eq!(data.keyframe_values[3], 1.0);
        assert_eq!(data.keyframe_values[4], 2.0);
        assert_eq!(data.keyframe_values[5], 3.0);
    }

    #[test]
    fn test_joint_infos_parent_mapping() {
        let (skin, skeleton) = make_test_skin_and_skeleton();
        let animated_model = AnimatedModel {
            animations: HashMap::new(),
            sequences: HashMap::new(),
        };

        let data = prepare_gpu_anim_data(&animated_model, &skin, &skeleton);

        assert_eq!(data.joint_infos.len(), 3);
        // Joint 0: root -> no parent -> 0xFFFFFFFF
        assert_eq!(data.joint_infos[0].parent_index, 0xFFFFFFFF);
        // Joint 1: parent is joint 0
        assert_eq!(data.joint_infos[1].parent_index, 0);
        // Joint 2: parent is joint 1
        assert_eq!(data.joint_infos[2].parent_index, 1);
    }

    #[test]
    fn test_joint_infos_ibm_column_major() {
        let joints = vec![0];
        let ibm = Mat4::from_translation([5.0, 10.0, 15.0]);
        let skin = Skin::new("test", joints, vec![ibm]);

        let skeleton = Skeleton::with_parents("test", vec![None]);

        let animated_model = AnimatedModel {
            animations: HashMap::new(),
            sequences: HashMap::new(),
        };

        let data = prepare_gpu_anim_data(&animated_model, &skin, &skeleton);

        // Translation in column-major: col3 = [tx, ty, tz, 1]
        // indices 12,13,14 in the flat array
        let arr = &data.joint_infos[0].inverse_bind_matrix;
        assert!((arr[12] - 5.0).abs() < 1e-6);
        assert!((arr[13] - 10.0).abs() < 1e-6);
        assert!((arr[14] - 15.0).abs() < 1e-6);
    }

    #[test]
    fn test_build_skeleton_params() {
        let mut clip_map = HashMap::new();
        clip_map.insert("Walk".to_string(), 0u32);
        clip_map.insert("Run".to_string(), 1u32);

        let player = AnimationPlayer::new("Walk").looping().with_duration(2.0);

        let params = build_skeleton_params(&player, &clip_map, 0, 8);

        assert_eq!(params.clip_index, 0);
        assert_eq!(params.target_clip_index, 0); // no target
        assert_eq!(params.current_time, 0.0);
        assert_eq!(params.joint_offset, 0);
        assert_eq!(params.joint_count, 8);
        assert_eq!(params.blend_weight, 1.0);
        // flags: playing=1, loop=2 => 3
        assert_eq!(params.flags, 3);
    }

    #[test]
    fn test_unknown_target_node_maps_to_sentinel() {
        let joints = vec![10];
        let skin = Skin::new("test", joints, vec![Mat4::identity()]);
        let skeleton = Skeleton::with_parents("test", vec![None]);

        let sampler =
            AnimationSampler::new_scale(vec![0.0], vec![[1.0, 1.0, 1.0]], Interpolation::Linear);
        let channel = AnimationChannel {
            target_node: 99, // not in skin.joints
            path: ChannelPath::Scale,
            sampler,
        };
        let clip = AnimationClip {
            name: "Test".to_string(),
            duration: 0.0,
            channels: vec![channel],
        };

        let mut animations = HashMap::new();
        animations.insert("Test".to_string(), clip);

        let animated_model = AnimatedModel {
            animations,
            sequences: HashMap::new(),
        };

        let data = prepare_gpu_anim_data(&animated_model, &skin, &skeleton);
        assert_eq!(data.channel_infos[0].target_joint, 0xFFFFFFFF);
    }

    #[test]
    fn test_rotation_packs_4_floats_per_keyframe() {
        let joints = vec![0];
        let skin = Skin::new("test", joints, vec![Mat4::identity()]);
        let skeleton = Skeleton::with_parents("test", vec![None]);

        let sampler = AnimationSampler::new_rotation(
            vec![0.0, 0.5, 1.0],
            vec![
                [0.0, 0.0, 0.0, 1.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            Interpolation::Linear,
        );
        let channel = AnimationChannel {
            target_node: 0,
            path: ChannelPath::Rotation,
            sampler,
        };
        let clip = AnimationClip {
            name: "Rot".to_string(),
            duration: 1.0,
            channels: vec![channel],
        };

        let mut animations = HashMap::new();
        animations.insert("Rot".to_string(), clip);

        let animated_model = AnimatedModel {
            animations,
            sequences: HashMap::new(),
        };

        let data = prepare_gpu_anim_data(&animated_model, &skin, &skeleton);
        // 3 keyframes * 4 floats (quat) = 12
        assert_eq!(data.keyframe_values.len(), 12);
        assert_eq!(data.channel_infos[0].path_type, 1); // Rotation
    }
}
