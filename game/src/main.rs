use clap::Parser;
use katla_app::application::ApplicationBuilder;
use log::info;

use katla_app::animation::AnimationUpdateSystem;
use katla_app::systems::{
    OrbitCameraSystem, PhysicsSystem, TransformHierarchySystem, VelocitySystem,
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

    /// Enable GPU-assisted validation (more thorough but slower)
    #[arg(short = 'v', long)]
    gpu_validation: bool,

    /// Check for black frames by reading back swapchain center pixel
    #[arg(long)]
    check_black_frames: bool,
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = Args::parse();

    info!("Katla 3D Engine starting...");
    if args.single_frame {
        info!("Running in limited-frame mode (25 frames) for validation testing");
    }
    if args.gpu_validation {
        info!("GPU-assisted validation enabled");
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
        .with_system(Box::new(OrbitCameraSystem), SystemExecutionOrder::NORMAL)
        .with_system(Box::new(PhysicsSystem), SystemExecutionOrder::NORMAL)
        .with_system(Box::new(VelocitySystem), SystemExecutionOrder::LATE);

    let result = if args.single_frame {
        builder
            .with_name("Katla")
            .validation_layer(true)
            .gpu_assisted_validation(args.gpu_validation)
            .max_frames(100)
            .check_black_frames(args.check_black_frames)
            .build()
    } else {
        builder
            .with_name("Katla")
            .validation_layer(true)
            .gpu_assisted_validation(args.gpu_validation)
            .check_black_frames(args.check_black_frames)
            .build()
    };

    match result {
        Ok((mut application, event_loop)) => {
            application.init();
            info!("About to enter event loop");
            event_loop.run_app(&mut application).unwrap();
            info!("Event loop exited");
        }
        Err(e) => {
            eprintln!("Failed to initialize application: {}", e);
            std::process::exit(1);
        }
    }
}
