use clap::Parser;
use katla_app::animation::AnimationUpdateSystem;
use katla_app::application::ApplicationBuilder;
use katla_app::components::transform::TransformComponent;
use katla_app::input::{Action, InputState};
use katla_app::systems::{OrbitCameraSystem, RapierPhysicsSystem, TransformHierarchySystem};
use katla_ecs::SystemExecutionOrder;
use katla_script::{InputSnapshot, ScriptSystem};
use log::{error, info};

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

    /// Scene file to load on startup (e.g., assets/scenes/playground.katla)
    #[arg(long)]
    scene: Option<String>,

    /// Dump UI layout tree to stdout after first frame, then exit
    #[arg(long)]
    dump_layout: bool,

    /// Dump UI layout tree to a file after first frame, then exit
    #[arg(long = "dump-layout-file", value_name = "PATH")]
    dump_layout_file: Option<String>,

    /// Run in headless mode (no window, offscreen rendering, save screenshot)
    #[arg(long)]
    headless: bool,

    /// Screenshot output path (requires --headless, default: /tmp/katla_screenshot.png)
    #[arg(long, value_name = "PATH")]
    screenshot: Option<String>,

    /// UI test mode: capture multiple screenshots at different UI states into <DIR>
    /// Implies --headless and --single-frame (100 frames)
    #[arg(long, value_name = "DIR")]
    ui_test: Option<String>,

    /// Editor camera pose as yaw_deg,pitch_deg,distance (diagnostic, headless captures)
    #[arg(long, value_name = "YAW,PITCH,DIST", default_value = None)]
    camera: Option<String>,
}

fn main() {
    let args = Args::parse();

    info!("Katla 3D Engine starting...");
    if args.headless {
        info!("Running in headless mode (offscreen rendering)");
    }
    if args.single_frame {
        info!("Running in limited-frame mode (100 frames) for validation testing");
    }
    if args.gpu_validation {
        info!("GPU-assisted validation enabled");
    }
    if args.check_black_frames {
        info!("Black frame detection enabled - will check center pixel color");
    }
    if let Some(ref path) = args.screenshot {
        info!("Screenshot will be saved to: {}", path);
    }
    if let Some(ref dir) = args.ui_test {
        info!("UI test mode: screenshots will be saved to: {}", dir);
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
        .with_system(Box::new(RapierPhysicsSystem), SystemExecutionOrder::NORMAL)
        .with_system(
            Box::new(
                ScriptSystem::new()
                    .expect("failed to create script system")
                    .with_scripts_dir("resources/scripts")
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
                    })
                    .with_component_entities_provider(|world| {
                        let mut map = std::collections::HashMap::new();
                        macro_rules! register {
                            ($ty:ty) => {
                                let ids: Vec<_> =
                                    world.query_ref::<&$ty>().map(|(id, _)| id).collect();
                                if !ids.is_empty() {
                                    map.insert(
                                        std::any::type_name::<$ty>()
                                            .rsplit("::")
                                            .next()
                                            .unwrap()
                                            .to_string(),
                                        ids,
                                    );
                                }
                            };
                        }
                        register!(katla_app::components::transform::TransformComponent);
                        register!(katla_app::components::transform::WorldTransform);
                        register!(katla_app::components::physics::VelocityComponent);
                        register!(katla_app::components::scene::NameComponent);
                        register!(katla_app::components::rendering::DrawableComponent);
                        register!(katla_app::components::rendering::BillboardComponent);
                        register!(katla_app::components::rendering::DirectionalLight);
                        register!(katla_app::components::rendering::PointLight);
                        register!(katla_app::components::camera::PerspectiveComponent);
                        register!(katla_app::components::camera::FlyCameraControllerComponent);
                        register!(katla_app::components::camera::FlyCameraLookComponent);
                        register!(katla_app::components::camera::OrbitCameraControllerComponent);
                        register!(katla_app::components::scene::Children);
                        register!(katla_app::components::scene::Parent);
                        register!(katla_script::ScriptComponent);
                        map
                    }),
            ),
            SystemExecutionOrder::LATE,
        );

    let mut builder = builder
        .with_name("Katla")
        .validation_layer(true)
        .gpu_assisted_validation(args.gpu_validation)
        .check_black_frames(args.check_black_frames);

    if let Some(scene) = &args.scene {
        builder = builder.with_scene_path(scene);
        info!("Loading scene: {}", scene);
    }

    // --headless: offscreen rendering, no window
    if args.headless || args.ui_test.is_some() {
        builder = builder.headless(true);
    }

    // Screenshot output path (only meaningful in headless mode)
    if let Some(ref path) = args.screenshot {
        builder = builder.screenshot_path(path.clone());
    }

    // UI test mode implies single-frame (100 frames)
    if args.single_frame || args.ui_test.is_some() {
        builder = builder.max_frames(100);
    }

    if args.dump_layout {
        builder = builder.max_frames(3).dump_layout_to_stdout();
        info!("Layout dump mode: will print widget tree after first frame");
    }

    if let Some(ref path) = args.dump_layout_file {
        builder = builder.max_frames(3).dump_layout_to_file(path.clone());
        info!("Layout dump mode: will write widget tree to {}", path);
    }

    if args.headless || args.ui_test.is_some() {
        if let Some(ref dir) = args.ui_test {
            builder = builder.ui_test_path(dir.clone());
        }
        let screenshot_path = args
            .screenshot
            .unwrap_or_else(|| "/tmp/katla_screenshot.png".to_string());
        let max_frames = if args.single_frame || args.ui_test.is_some() {
            100
        } else {
            10
        };

        let result = builder.build_headless(max_frames, screenshot_path);
        match result {
            Ok(mut app) => {
                if let Err(e) = app.init() {
                    error!("Application init failed: {e}");
                    std::process::exit(1);
                }
                if let Some(camera) = parse_camera_pose(&args.camera) {
                    let (yaw_deg, pitch_deg, distance) = camera;
                    app.set_editor_camera_pose(
                        yaw_deg.to_radians(),
                        pitch_deg.to_radians(),
                        distance,
                    );
                }
                if let Err(e) = app.run_headless() {
                    error!("Headless render failed: {e}");
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("Failed to initialize headless application: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        let result = builder.build();

        match result {
            Ok((mut application, event_loop)) => {
                if let Err(e) = application.init() {
                    error!("Application init failed: {e}");
                    std::process::exit(1);
                }
                info!("About to enter event loop");
                if let Err(e) = event_loop.run_app(&mut application) {
                    error!("Event loop error: {e}");
                }
                info!("Event loop exited");
            }
            Err(e) => {
                eprintln!("Failed to initialize application: {}", e);
                std::process::exit(1);
            }
        }
    }
}

/// Parse the --camera diagnostic value "yaw_deg,pitch_deg,distance".
fn parse_camera_pose(value: &Option<String>) -> Option<(f32, f32, f32)> {
    let value = value.as_deref()?;
    let parts: Vec<&str> = value.split(',').collect();
    if parts.len() != 3 {
        error!("--camera expects yaw_deg,pitch_deg,distance (got '{value}')");
        std::process::exit(1);
    }
    let (yaw, pitch, distance) = (
        parts[0].trim().parse::<f32>(),
        parts[1].trim().parse::<f32>(),
        parts[2].trim().parse::<f32>(),
    );
    match (yaw, pitch, distance) {
        (Ok(yaw), Ok(pitch), Ok(distance)) => Some((yaw, pitch, distance)),
        _ => {
            error!("--camera expects numeric yaw_deg,pitch_deg,distance (got '{value}')");
            std::process::exit(1);
        }
    }
}
