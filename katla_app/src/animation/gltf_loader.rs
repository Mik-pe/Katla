use crate::animation::samplers::Interpolation;
use crate::animation::{AnimationChannel, AnimationClip, AnimationSampler, ChannelPath};
use crate::util::gltf_parser::AttributeParser;
use gltf::buffer::Data as BufferData;

/// Load a single animation clip from a GLTF animation.
///
/// This is exposed for `AnimationManager::setup_animated_model`.
pub fn load_animation_clip(
    parser: &AttributeParser,
    gltf_animation: &gltf::Animation,
) -> AnimationClip {
    let name = gltf_animation.name().unwrap_or("Animation").to_string();

    let mut channels: Vec<AnimationChannel> = Vec::new();
    let mut duration: f32 = 0.0;

    for channel in gltf_animation.channels() {
        let sampler = channel.sampler();
        let target_node = channel.target().node().index();
        let property = match channel.target().property() {
            gltf::animation::Property::Translation => ChannelPath::Translation,
            gltf::animation::Property::Rotation => ChannelPath::Rotation,
            gltf::animation::Property::Scale => ChannelPath::Scale,
            gltf::animation::Property::MorphTargetWeights => ChannelPath::Weights,
        };

        let interpolation = match sampler.interpolation() {
            gltf::animation::Interpolation::Linear => Interpolation::Linear,
            gltf::animation::Interpolation::Step => Interpolation::Step,
            gltf::animation::Interpolation::CubicSpline => Interpolation::CubicSpline,
        };

        let input_accessor = sampler.input();
        let output_accessor = sampler.output();

        let animation_sampler = parse_sampler(
            parser,
            input_accessor,
            output_accessor,
            property,
            interpolation,
        );

        duration = duration.max(animation_sampler.duration());

        channels.push(AnimationChannel {
            target_node,
            path: property,
            sampler: animation_sampler,
        });
    }

    AnimationClip {
        name,
        duration,
        channels,
    }
}

fn parse_sampler(
    parser: &AttributeParser,
    input_accessor: gltf::Accessor,
    output_accessor: gltf::Accessor,
    path: ChannelPath,
    interpolation: Interpolation,
) -> AnimationSampler {
    let inputs = parser.parse_scalars(input_accessor);

    match path {
        ChannelPath::Translation => {
            let translations = parser.parse_positions(output_accessor);
            AnimationSampler::new_translation(inputs, translations, interpolation)
        }
        ChannelPath::Rotation => {
            let rotations = parser.parse_tangents(output_accessor); // Vec4 quaternions
            AnimationSampler::new_rotation(inputs, rotations, interpolation)
        }
        ChannelPath::Scale => {
            let scales = parser.parse_positions(output_accessor); // Vec3 scales
            AnimationSampler::new_scale(inputs, scales, interpolation)
        }
        ChannelPath::Weights => {
            let weights = parser.parse_scalars(output_accessor);
            AnimationSampler::new_weights(inputs, weights, interpolation)
        }
    }
}

/// Parse Mat4 matrices from an accessor.
///
/// This is exposed for `AnimationManager::setup_animated_model`.
pub fn parse_mat4_from_accessor(
    buffers: &[BufferData],
    accessor: gltf::Accessor,
) -> Vec<katla_math::Mat4> {
    let parser = AttributeParser::new(buffers);
    parser.parse_matrices(accessor)
}

/// Build parent indices for skeleton hierarchy.
///
/// Returns a vector where parent_indices[i] is the skeleton index of joint i's parent,
/// or None if it's a root joint.
pub fn build_skeleton_parents(
    skin_joints: &[usize],
    document: &gltf::Document,
) -> Vec<Option<usize>> {
    let nodes: Vec<_> = document.nodes().collect();

    // Map GLTF node index -> parent GLTF node index
    let mut gltf_parent_map: std::collections::HashMap<usize, Option<usize>> =
        std::collections::HashMap::new();
    for node in &nodes {
        gltf_parent_map.insert(node.index(), None);
    }
    for node in &nodes {
        for child in node.children() {
            gltf_parent_map.insert(child.index(), Some(node.index()));
        }
    }

    // Map GLTF node index -> skeleton joint index
    let mut node_to_joint: std::collections::HashMap<usize, usize> =
        std::collections::HashMap::new();
    for (joint_idx, &node_idx) in skin_joints.iter().enumerate() {
        node_to_joint.insert(node_idx, joint_idx);
    }

    // Convert GLTF parent indices to skeleton parent indices
    let mut parent_indices = Vec::with_capacity(skin_joints.len());
    for &node_idx in skin_joints {
        let parent = gltf_parent_map.get(&node_idx).copied().flatten();
        let skeleton_parent = parent.and_then(|p| node_to_joint.get(&p).copied());
        parent_indices.push(skeleton_parent);
    }

    parent_indices
}

/// Build local transforms for skeleton joints from GLTF node rest poses.
///
/// Returns a vector of local transforms (rest pose) for each joint in the skeleton.
pub fn build_skeleton_local_transforms(
    skin_joints: &[usize],
    document: &gltf::Document,
) -> Vec<katla_math::Mat4> {
    let nodes: Vec<_> = document.nodes().collect();
    let node_map: std::collections::HashMap<usize, gltf::Node> =
        nodes.iter().map(|n| (n.index(), n.clone())).collect();

    let mut local_transforms = Vec::with_capacity(skin_joints.len());
    for &node_idx in skin_joints {
        if let Some(node) = node_map.get(&node_idx) {
            let transform = node.transform();
            let (translation, rotation, scale) = transform.decomposed();
            let t = katla_math::Vec3::new(translation[0], translation[1], translation[2]);
            let r = katla_math::Quat::new(rotation[0], rotation[1], rotation[2], rotation[3]);
            let s = katla_math::Vec3::new(scale[0], scale[1], scale[2]);
            local_transforms.push(katla_math::Mat4::from_trs(t, r, s));
        } else {
            local_transforms.push(katla_math::Mat4::identity());
        }
    }

    local_transforms
}

/// Build world transforms for all nodes in topological order.
///
/// This optimized version processes nodes iteratively, ensuring each parent
/// is computed before its children. Uses memoization to avoid redundant
/// computation of shared ancestors in deep hierarchies.
///
/// Returns a map: node_index -> world_transform
#[cfg(test)]
fn build_world_transforms(
    nodes: &[gltf::Node],
    parent_map: &std::collections::HashMap<usize, Option<usize>>,
) -> std::collections::HashMap<usize, katla_math::Mat4> {
    use std::collections::{HashMap, VecDeque};

    let mut world_transforms: HashMap<usize, katla_math::Mat4> =
        HashMap::with_capacity(nodes.len());

    // Build node lookup by GLTF index (not Vec position!)
    let node_by_index: HashMap<usize, &gltf::Node> = nodes.iter().map(|n| (n.index(), n)).collect();

    // Build children map for topological traversal
    let mut children_map: HashMap<usize, Vec<usize>> = HashMap::new();
    for node in nodes {
        let node_index = node.index();
        children_map.entry(node_index).or_default();
        for child in node.children() {
            children_map
                .entry(node_index)
                .or_default()
                .push(child.index());
        }
    }

    // Find root nodes (no parent) and start BFS from them
    let mut queue: VecDeque<usize> = VecDeque::new();
    for node in nodes {
        let node_index = node.index();
        if parent_map.get(&node_index) == Some(&None) {
            queue.push_back(node_index);
        }
    }

    // Process nodes in topological order (parent before children)
    while let Some(node_index) = queue.pop_front() {
        let node = match node_by_index.get(&node_index) {
            Some(n) => n,
            None => continue,
        };

        // Compute local transform
        let transform = node.transform();
        let (translation, rotation, scale) = transform.decomposed();
        let t = katla_math::Vec3::new(translation[0], translation[1], translation[2]);
        let r = katla_math::Quat::new(rotation[0], rotation[1], rotation[2], rotation[3]);
        let s = katla_math::Vec3::new(scale[0], scale[1], scale[2]);
        let local_matrix = katla_math::Mat4::from_trs(t, r, s);

        // Get parent transform (already computed due to topological order)
        let world_matrix = if let Some(Some(parent_index)) = parent_map.get(&node_index) {
            if let Some(parent_transform) = world_transforms.get(parent_index) {
                parent_transform.clone() * local_matrix
            } else {
                // Parent not yet computed (shouldn't happen with proper topological order)
                local_matrix
            }
        } else {
            local_matrix
        };

        world_transforms.insert(node_index, world_matrix);

        // Queue children for processing
        if let Some(children) = children_map.get(&node_index) {
            for child_index in children {
                queue.push_back(*child_index);
            }
        }
    }

    world_transforms
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::GLTFModel;

    fn get_fox_model_path() -> std::path::PathBuf {
        let mut model_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        model_path.pop(); // Go up from katla_app to workspace root
        model_path.push("resources");
        model_path.push("models");
        model_path.push("Fox.glb");
        model_path
    }

    #[test]
    fn test_build_world_transforms_coverage() {
        let model_path = get_fox_model_path();
        if !model_path.exists() {
            eprintln!("Skipping test - Fox.glb not found");
            return;
        }

        let model = GLTFModel::new(&model_path).expect("Failed to load Fox.glb");
        let nodes: Vec<_> = model.document.nodes().collect();

        // Build parent map
        let mut parent_map: std::collections::HashMap<usize, Option<usize>> =
            std::collections::HashMap::new();
        for node in &nodes {
            parent_map.insert(node.index(), None);
        }
        for node in &nodes {
            for child in node.children() {
                parent_map.insert(child.index(), Some(node.index()));
            }
        }

        // Use the optimized function
        let transforms = build_world_transforms(&nodes, &parent_map);

        // Verify all nodes have transforms
        for node in &nodes {
            let idx = node.index();
            assert!(
                transforms.contains_key(&idx),
                "Missing transform for node {}",
                idx
            );
        }

        // Verify root nodes have identity-ish parent multiplication
        for node in &nodes {
            let idx = node.index();
            if parent_map.get(&idx) == Some(&None) {
                // Root node - its transform should just be its local transform
                let world = transforms.get(&idx).unwrap();
                let local = {
                    let (t, r, s) = node.transform().decomposed();
                    katla_math::Mat4::from_trs(
                        katla_math::Vec3::new(t[0], t[1], t[2]),
                        katla_math::Quat::new(r[0], r[1], r[2], r[3]),
                        katla_math::Vec3::new(s[0], s[1], s[2]),
                    )
                };

                let world_data = world.to_array();
                let local_data = local.to_array();
                for (i, (w, l)) in world_data.iter().zip(local_data.iter()).enumerate() {
                    assert!(
                        (w - l).abs() < 0.0001,
                        "Root node {} mismatch at index {}: world={}, local={}",
                        idx,
                        i,
                        w,
                        l
                    );
                }
            }
        }
    }

    /// Test that parent indices are topologically sorted (parents before children)
    /// This is required for the single-pass compute_world_transforms to work correctly
    #[test]
    fn test_skeleton_parent_ordering() {
        let model_path = get_fox_model_path();
        if !model_path.exists() {
            eprintln!("Skipping test - Fox.glb not found");
            return;
        }

        let model = GLTFModel::new(&model_path).expect("Failed to load Fox.glb");
        let document = &model.document;

        // Get skin data
        let skins: Vec<_> = document.skins().collect();
        if skins.is_empty() {
            eprintln!("Skipping test - Fox.glb has no skins");
            return;
        }

        let gltf_skin = &skins[0];
        let joints: Vec<usize> = gltf_skin.joints().map(|n| n.index()).collect();
        let parent_indices = build_skeleton_parents(&joints, document);

        // Check that all parent indices come before their children
        // i.e., if parent_indices[i] = Some(p), then p < i
        for (i, parent) in parent_indices.iter().enumerate() {
            if let Some(p) = parent {
                assert!(
                    *p < i,
                    "Joint {} has parent {} which comes after it in the joint array. \
                     This breaks single-pass world transform computation!",
                    i,
                    p
                );
            }
        }
    }

    /// Test that sampling animations produces reasonable transform values
    #[test]
    fn test_animation_samples_are_sane() {
        let model_path = get_fox_model_path();
        if !model_path.exists() {
            eprintln!("Skipping test - Fox.glb not found");
            return;
        }

        let model = GLTFModel::new(&model_path).expect("Failed to load Fox.glb");
        let document = &model.document;

        // Load animation clips
        let parser = AttributeParser::new(&model.buffers);
        let animations: Vec<_> = document.animations().collect();
        if animations.is_empty() {
            eprintln!("Skipping test - Fox.glb has no animations");
            return;
        }

        for (anim_idx, gltf_animation) in animations.iter().enumerate() {
            let clip = load_animation_clip(&parser, gltf_animation);
            eprintln!(
                "Testing animation '{}' ({} channels, {:.2}s duration)",
                clip.name,
                clip.channels.len(),
                clip.duration
            );

            // Sample at multiple points in time
            let sample_times = [
                0.0,
                clip.duration * 0.25,
                clip.duration * 0.5,
                clip.duration * 0.75,
                clip.duration,
            ];

            for time in sample_times {
                let samples = clip.sample(time);

                for (node_idx, path, value) in &samples {
                    match value {
                        crate::animation::clips::SampledValue::Vec3(v) => {
                            // Check for NaN/Inf
                            assert!(
                                v[0].is_finite() && v[1].is_finite() && v[2].is_finite(),
                                "Animation '{}' at t={:.2}: node {} {:?} has non-finite value {:?}",
                                clip.name,
                                time,
                                node_idx,
                                path,
                                v
                            );

                            // Check for unreasonably large values
                            let max_reasonable = 1000.0;
                            assert!(
                                v[0].abs() < max_reasonable
                                    && v[1].abs() < max_reasonable
                                    && v[2].abs() < max_reasonable,
                                "Animation '{}' at t={:.2}: node {} {:?} has suspiciously large value {:?}",
                                clip.name,
                                time,
                                node_idx,
                                path,
                                v
                            );
                        }
                        crate::animation::clips::SampledValue::Quat(q) => {
                            // Check for NaN/Inf
                            assert!(
                                q[0].is_finite()
                                    && q[1].is_finite()
                                    && q[2].is_finite()
                                    && q[3].is_finite(),
                                "Animation '{}' at t={:.2}: node {} {:?} has non-finite quaternion {:?}",
                                clip.name,
                                time,
                                node_idx,
                                path,
                                q
                            );

                            // Check quaternion is normalized (within tolerance)
                            let len =
                                (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
                            assert!(
                                (len - 1.0).abs() < 0.1,
                                "Animation '{}' at t={:.2}: node {} {:?} has non-unit quaternion {:?} (len={:.3})",
                                clip.name,
                                time,
                                node_idx,
                                path,
                                q,
                                len
                            );
                        }
                        crate::animation::clips::SampledValue::Float(f) => {
                            assert!(
                                f.is_finite(),
                                "Animation '{}' at t={:.2}: node {} {:?} has non-finite float {}",
                                clip.name,
                                time,
                                node_idx,
                                path,
                                f
                            );
                        }
                        crate::animation::clips::SampledValue::Unknown => {
                            // Skip unknown sample types
                        }
                    }
                }
            }
        }
    }

    /// Test that inverse bind matrices have reasonable values
    #[test]
    fn test_inverse_bind_matrices_are_sane() {
        let model_path = get_fox_model_path();
        if !model_path.exists() {
            eprintln!("Skipping test - Fox.glb not found");
            return;
        }

        let model = GLTFModel::new(&model_path).expect("Failed to load Fox.glb");
        let document = &model.document;

        // Get skin data
        let skins: Vec<_> = document.skins().collect();
        if skins.is_empty() {
            eprintln!("Skipping test - Fox.glb has no skins");
            return;
        }

        let gltf_skin = &skins[0];
        let joints: Vec<usize> = gltf_skin.joints().map(|n| n.index()).collect();

        // Get inverse bind matrices
        let inverse_bind_matrices = if let Some(accessor) = gltf_skin.inverse_bind_matrices() {
            parse_mat4_from_accessor(&model.buffers, accessor)
        } else {
            eprintln!("No inverse bind matrices in Fox.glb");
            return;
        };

        eprintln!(
            "Checking {} inverse bind matrices",
            inverse_bind_matrices.len()
        );

        for (i, matrix) in inverse_bind_matrices.iter().enumerate() {
            let data = matrix.to_array();

            // Check for NaN/Inf
            for (j, &val) in data.iter().enumerate() {
                assert!(
                    val.is_finite(),
                    "Inverse bind matrix {} has non-finite value at index {}: {}",
                    i,
                    j,
                    val
                );
            }

            // Check for unreasonably large values (should be within model bounds)
            let max_reasonable = 100.0; // Fox is roughly 100 units
            for (j, &val) in data.iter().enumerate() {
                assert!(
                    val.abs() < max_reasonable,
                    "Inverse bind matrix {} has suspiciously large value at index {}: {}",
                    i,
                    j,
                    val
                );
            }

            // Print translation component for debugging
            let translation = matrix.extract_translation();
            eprintln!(
                "  IBM[{}] translation: ({:.2}, {:.2}, {:.2})",
                i,
                translation.x(),
                translation.y(),
                translation.z()
            );
        }
    }

    /// Test the full animated skeleton pipeline with actual animation data
    #[test]
    fn test_animated_skeleton_pipeline() {
        use crate::animation::clips::SampledValue;

        let model_path = get_fox_model_path();
        if !model_path.exists() {
            eprintln!("Skipping test - Fox.glb not found");
            return;
        }

        let model = GLTFModel::new(&model_path).expect("Failed to load Fox.glb");
        let document = &model.document;

        // Get skin data
        let skins: Vec<_> = document.skins().collect();
        if skins.is_empty() {
            eprintln!("Skipping test - Fox.glb has no skins");
            return;
        }

        let gltf_skin = &skins[0];
        let joints: Vec<usize> = gltf_skin.joints().map(|n| n.index()).collect();

        // Get inverse bind matrices
        let inverse_bind_matrices = if let Some(accessor) = gltf_skin.inverse_bind_matrices() {
            parse_mat4_from_accessor(&model.buffers, accessor)
        } else {
            vec![katla_math::Mat4::identity(); joints.len()]
        };

        // Build skeleton data
        let parent_indices = build_skeleton_parents(&joints, document);
        let local_transforms = build_skeleton_local_transforms(&joints, document);

        // Load first animation
        let parser = AttributeParser::new(&model.buffers);
        let animations: Vec<_> = document.animations().collect();
        if animations.is_empty() {
            eprintln!("Skipping test - no animations");
            return;
        }

        let clip = load_animation_clip(&parser, &animations[0]);
        eprintln!("Testing with animation '{}' at various times", clip.name);

        // Test at multiple animation times
        for time_percent in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let time = clip.duration * time_percent;
            let samples = clip.sample(time);

            // Apply animation samples to local transforms (like SkeletalAnimationSystem does)
            let mut animated_local = local_transforms.clone();
            for (node_index, path, value) in &samples {
                if let Some(joint_index) = joints.iter().position(|&j| j == *node_index) {
                    if joint_index < animated_local.len() {
                        let transform = &animated_local[joint_index];
                        let decomposed = transform.decompose();

                        let new_transform = match (path, value) {
                            (crate::animation::ChannelPath::Translation, SampledValue::Vec3(t)) => {
                                let t_vec = katla_math::Vec3::new(t[0], t[1], t[2]);
                                katla_math::Mat4::from_trs(
                                    t_vec,
                                    decomposed.rotation,
                                    decomposed.scale,
                                )
                            }
                            (crate::animation::ChannelPath::Rotation, SampledValue::Quat(q)) => {
                                let q_quat = katla_math::Quat::new(q[0], q[1], q[2], q[3]);
                                katla_math::Mat4::from_trs(
                                    decomposed.position,
                                    q_quat,
                                    decomposed.scale,
                                )
                            }
                            (crate::animation::ChannelPath::Scale, SampledValue::Vec3(s)) => {
                                let s_vec = katla_math::Vec3::new(s[0], s[1], s[2]);
                                katla_math::Mat4::from_trs(
                                    decomposed.position,
                                    decomposed.rotation,
                                    s_vec,
                                )
                            }
                            _ => continue,
                        };
                        animated_local[joint_index] = new_transform;
                    }
                }
            }

            // Compute world transforms (like Skeleton::compute_world_transforms does)
            let mut world_transforms = vec![katla_math::Mat4::identity(); joints.len()];
            for i in 0..world_transforms.len() {
                let local = animated_local[i].clone();
                if let Some(Some(parent_idx)) = parent_indices.get(i) {
                    if *parent_idx < world_transforms.len() {
                        world_transforms[i] = world_transforms[*parent_idx].clone() * local;
                    } else {
                        world_transforms[i] = local;
                    }
                } else {
                    world_transforms[i] = local;
                }
            }

            // Compute skinning matrices (like Skeleton::compute_skinning_matrices does)
            let mut skinning_matrices: Vec<katla_math::Mat4> = Vec::with_capacity(joints.len());
            for i in 0..joints.len() {
                let skin_matrix = world_transforms[i].mul(&inverse_bind_matrices[i]);
                skinning_matrices.push(skin_matrix);
            }

            // Verify all skinning matrices are sane
            for (i, matrix) in skinning_matrices.iter().enumerate() {
                let data = matrix.to_array();
                for (j, &val) in data.iter().enumerate() {
                    assert!(
                        val.is_finite(),
                        "Animation '{}' t={:.2}: skinning matrix {} has non-finite at [{}]: {}",
                        clip.name,
                        time,
                        i,
                        j,
                        val
                    );

                    // Allow larger values but not infinite
                    assert!(
                        val.abs() < 1e6,
                        "Animation '{}' t={:.2}: skinning matrix {} has extreme value at [{}]: {}",
                        clip.name,
                        time,
                        i,
                        j,
                        val
                    );
                }
            }

            eprintln!(
                "  t={:.2}s: All {} skinning matrices are finite and reasonable",
                time,
                skinning_matrices.len()
            );
        }
    }
}
