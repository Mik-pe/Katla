use crate::animation::samplers::Interpolation;
use crate::animation::{
    AnimatedModel, AnimationChannel, AnimationClip, AnimationSampler, ChannelPath,
};
use crate::util::GLTFModel;
use byteorder::{ByteOrder, LittleEndian};
use gltf::buffer::Data as BufferData;
use katla_ecs::World;

pub fn load_animations(world: &mut World, model: &GLTFModel) {
    let document = &model.document;

    let animations: Vec<_> = document.animations().collect();
    if animations.is_empty() {
        log::info!("Model has no animations");
        return;
    }

    log::info!("Model has {} animations:", animations.len());

    let mut animated_model = AnimatedModel {
        animations: std::collections::HashMap::new(),
        sequences: std::collections::HashMap::new(),
    };

    for (index, gltf_animation) in animations.iter().enumerate() {
        let name = gltf_animation
            .name()
            .unwrap_or(&format!("Animation_{}", index))
            .to_string();

        log::info!("  Parsing animation '{}'", name);

        let clip = load_animation_clip(&model.buffers, gltf_animation);
        animated_model.animations.insert(name, clip);
    }

    log::info!(
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
    buffers: &[BufferData],
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
            buffers,
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
    buffers: &[BufferData],
    input_accessor: gltf::Accessor,
    output_accessor: gltf::Accessor,
    path: ChannelPath,
    interpolation: Interpolation,
) -> AnimationSampler {
    let inputs = parse_accessor_f32(buffers, input_accessor);

    match path {
        ChannelPath::Translation => {
            let translations = parse_accessor_vec3(buffers, output_accessor);
            AnimationSampler::new_translation(inputs, translations, interpolation)
        }
        ChannelPath::Rotation => {
            let rotations = parse_accessor_vec4(buffers, output_accessor);
            AnimationSampler::new_rotation(inputs, rotations, interpolation)
        }
        ChannelPath::Scale => {
            let scales = parse_accessor_vec3(buffers, output_accessor);
            AnimationSampler::new_scale(inputs, scales, interpolation)
        }
        ChannelPath::Weights => {
            let weights = parse_accessor_f32(buffers, output_accessor);
            AnimationSampler::new_weights(inputs, weights, interpolation)
        }
    }
}

fn parse_accessor_f32(buffers: &[BufferData], accessor: gltf::Accessor) -> Vec<f32> {
    let view = match accessor.view() {
        Some(v) => v,
        None => return vec![],
    };

    let buf_index = view.buffer().index();
    let buf_stride = view.stride();
    let attr_buf = &buffers[buf_index];

    let start_index = accessor.offset() + view.offset();
    let stride = buf_stride.unwrap_or(accessor.size());
    let total_size = accessor.size() * accessor.count();
    let end_index = start_index + total_size;

    let attr_arr = &attr_buf[start_index..end_index];

    attr_arr
        .chunks(stride)
        .map(|bytes| LittleEndian::read_f32(&bytes[0..4]))
        .collect()
}

fn parse_accessor_vec3(buffers: &[BufferData], accessor: gltf::Accessor) -> Vec<[f32; 3]> {
    let view = match accessor.view() {
        Some(v) => v,
        None => return vec![],
    };

    let buf_index = view.buffer().index();
    let buf_stride = view.stride();
    let attr_buf = &buffers[buf_index];

    let start_index = accessor.offset() + view.offset();
    let stride = buf_stride.unwrap_or(accessor.size());
    let total_size = accessor.size() * accessor.count();
    let end_index = start_index + total_size;

    let attr_arr = &attr_buf[start_index..end_index];

    attr_arr
        .chunks(stride)
        .map(|bytes| {
            [
                LittleEndian::read_f32(&bytes[0..4]),
                LittleEndian::read_f32(&bytes[4..8]),
                LittleEndian::read_f32(&bytes[8..12]),
            ]
        })
        .collect()
}

fn parse_accessor_vec4(buffers: &[BufferData], accessor: gltf::Accessor) -> Vec<[f32; 4]> {
    let view = match accessor.view() {
        Some(v) => v,
        None => return vec![],
    };

    let buf_index = view.buffer().index();
    let buf_stride = view.stride();
    let attr_buf = &buffers[buf_index];

    let start_index = accessor.offset() + view.offset();
    let stride = buf_stride.unwrap_or(accessor.size());
    let total_size = accessor.size() * accessor.count();
    let end_index = start_index + total_size;

    let attr_arr = &attr_buf[start_index..end_index];

    attr_arr
        .chunks(stride)
        .map(|bytes| {
            [
                LittleEndian::read_f32(&bytes[0..4]),
                LittleEndian::read_f32(&bytes[4..8]),
                LittleEndian::read_f32(&bytes[8..12]),
                LittleEndian::read_f32(&bytes[12..16]),
            ]
        })
        .collect()
}

pub fn load_skins(world: &mut World, model: &GLTFModel) {
    let document = &model.document;

    let skins: Vec<_> = document.skins().collect();
    if skins.is_empty() {
        log::info!("Model has no skins");
        return;
    }

    log::info!("Model has {} skins:", skins.len());

    for (index, gltf_skin) in skins.iter().enumerate() {
        let name = gltf_skin
            .name()
            .unwrap_or(&format!("Skin_{}", index))
            .to_string();

        log::info!("  Parsing skin '{}'", name);

        let joints: Vec<usize> = gltf_skin.joints().map(|node| node.index()).collect();
        let joints_count = joints.len();

        log::debug!("    - Found {} joints", joints_count);

        let inverse_bind_matrices = if let Some(accessor) = gltf_skin.inverse_bind_matrices() {
            parse_accessor_mat4(&model.buffers, accessor)
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

    log::info!("  Successfully loaded {} skins", skins.len());
}

/// Parse Mat4 matrices from an accessor.
///
/// This is exposed for `AnimationManager::setup_animated_model`.
pub fn parse_mat4_from_accessor(buffers: &[BufferData], accessor: gltf::Accessor) -> Vec<katla_math::Mat4> {
    parse_accessor_mat4(buffers, accessor)
}

fn parse_accessor_mat4(buffers: &[BufferData], accessor: gltf::Accessor) -> Vec<katla_math::Mat4> {
    let view = match accessor.view() {
        Some(v) => v,
        None => return vec![],
    };

    let buf_index = view.buffer().index();
    let buf_stride = view.stride();
    let attr_buf = &buffers[buf_index];

    let start_index = accessor.offset() + view.offset();
    let stride = buf_stride.unwrap_or(accessor.size());
    let total_size = accessor.size() * accessor.count();
    let end_index = start_index + total_size;

    let attr_arr = &attr_buf[start_index..end_index];

    attr_arr
        .chunks(stride)
        .map(|bytes| {
            let m00 = LittleEndian::read_f32(&bytes[0..4]);
            let m01 = LittleEndian::read_f32(&bytes[4..8]);
            let m02 = LittleEndian::read_f32(&bytes[8..12]);
            let m03 = LittleEndian::read_f32(&bytes[12..16]);
            let m10 = LittleEndian::read_f32(&bytes[16..20]);
            let m11 = LittleEndian::read_f32(&bytes[20..24]);
            let m12 = LittleEndian::read_f32(&bytes[24..28]);
            let m13 = LittleEndian::read_f32(&bytes[28..32]);
            let m20 = LittleEndian::read_f32(&bytes[32..36]);
            let m21 = LittleEndian::read_f32(&bytes[36..40]);
            let m22 = LittleEndian::read_f32(&bytes[40..44]);
            let m23 = LittleEndian::read_f32(&bytes[44..48]);
            let m30 = LittleEndian::read_f32(&bytes[48..52]);
            let m31 = LittleEndian::read_f32(&bytes[52..56]);
            let m32 = LittleEndian::read_f32(&bytes[56..60]);
            let m33 = LittleEndian::read_f32(&bytes[60..64]);

            katla_math::Mat4([
                katla_math::Vec4::new(m00, m01, m02, m03),
                katla_math::Vec4::new(m10, m11, m12, m13),
                katla_math::Vec4::new(m20, m21, m22, m23),
                katla_math::Vec4::new(m30, m31, m32, m33),
            ])
        })
        .collect()
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
