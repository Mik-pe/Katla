use clap::Parser;
use katla_app::application::ApplicationBuilder;
use log::info;

use katla_app::animation::AnimationUpdateSystem;
use katla_app::systems::{
    FlyCameraLookSystem, LightingSystem, PhysicsSystem, TransformHierarchySystem, VelocitySystem,
};
use katla_ecs::System;

/// Katla 3D Engine - Command line arguments
#[derive(Parser, Debug)]
#[command(name = "katla")]
#[command(about = "Vulkan-based 3D rendering engine", long_about = None)]
struct Args {
    /// Run in limited-frame mode for validation testing
    #[arg(short, long)]
    single_frame: bool,
}

fn main() {
    let args = Args::parse();

    // Configure logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();

    info!("Katla 3D Engine starting...");
    if args.single_frame {
        info!("Running in limited-frame mode (25 frames) for validation testing");
    }

    let _systems: Vec<Box<dyn System>> = vec![
        Box::new(TransformHierarchySystem::default()), // EARLY: Update world transforms first
        Box::new(AnimationUpdateSystem),               // Update animation playback
        Box::new(LightingSystem),                      // Collect lights for rendering
        Box::new(FlyCameraLookSystem),
        Box::new(VelocitySystem),
        Box::new(PhysicsSystem),
    ];

    // Build with conditional configuration
    let result = if args.single_frame {
        ApplicationBuilder::new()
            .with_name("Katla")
            .validation_layer(true)
            .max_frames(25)
            .build()
    } else {
        ApplicationBuilder::new()
            .with_name("Katla")
            .validation_layer(true)
            .build()
    };

    match result {
        Ok((mut application, event_loop)) => {
            application.init();
            event_loop.run_app(&mut application).unwrap();
        }
        Err(e) => {
            eprintln!("Failed to initialize application: {}", e);
            std::process::exit(1);
        }
    }
}
