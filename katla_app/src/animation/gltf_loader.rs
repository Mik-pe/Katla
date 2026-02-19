use crate::animation::samplers::Interpolation;
use crate::animation::{
    AnimatedModel, AnimationChannel, AnimationClip, AnimationSampler, ChannelPath,
};
use crate::util::gltf_parser::AttributeParser;
use crate::util::GLTFModel;
use gltf::buffer::Data as BufferData;
use katla_ecs::World;

pub fn load_animations(world: &mut World, model: &GLTFModel) {
    let document = &model.document;
    let parser = AttributeParser::new(&model.buffers);

    let animations: Vec<_> = document.animations().collect();
    if animations.is_empty() {
        log::debug!("Model has no animations");
        return;
    }

    log::debug!("Model has {} animations:", animations.len());

    let mut animated_model = AnimatedModel {
        animations: std::collections::HashMap::new(),
        sequences: std::collections::HashMap::new(),
    };

    for (index, gltf_animation) in animations.iter().enumerate() {
        let name = gltf_animation
            .name()
            .unwrap_or(&format!("Animation_{}", index))
            .to_string();

        log::debug!("  Parsing animation '{}'", name);

        let clip = load_animation_clip(&parser, gltf_animation);
        animated_model.animations.insert(name, clip);
    }

    log::debug!(
        "  Successfully loaded {} animation clips",
        animated_model.animations.len()
    );

    let entity = world.create_entity();
    world.add_component(entity, animated_model);
    log::debug!("  Attached AnimatedModel to entity {:?}", entity);
}

/// Load a single animation clip from a GLTF animation.
///
/// This is exposed for `AnimationManager::setup_animated_model`.
pub fn load_animation_clip(
    parser: &AttributeParser,
    gltf_animation: &gltf::Animation,
) -> AnimationClip {
    let name = gltf_animation
        .name()
        .unwrap_or("Animation")
        .to_string();

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

pub fn load_skins(world: &mut World, model: &GLTFModel) {
    let document = &model.document;
    let parser = AttributeParser::new(&model.buffers);

    let skins: Vec<_> = document.skins().collect();
    if skins.is_empty() {
        log::debug!("Model has no skins");
        return;
    }

    log::debug!("Model has {} skins:", skins.len());

    for (index, gltf_skin) in skins.iter().enumerate() {
        let name = gltf_skin
            .name()
            .unwrap_or(&format!("Skin_{}", index))
            .to_string();

        log::debug!("  Parsing skin '{}'", name);

        let joints: Vec<usize> = gltf_skin.joints().map(|node| node.index()).collect();
        let joints_count = joints.len();

        log::debug!("    - Found {} joints", joints_count);

        let inverse_bind_matrices = if let Some(accessor) = gltf_skin.inverse_bind_matrices() {
            parser.parse_matrices(accessor)
        } else {
            log::debug!("    - No inverse bind matrices, using identity");
            vec![katla_math::Mat4::identity(); joints_count]
        };

        let skin = crate::animation::Skin::new(name.clone(), joints, inverse_bind_matrices);

        log::debug!(
            "    - Created skin component with {} joints",
            skin.joint_count()
        );

        let entity = world.create_entity();
        world.add_component(entity, skin);
        log::debug!("    - Attached Skin component to entity {:?}", entity);
    }

    log::debug!("  Successfully loaded {} skins", skins.len());
}

/// Parse Mat4 matrices from an accessor.
///
/// This is exposed for `AnimationManager::setup_animated_model`.
pub fn parse_mat4_from_accessor(buffers: &[BufferData], accessor: gltf::Accessor) -> Vec<katla_math::Mat4> {
    let parser = AttributeParser::new(buffers);
    parser.parse_matrices(accessor)
}

pub fn build_skeleton(model: &GLTFModel, skin_joints: &[usize]) -> Vec<katla_math::Mat4> {
    log::debug!("Building skeleton for {} joints", skin_joints.len());

    let mut joint_transforms = Vec::with_capacity(skin_joints.len());
    let document = &model.document;

    let nodes: Vec<_> = document.nodes().collect();

    let mut parent_map: std::collections::HashMap<usize, Option<usize>> =
        std::collections::HashMap::new();
    for node in &nodes {
        let node_index = node.index();
        parent_map.insert(node_index, None);
    }

    for node in &nodes {
        for child in node.children() {
            parent_map.insert(child.index(), Some(node.index()));
        }
    }

    for joint_index in skin_joints {
        let node = nodes.get(*joint_index);

        if node.is_some() {
            let transform = get_node_world_transform(&nodes, &parent_map, *joint_index);
            joint_transforms.push(transform);
        } else {
            log::warn!("    Joint node {} not found", joint_index);
            joint_transforms.push(katla_math::Mat4::identity());
        }
    }

    log::debug!(
        "  Built skeleton with {} joint transforms",
        joint_transforms.len()
    );
    joint_transforms
}

/// Build parent indices for skeleton hierarchy.
///
/// Returns a vector where parent_indices[i] is the skeleton index of joint i's parent,
/// or None if it's a root joint.
pub fn build_skeleton_parents(skin_joints: &[usize], document: &gltf::Document) -> Vec<Option<usize>> {
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
pub fn build_skeleton_local_transforms(skin_joints: &[usize], document: &gltf::Document) -> Vec<katla_math::Mat4> {
    let nodes: Vec<_> = document.nodes().collect();
    let node_map: std::collections::HashMap<usize, gltf::Node> = nodes.iter()
        .map(|n| (n.index(), n.clone()))
        .collect();

    let mut local_transforms = Vec::with_capacity(skin_joints.len());
    for &node_idx in skin_joints {
        if let Some(node) = node_map.get(&node_idx) {
            let transform = node.transform();
            let (translation, rotation, scale) = transform.decomposed();
            let t = katla_math::Vec3::new(translation[0], translation[1], translation[2]);
            let r = katla_math::Quat::new_from_xyzw(rotation[0], rotation[1], rotation[2], rotation[3]);
            let s = katla_math::Vec3::new(scale[0], scale[1], scale[2]);
            local_transforms.push(katla_math::Mat4::from_trs(t, r, s));
        } else {
            local_transforms.push(katla_math::Mat4::identity());
        }
    }

    local_transforms
}

fn get_node_world_transform(
    nodes: &[gltf::Node],
    parent_map: &std::collections::HashMap<usize, Option<usize>>,
    node_index: usize,
) -> katla_math::Mat4 {
    let node = match nodes.get(node_index) {
        Some(n) => n,
        None => return katla_math::Mat4::identity(),
    };

    let transform = node.transform();
    let (translation, rotation, scale) = transform.decomposed();

    let t = katla_math::Vec3::new(translation[0], translation[1], translation[2]);
    let r = katla_math::Quat::new_from_xyzw(rotation[0], rotation[1], rotation[2], rotation[3]);
    let s = katla_math::Vec3::new(scale[0], scale[1], scale[2]);

    let local_matrix = katla_math::Mat4::from_trs(t, r, s);

    if let Some(Some(parent_index)) = parent_map.get(&node_index) {
        let parent_matrix = get_node_world_transform(nodes, parent_map, *parent_index);
        parent_matrix * local_matrix
    } else {
        local_matrix
    }
}
