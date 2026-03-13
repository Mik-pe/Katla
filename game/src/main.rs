use clap::Parser;
use katla_app::application::ApplicationBuilder;
use log::info;

use katla_app::animation::{AnimationUpdateSystem, SkeletalAnimationSystem};
use katla_app::systems::{
    FlyCameraLookSystem, LightingSystem, PhysicsSystem, TransformHierarchySystem, VelocitySystem,
};
use katla_ecs::SystemExecutionOrder;

/// Katla 3D Engine - Command line arguments
#[derive(Parser, Debug)]
#[command(name = "katla")]
#[command(about = "Vulkan-based 3D rendering engine", long_about = None)]
struct Args {
    /// Run in limited-frame mode for validation testing
    #[arg(short, long)]
    single_frame: bool,

    /// Check for black frames by reading back swapchain center pixel
    #[arg(long)]
    check_black_frames: bool,
}

fn main() {
    let args = Args::parse();

    // Configure logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("Katla 3D Engine starting...");
    if args.single_frame {
        info!("Running in limited-frame mode (25 frames) for validation testing");
    }
    if args.check_black_frames {
        info!("Black frame detection enabled - will check center pixel color");
    }

    // Build with conditional configuration
    let builder = ApplicationBuilder::new()
        // Register systems with proper execution order
        .with_system(
            Box::new(TransformHierarchySystem::default()),
            SystemExecutionOrder::EARLY,
        )
        .with_system(
            Box::new(AnimationUpdateSystem),
            SystemExecutionOrder::NORMAL,
        )
        .with_system(
            Box::new(SkeletalAnimationSystem::default()),
            SystemExecutionOrder::NORMAL,
        )
        .with_system(Box::new(LightingSystem), SystemExecutionOrder::NORMAL)
        .with_system(Box::new(FlyCameraLookSystem), SystemExecutionOrder::NORMAL)
        .with_system(Box::new(PhysicsSystem), SystemExecutionOrder::NORMAL)
        .with_system(Box::new(VelocitySystem), SystemExecutionOrder::LATE);

    let result = if args.single_frame {
        builder
            .with_name("Katla")
            .validation_layer(true)
            .max_frames(100)
            .check_black_frames(args.check_black_frames)
            .build()
    } else {
        builder
            .with_name("Katla")
            .validation_layer(true)
            .check_black_frames(args.check_black_frames)
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
