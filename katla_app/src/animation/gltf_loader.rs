use crate::util::GLTFModel;
use katla_ecs::World;

/// Load animations from a GLTF model into the world.
///
/// This is a placeholder implementation that logs the animations found
/// in the GLTF file but doesn't fully parse them yet.
///
/// TODO: Full implementation requires:
/// - Parse animation channels (translation, rotation, scale, morph weights)
/// - Read keyframe data from gltf accessors (input times, output values)
/// - Build AnimationClip structures with samplers
/// - Create AnimatedModel component and attach to entities
/// - Handle interpolation modes (Linear, Step, CubicSpline)
pub fn load_animations(_world: &mut World, model: &GLTFModel) {
    let document = &model.document;

    // Check if the model has any animations
    let animations: Vec<_> = document.animations().collect();
    if animations.is_empty() {
        println!("Model has no animations");
        return;
    }

    println!("Model has {} animations:", animations.len());

    // Log animation information
    for (index, gltf_animation) in animations.iter().enumerate() {
        let name = gltf_animation
            .name()
            .unwrap_or(&format!("Animation_{}", index))
            .to_string();

        let channels_count = gltf_animation.channels().count();
        let samplers_count = gltf_animation.samplers().count();

        println!(
            "  - Animation '{}': {} channels, {} samplers",
            name, channels_count, samplers_count
        );

        // Log each channel
        for channel in gltf_animation.channels() {
            let sampler = channel.sampler();
            let target_node = channel.target().node().index();
            let property = match channel.target().property() {
                gltf::animation::Property::Translation => "translation",
                gltf::animation::Property::Rotation => "rotation",
                gltf::animation::Property::Scale => "scale",
                gltf::animation::Property::MorphTargetWeights => "morph weights",
            };

            let interpolation_str = match sampler.interpolation() {
                gltf::animation::Interpolation::Linear => "linear",
                gltf::animation::Interpolation::Step => "step",
                gltf::animation::Interpolation::CubicSpline => "cubic spline",
            };
            println!(
                "    - Channel on node {}: {}, interpolation: {}",
                target_node, property, interpolation_str
            );
        }
    }

    // TODO: Create AnimatedModel component with parsed animation data
    // Need to:
    // 1. Build AnimationClip for each GLTF animation
    // 2. Create AnimationChannel for each GLTF channel
    // 3. Create AnimationSampler with keyframe data
    // 4. Store clips in AnimatedModel.animations HashMap
    // 5. Attach AnimatedModel to the loaded entity
    println!("Animation parsing is not yet fully implemented.");
}

/// Load skin data from a GLTF model.
///
/// Skins define the joint hierarchy and inverse bind matrices for skeletal animation.
pub fn load_skins(_world: &mut World, model: &GLTFModel) {
    let document = &model.document;

    let skins: Vec<_> = document.skins().collect();
    if skins.is_empty() {
        println!("Model has no skins");
        return;
    }

    println!("Model has {} skins:", skins.len());

    // Log skin information
    for (index, gltf_skin) in skins.iter().enumerate() {
        let name = gltf_skin
            .name()
            .unwrap_or(&format!("Skin_{}", index))
            .to_string();

        let joints_count = gltf_skin.joints().count();
        let has_inverse_bind = gltf_skin.inverse_bind_matrices().is_some();

        println!(
            "  - Skin '{}': {} joints, inverse bind matrices: {}",
            name, joints_count, has_inverse_bind
        );
    }

    // TODO: Store skin data properly
    // Need to:
    // 1. Create Skin component with joint indices
    // 2. Load inverse bind matrices from gltf accessors
    // 3. Store joint hierarchy (parent-child relationships)
    // 4. Attach Skin component to the skinned mesh entity
    println!("Skin loading is not yet fully implemented.");
}

/// Parse node hierarchy to build skeleton.
///
/// GLTF skins reference nodes by index. We need to build the actual
/// transform hierarchy for the skeleton.
pub fn build_skeleton(_model: &GLTFModel, skin_joints: &[usize]) -> Vec<katla_math::Mat4> {
    println!("Building skeleton for {} joints", skin_joints.len());

    // TODO: Extract node transforms from GLTF scene graph
    // Need to:
    // 1. Traverse GLTF nodes by index
    // 2. Build parent-child relationships
    // 3. Compute world-space transforms for each joint
    // 4. Return transform matrices for skeleton
    // For now, return identity matrices
    vec![katla_math::Mat4::identity(); skin_joints.len()]
}
