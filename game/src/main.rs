use clap::Parser;
use katla_app::animation::AnimationUpdateSystem;
use katla_app::application::ApplicationBuilder;
use katla_app::components::transform::TransformComponent;
use katla_app::input::{Action, InputState};
use katla_app::systems::{
    OrbitCameraSystem, PhysicsSystem, TransformHierarchySystem, VelocitySystem,
};
use katla_ecs::SystemExecutionOrder;
use katla_script::{InputSnapshot, ScriptSystem};
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
        info!("Running in limited-frame mode (100 frames) for validation testing");
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
        .with_system(Box::new(VelocitySystem), SystemExecutionOrder::LATE)
        .with_system(
            Box::new(
                ScriptSystem::new()
                    .with_transform_provider(|world| {
                        world
                            .query_ref::<&TransformComponent>()
                            .map(|(id, tc)| (id, tc.transform))
                            .collect()
                    })
                    .with_command_consumer(|world, commands| {
                        for cmd in commands {
                            match cmd {
                                katla_script::ScriptCommand::SetTransform(entity, transform) => {
                                    if let Some(tc) =
                                        world.get_component_mut::<TransformComponent>(*entity)
                                    {
                                        tc.transform = *transform;
                                    }
                                }
                                katla_script::ScriptCommand::SetPosition(entity, position) => {
                                    if let Some(tc) =
                                        world.get_component_mut::<TransformComponent>(*entity)
                                    {
                                        tc.transform.position = *position;
                                    }
                                }
                                _ => {}
                            }
                        }
                    })
                    .with_input_provider(|world| {
                        let mut snapshot = InputSnapshot::default();
                        if let Some(input) = world.get_resource::<InputState>() {
                            let actions: [(Action, &str); 15] = [
                                (Action::MoveForward, "move_forward"),
                                (Action::MoveBackward, "move_backward"),
                                (Action::MoveLeft, "move_left"),
                                (Action::MoveRight, "move_right"),
                                (Action::MoveUp, "move_up"),
                                (Action::MoveDown, "move_down"),
                                (Action::Jump, "jump"),
                                (Action::Interact, "interact"),
                                (Action::Inventory, "inventory"),
                                (Action::Pause, "pause"),
                                (Action::Exit, "exit"),
                                (Action::LookEnable, "look_enable"),
                                (Action::Sprint, "sprint"),
                                (Action::PanEnable, "pan_enable"),
                                (Action::Slow, "slow"),
                            ];
                            for (action, name) in &actions {
                                if input.is_action_pressed(*action) {
                                    snapshot.pressed_actions.insert(name.to_string());
                                }
                            }
                            snapshot.mouse_delta = input.mouse_delta;
                            snapshot.mouse_wheel = input.mouse_wheel_delta;
                        }
                        snapshot
                    }),
            ),
            SystemExecutionOrder::LATE,
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
