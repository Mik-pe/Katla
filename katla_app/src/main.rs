use katla::application::ApplicationBuilder;

use katla::animation::AnimationUpdateSystem;
use katla::systems::{
    FlyCameraLookSystem, LightingSystem, PhysicsSystem, TransformHierarchySystem, VelocitySystem,
};
use katla_ecs::System;

fn main() {
    // Parse command-line arguments
    let args: Vec<String> = std::env::args().collect();
    let single_frame = args.contains(&"--single-frame".to_string());

    if single_frame {
        println!("Running in limited-frame mode (10 frames) for validation testing");
    }

    let systems: Vec<Box<dyn System>> = vec![
        Box::new(TransformHierarchySystem::default()), // EARLY: Update world transforms first
        Box::new(AnimationUpdateSystem),               // Update animation playback
        Box::new(LightingSystem),                      // Collect lights for rendering
        Box::new(FlyCameraLookSystem),
        Box::new(VelocitySystem),
        Box::new(PhysicsSystem),
    ];

    let mut builder = ApplicationBuilder::new()
        .with_name("Katla")
        .validation_layer(true)
        .with_systems(systems);

    if single_frame {
        builder = builder.single_frame(true);
    }

    let (mut application, event_loop) = builder.build();

    application.init();
    event_loop.run_app(&mut application).unwrap();
}
