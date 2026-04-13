use clap::Parser;
use katla_app::animation::{AnimatedModel, AnimationPlayer, AnimationUpdateSystem};
use katla_app::application::ApplicationBuilder;
use katla_app::components::{
    DragComponent, OrbitCameraControllerComponent, Parent, TransformComponent, VelocityComponent,
};
use katla_app::systems::{
    OrbitCameraSystem, PhysicsSystem, TransformHierarchySystem, VelocitySystem,
};
use katla_ecs::{ComponentAccess, SystemExecutionOrder};
use log::info;

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
        // Register systems with proper execution order and component access
        .with_system_and_access(
            Box::new(TransformHierarchySystem::default()),
            SystemExecutionOrder::EARLY,
            vec![
                ComponentAccess::read::<TransformComponent>(),
                ComponentAccess::read::<Parent>(),
            ],
        )
        .with_system_and_access(
            Box::new(AnimationUpdateSystem),
            SystemExecutionOrder::NORMAL,
            vec![
                ComponentAccess::read::<AnimatedModel>(),
                ComponentAccess::write::<AnimationPlayer>(),
            ],
        )
        .with_system_and_access(
            Box::new(OrbitCameraSystem),
            SystemExecutionOrder::NORMAL,
            vec![
                ComponentAccess::write::<OrbitCameraControllerComponent>(),
                ComponentAccess::write::<TransformComponent>(),
            ],
        )
        .with_system_and_access(
            Box::new(PhysicsSystem),
            SystemExecutionOrder::NORMAL,
            vec![
                ComponentAccess::read::<VelocityComponent>(),
                ComponentAccess::read::<DragComponent>(),
                ComponentAccess::write::<VelocityComponent>(),
            ],
        )
        .with_system_and_access(
            Box::new(VelocitySystem),
            SystemExecutionOrder::LATE,
            vec![
                ComponentAccess::write::<TransformComponent>(),
                ComponentAccess::read::<VelocityComponent>(),
            ],
        );

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
