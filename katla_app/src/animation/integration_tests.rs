#[cfg(test)]
mod integration_tests {
    use katla_ecs::World;
    use crate::util::GLTFModel;
    use crate::animation::{AnimationManager, AnimatedModel};
    use std::path::PathBuf;

    #[test]
    #[ignore] // Integration test - requires resources folder
    fn test_load_fox_animations() {
        // Find the resources folder (workspace root, not crate root)
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.pop(); // Go up from katla_app to workspace root
        path.push("resources/models/Fox.glb");

        assert!(path.exists(), "Fox.glb not found at {:?}", path);

        // Load the GLTF model
        let model = GLTFModel::new(&path);

        // Check that animations exist
        let animations: Vec<gltf::Animation> = model.document.animations().collect();
        println!("Fox model has {} animations:", animations.len());

        for (index, anim) in animations.iter().enumerate() {
            let default_name = format!("Animation_{}", index);
            let name = anim.name().unwrap_or(default_name.as_str());
            let channels = anim.channels().count();
            let samplers = anim.samplers().count();

            println!("  - {}: {} channels, {} samplers", name, channels, samplers);
        }

        // Load animations into the world
        let mut world = World::new();
        AnimationManager::load_gltf_animations(&mut world, &model);

        // Check that the fox has the expected animations
        assert!(animations.len() > 0, "Fox should have animations");

        // Common fox animations from the sample file
        let animation_names: Vec<String> = animations
            .iter()
            .map(|a| a.name().unwrap_or("unknown").to_string())
            .collect();

        println!("Animation names: {:?}", animation_names);

        // The fox model typically has these animations
        // (exact names depend on the specific file)
        if let Some(walk) = animation_names.iter().find(|n: &&String| n.contains("Walk") || n.contains("walk")) {
            println!("Found Walk animation: {}", walk);
        }

        if let Some(run) = animation_names.iter().find(|n: &&String| n.contains("Run") || n.contains("run")) {
            println!("Found Run animation: {}", run);
        }
    }

    #[test]
    #[ignore] // Integration test - requires resources folder
    fn test_load_fox_skins() {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.pop(); // Go up from katla_app to workspace root
        path.push("resources/models/Fox.glb");

        assert!(path.exists(), "Fox.glb not found at {:?}", path);

        let model = GLTFModel::new(&path);

        // Check for skins
        let skins: Vec<gltf::Skin> = model.document.skins().collect();
        println!("Fox model has {} skins", skins.len());

        for (index, skin) in skins.iter().enumerate() {
            let default_name = format!("Skin_{}", index);
            let skin_name: &str = skin.name().unwrap_or(default_name.as_str());
            let joints = skin.joints().count();
            let has_ibm = skin.inverse_bind_matrices().is_some();

            println!("  - {}: {} joints, inverse bind matrices: {}", skin_name, joints, has_ibm);
        }

        // Fox should have a skin
        assert!(skins.len() > 0, "Fox should have a skin for skeletal animation");

        let fox_skin = &skins[0];
        let joint_count = fox_skin.joints().count();

        println!("Fox has {} joints", joint_count);
        assert!(joint_count > 0, "Fox skin should have joints");
    }

    #[test]
    #[ignore] // Integration test - requires resources folder
    fn test_fox_animation_structure() {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.pop(); // Go up from katla_app to workspace root
        path.push("resources/models/Fox.glb");

        let model = GLTFModel::new(&path);

        // Examine animation structure
        let animations: Vec<gltf::Animation> = model.document.animations().collect();

        for anim in animations.iter() {
            let anim_name: &str = anim.name().unwrap_or("unnamed");

            println!("\nAnimation: {}", anim_name);
            println!("  Channels:");

            for channel in anim.channels() {
                let target = channel.target().node().index();
                let property = match channel.target().property() {
                    gltf::animation::Property::Translation => "translation",
                    gltf::animation::Property::Rotation => "rotation",
                    gltf::animation::Property::Scale => "scale",
                    gltf::animation::Property::MorphTargetWeights => "weights",
                };

                let sampler = channel.sampler();
                let interpolation = match sampler.interpolation() {
                    gltf::animation::Interpolation::Linear => "linear",
                    gltf::animation::Interpolation::Step => "step",
                    gltf::animation::Interpolation::CubicSpline => "cubic spline",
                };

                // Get keyframe count
                let inputs_count = sampler.input().count();

                println!("    - Node {}: {} ({})", target, property, interpolation);
                println!("      Keyframes: {}", inputs_count);
            }
        }
    }

    #[test]
    #[ignore] // Integration test - requires resources folder
    fn test_tiger_animations() {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.pop(); // Go up from katla_app to workspace root
        path.push("resources/models/Tiger.glb");

        if !path.exists() {
            println!("Tiger.glb not found, skipping test");
            return;
        }

        let model = GLTFModel::new(&path);

        let animations: Vec<_> = model.document.animations().collect();
        println!("Tiger model has {} animations", animations.len());

        let skins: Vec<_> = model.document.skins().collect();
        println!("Tiger model has {} skins", skins.len());
    }

    #[test]
    #[ignore] // Integration test - requires resources folder
    fn test_all_model_animations() {
        // Test all models in the resources folder
        let models = vec![
            "Fox.glb",
            "FoxBlender.glb",
            "FoxFixed.glb",
            "Tiger.glb",
            "Avocado.glb",
        ];

        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.pop(); // Go up from katla_app to workspace root
        path.push("resources/models");

        for model_name in models {
            let model_path = path.join(model_name);

            if !model_path.exists() {
                println!("{} not found, skipping", model_name);
                continue;
            }

            println!("\n=== Testing {} ===", model_name);

            let model = GLTFModel::new(&model_path);

            let animations: Vec<gltf::Animation> = model.document.animations().collect();
            let skins: Vec<gltf::Skin> = model.document.skins().collect();

            println!("Animations: {}", animations.len());
            println!("Skins: {}", skins.len());

            for anim in animations.iter() {
                let anim_name: &str = anim.name().unwrap_or("unnamed");
                let channels = anim.channels().count();
                println!("  - {} ({} channels)", anim_name, channels);
            }

            for skin in skins.iter() {
                let skin_name: &str = skin.name().unwrap_or("unnamed");
                let joints = skin.joints().count();
                println!("  - {} ({} joints)", skin_name, joints);
            }
        }
    }
}
