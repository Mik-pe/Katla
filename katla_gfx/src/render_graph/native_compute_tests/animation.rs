//! Native animation interpolation and glTF sampler boundary contracts.

use super::*;
use crate::animation::{AnimChannelInfo, AnimClipHeader, JointInfo, SkeletonAnimParams};

const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];
const LINEAR_VALUES: [f32; 6] = [2.0, 4.0, 6.0, 12.0, 14.0, 16.0];
const CUBIC_VALUES: [f32; 18] = [
    0.0, 0.0, 0.0, 2.0, 4.0, 6.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 12.0, 14.0, 16.0, 0.0, 0.0, 0.0,
];

struct AnimationCase<'a> {
    path_type: u32,
    interpolation: u32,
    times: &'a [f32],
    values: &'a [f32],
    rest_pose: bool,
}

fn translation_case(interpolation: u32, values: &[f32]) -> AnimationCase<'_> {
    AnimationCase {
        path_type: 0,
        interpolation,
        times: &[0.0, 1.0],
        values,
        rest_pose: false,
    }
}

fn translation(value: [f32; 3]) -> [f32; 16] {
    let mut result = IDENTITY;
    result[12..15].copy_from_slice(&value);
    result
}

#[test]
fn test_native_graph_animation_interpolates_imported_joint_output() {
    animation_output(
        translation_case(0, &LINEAR_VALUES),
        [0.25, 0.5, 0.75],
        |time| translation([2.0 + 10.0 * time, 4.0 + 10.0 * time, 6.0 + 10.0 * time]),
    );
}

#[test]
fn test_native_graph_animation_without_channels_preserves_rest_pose() {
    animation_output(
        AnimationCase {
            rest_pose: true,
            ..translation_case(0, &LINEAR_VALUES)
        },
        [0.25, 0.5, 0.75],
        |_| translation([2.0, 3.0, 4.0]),
    );
}

#[test]
fn test_native_graph_animation_cubic_translation_uses_gltf_triplets() {
    animation_output(
        translation_case(2, &CUBIC_VALUES),
        [0.25, 0.5, 0.75],
        |time| {
            let blend = time * time * (3.0 - 2.0 * time);
            translation([2.0 + 10.0 * blend, 4.0 + 10.0 * blend, 6.0 + 10.0 * blend])
        },
    );
}

#[test]
fn test_native_graph_animation_cubic_scale_applies_duration_to_tangents() {
    let values = [
        9.0, 8.0, 7.0, 2.0, 4.0, 6.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 12.0, 14.0, 16.0, 7.0, 8.0, 9.0,
    ];
    let case = AnimationCase {
        path_type: 2,
        times: &[0.0, 2.0],
        ..translation_case(2, &values)
    };
    animation_output(case, [0.5, 1.0, 1.5], |time| {
        let scale = if time == 0.5 {
            [3.46875, 5.65625, 7.84375]
        } else if time == 1.0 {
            [6.25, 8.25, 10.25]
        } else {
            [9.40625, 11.21875, 13.03125]
        };
        let mut result = translation([2.0, 3.0, 4.0]);
        result[0] = scale[0];
        result[5] = scale[1];
        result[10] = scale[2];
        result
    });
}

#[test]
fn test_native_graph_animation_cubic_rotation_normalizes_sampled_quaternion() {
    let half = std::f32::consts::FRAC_1_SQRT_2;
    let values = [
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        half, half, 0.0, 0.0, 0.0, 0.0,
    ];
    let case = AnimationCase {
        path_type: 1,
        ..translation_case(2, &values)
    };
    animation_output(case, [0.25, 0.5, 0.75], |time| {
        let blend = time * time * (3.0 - 2.0 * time);
        let z = half * blend;
        let w = 1.0 + (half - 1.0) * blend;
        let inverse_length = (z * z + w * w).sqrt().recip();
        let z = z * inverse_length;
        let w = w * inverse_length;
        let mut result = translation([2.0, 3.0, 4.0]);
        result[0] = 1.0 - 2.0 * z * z;
        result[1] = 2.0 * z * w;
        result[4] = -2.0 * z * w;
        result[5] = result[0];
        result
    });
}

#[test]
fn test_native_graph_animation_step_selects_last_keyframe_at_end() {
    animation_output(
        translation_case(1, &LINEAR_VALUES),
        [-0.25, 1.0, 1.25],
        |time| {
            translation(if time < 1.0 {
                [2.0, 4.0, 6.0]
            } else {
                [12.0, 14.0, 16.0]
            })
        },
    );
}

#[test]
fn test_native_graph_animation_linear_clamps_to_channel_interval() {
    let case = AnimationCase {
        times: &[0.5, 1.0],
        ..translation_case(0, &LINEAR_VALUES)
    };
    animation_output(case, [0.0, 0.75, 1.25], |time| {
        let blend = ((time - 0.5) * 2.0).clamp(0.0, 1.0);
        translation([2.0 + 10.0 * blend, 4.0 + 10.0 * blend, 6.0 + 10.0 * blend])
    });
}

#[test]
fn test_native_graph_animation_cubic_clamps_to_channel_interval() {
    let case = AnimationCase {
        times: &[0.5, 1.0],
        ..translation_case(2, &CUBIC_VALUES)
    };
    animation_output(case, [0.0, 0.75, 1.25], |time| {
        let blend = ((time - 0.5) * 2.0).clamp(0.0, 1.0);
        let blend = blend * blend * (3.0 - 2.0 * blend);
        translation([2.0 + 10.0 * blend, 4.0 + 10.0 * blend, 6.0 + 10.0 * blend])
    });
}

#[test]
fn test_native_graph_animation_linear_single_keyframe_is_constant() {
    let case = AnimationCase {
        times: &[0.0],
        ..translation_case(0, &[2.0, 4.0, 6.0])
    };
    animation_output(case, [0.25, 0.5, 0.75], |_| translation([2.0, 4.0, 6.0]));
}

#[test]
fn test_native_graph_animation_cubic_single_keyframe_is_constant() {
    let case = AnimationCase {
        times: &[0.0],
        ..translation_case(2, &[0.0, 0.0, 0.0, 2.0, 4.0, 6.0, 0.0, 0.0, 0.0])
    };
    animation_output(case, [0.25, 0.5, 0.75], |_| translation([2.0, 4.0, 6.0]));
}

fn animation_output(
    case: AnimationCase<'_>,
    sample_times: [f32; 3],
    expected: impl Fn(f32) -> [f32; 16],
) {
    let channels = [AnimChannelInfo {
        target_joint: 0,
        path_type: case.path_type,
        time_offset: 0,
        value_offset: 0,
        keyframe_count: case.times.len() as u32,
        interpolation: case.interpolation,
        _pad: [0; 2],
    }];
    let clips = [AnimClipHeader {
        duration: case.times.last().copied().unwrap_or(0.0).max(1.0),
        channel_offset: 0,
        channel_count: u32::from(!case.rest_pose),
        _pad: 0,
    }];
    let joints = [JointInfo {
        inverse_bind_matrix: IDENTITY,
        parent_index: u32::MAX,
        _pad: [0; 3],
        rest_translation: [2.0, 3.0, 4.0],
        _pad2: 0,
        rest_rotation: [0.0, 0.0, 0.0, 1.0],
        rest_scale: [1.0; 3],
        _pad3: 0,
    }];
    animation_rig_output(
        AnimationRig {
            clips: &clips,
            channels: &channels,
            times: case.times,
            values: case.values,
            joints: &joints,
        },
        sample_times,
        |time| SkeletonAnimParams {
            clip_index: 0,
            target_clip_index: 0,
            current_time: time,
            target_time: 0.0,
            blend_weight: 0.0,
            joint_offset: 0,
            joint_count: 1,
            flags: 1,
        },
        |time| expected(time).to_vec(),
    );
}

struct AnimationRig<'a> {
    clips: &'a [AnimClipHeader],
    channels: &'a [AnimChannelInfo],
    times: &'a [f32],
    values: &'a [f32],
    joints: &'a [JointInfo],
}

fn animation_rig_output(
    rig: AnimationRig<'_>,
    sample_times: [f32; 3],
    params_at: impl Fn(f32) -> SkeletonAnimParams,
    expected: impl Fn(f32) -> Vec<f32>,
) {
    let AnimationRig {
        clips,
        channels,
        times,
        values,
        joints,
    } = rig;
    let mut renderer = renderer();
    let errors = capture_validation_errors(&renderer);
    let output_size = (joints.len() * 64) as u64;
    let mut graph = readback_graph();
    let storage = BufferUsages::STORAGE | BufferUsages::TRANSFER_SOURCE;
    let params_desc = BufferDesc::new(32, storage, BufferMemoryPolicy::CpuVisible);
    let output_desc = BufferDesc::new(output_size, storage, BufferMemoryPolicy::DeviceLocal);
    let params = (0..FRAME_SLOTS)
        .map(|_| renderer.create_buffer(params_desc).unwrap())
        .collect::<Vec<_>>();
    let world = (0..FRAME_SLOTS)
        .map(|_| renderer.create_buffer(output_desc).unwrap())
        .collect::<Vec<_>>();
    let output = (0..FRAME_SLOTS)
        .map(|_| renderer.create_buffer(output_desc).unwrap())
        .collect::<Vec<_>>();
    let params_id = graph
        .import_buffer("animation params", params[0], params_desc)
        .unwrap();
    let world_id = graph
        .import_buffer("animation world", world[0], output_desc)
        .unwrap();
    let output_id = graph
        .import_buffer("animation output", output[0], output_desc)
        .unwrap();
    let static_data: [&[u8]; 5] = [
        bytemuck::cast_slice(clips),
        bytemuck::cast_slice(channels),
        bytemuck::cast_slice(times),
        bytemuck::cast_slice(values),
        bytemuck::cast_slice(joints),
    ];
    let mut ids = vec![params_id];
    for (index, data) in static_data.into_iter().enumerate() {
        ids.push(
            import_data(
                &mut renderer,
                &mut graph,
                &format!("animation static {index}"),
                data,
                storage,
                BufferMemoryPolicy::DeviceLocal,
            )
            .1,
        );
    }
    ids.extend([world_id, output_id]);
    dispatch(
        &mut graph,
        "animation pose",
        include_str!("../../../../resources/shaders/compute/animation/pose_eval.wgsl"),
        &ids.iter()
            .enumerate()
            .map(|(binding, &id)| (0, binding as u32, id))
            .collect::<Vec<_>>(),
    );
    prepare_copies(&mut renderer, &mut graph, &[(output_id, 0, output_size)]);
    for time in sample_times.into_iter().take(FRAME_SLOTS) {
        let frame = acquire(&mut renderer);
        let slot = frame.slot();
        graph
            .rebind_imported_buffer(params_id, params[slot])
            .unwrap();
        graph.rebind_imported_buffer(world_id, world[slot]).unwrap();
        graph
            .rebind_imported_buffer(output_id, output[slot])
            .unwrap();
        renderer
            .write_buffer(
                &frame,
                params[slot],
                0,
                bytemuck::bytes_of(&params_at(time)),
            )
            .unwrap();
        renderer.render(&frame, &mut graph, |_| {}).unwrap();
        renderer.present(frame).unwrap();
        #[cfg(target_os = "macos")]
        assert!(renderer.frame_slots[slot].submission.is_some());
    }
    for (slot, time) in sample_times.into_iter().take(FRAME_SLOTS).enumerate() {
        let bytes = read_completed(&mut renderer, &graph, slot);
        let matrix: Vec<_> = bytes[..output_size as usize]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|v| f32::from_le_bytes(*v))
            .collect();
        let expected = expected(time);
        for (actual, expected) in matrix.iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-5, "slot {slot}: {matrix:?}");
        }
    }
    graph.cleanup();
    renderer.destroy();
    assert!(
        errors.lock().unwrap().is_empty(),
        "{:?}",
        errors.lock().unwrap()
    );
}

fn rest_joint(parent_index: u32, rest_translation: [f32; 3]) -> JointInfo {
    JointInfo {
        inverse_bind_matrix: IDENTITY,
        parent_index,
        _pad: [0; 3],
        rest_translation,
        _pad2: 0,
        rest_rotation: [0.0, 0.0, 0.0, 1.0],
        rest_scale: [1.0; 3],
        _pad3: 0,
    }
}

fn blend_params(source_weight: f32, joint_count: u32) -> SkeletonAnimParams {
    SkeletonAnimParams {
        clip_index: 0,
        target_clip_index: 1,
        current_time: 0.0,
        target_time: 0.0,
        blend_weight: source_weight,
        joint_offset: 0,
        joint_count,
        flags: 5,
    }
}

fn blend_clips() -> [AnimClipHeader; 2] {
    [
        AnimClipHeader {
            duration: 1.0,
            channel_offset: 0,
            channel_count: 1,
            _pad: 0,
        },
        AnimClipHeader {
            duration: 1.0,
            channel_offset: 1,
            channel_count: 1,
            _pad: 0,
        },
    ]
}

fn blend_channels(path_type: u32, stride: u32) -> [AnimChannelInfo; 2] {
    [0, stride].map(|value_offset| AnimChannelInfo {
        target_joint: 0,
        path_type,
        time_offset: 0,
        value_offset,
        keyframe_count: 1,
        interpolation: 0,
        _pad: [0; 2],
    })
}

#[test]
fn test_native_graph_animation_crossfade_preserves_source_and_target_endpoints() {
    animation_rig_output(
        AnimationRig {
            clips: &blend_clips(),
            channels: &blend_channels(0, 3),
            times: &[0.0],
            values: &[2.0, 4.0, 6.0, 12.0, 14.0, 16.0],
            joints: &[rest_joint(u32::MAX, [0.0; 3])],
        },
        [1.0, 0.5, 0.0],
        |weight| blend_params(weight, 1),
        |weight| {
            translation([
                12.0 - 10.0 * weight,
                14.0 - 10.0 * weight,
                16.0 - 10.0 * weight,
            ])
            .to_vec()
        },
    );
}

#[test]
fn test_native_graph_animation_crossfade_preserves_child_bone_length() {
    animation_rig_output(
        AnimationRig {
            clips: &blend_clips(),
            channels: &blend_channels(1, 4),
            times: &[0.0],
            values: &[
                0.0,
                0.0,
                0.0,
                1.0,
                0.0,
                0.0,
                std::f32::consts::FRAC_1_SQRT_2,
                std::f32::consts::FRAC_1_SQRT_2,
            ],
            joints: &[
                rest_joint(u32::MAX, [0.0; 3]),
                rest_joint(0, [2.0, 0.0, 0.0]),
            ],
        },
        [1.0, 0.5, 0.0],
        |weight| blend_params(weight, 2),
        |weight| {
            let (sin, cos) = ((1.0 - weight) * std::f32::consts::FRAC_PI_2).sin_cos();
            let mut root = IDENTITY;
            root[0] = cos;
            root[1] = sin;
            root[4] = -sin;
            root[5] = cos;
            let mut child = root;
            child[12] = 2.0 * cos;
            child[13] = 2.0 * sin;
            [root, child].concat()
        },
    );
}

#[test]
fn test_native_graph_animation_crossfade_preserves_signed_and_zero_scale() {
    animation_rig_output(
        AnimationRig {
            clips: &blend_clips(),
            channels: &blend_channels(2, 3),
            times: &[0.0],
            values: &[-1.0, 0.0, 2.0, -3.0, 0.0, 4.0],
            joints: &[rest_joint(u32::MAX, [0.0; 3])],
        },
        [1.0, 0.5, 0.0],
        |weight| blend_params(weight, 1),
        |weight| {
            let mut matrix = IDENTITY;
            matrix[0] = -3.0 + 2.0 * weight;
            matrix[5] = 0.0;
            matrix[10] = 4.0 - 2.0 * weight;
            matrix.to_vec()
        },
    );
}
