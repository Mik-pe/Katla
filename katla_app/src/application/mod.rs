pub mod builder;

use std::{cell::RefCell, ffi::CString, path::PathBuf, rc::Rc, time::Instant};

use winit::keyboard::ModifiersState;

pub use builder::*;
use env_logger::Env;
use katla_ecs::{input::Action, World};
use katla_math::{Transform, Vec2, Vec3};
use katla_vulkan::VulkanRenderer;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{DeviceEvent, DeviceId, ElementState, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use crate::{
    components::{DirectionalLight, PointLight, TransformComponent},
    entities::{Camera, Model},
    input::{InputBinding, InputMapper, KeyCombo, MouseCombo},
    rendering::{
        create_checkerboard_material, create_checkerboard_texture, MaterialManager, MeshBuilder,
    },
    util::{FileCache, GLTFModel, Timer},
};

/// Find the resources directory by searching common locations
fn find_resources_path() -> PathBuf {
    // List of possible paths to check, in order of preference
    let possible_paths = vec![
        // Current directory (for running from workspace root)
        PathBuf::from("resources/models"),
        // Parent directory (for running from katla_app)
        PathBuf::from("../resources/models"),
        // Grandparent directory (for running from target/debug)
        PathBuf::from("../../resources/models"),
        // Absolute path using CARGO_MANIFEST_DIR (for tests)
        {
            let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            path.pop(); // Go up from katla_app to workspace root
            path.push("resources/models");
            path
        },
    ];

    for path in possible_paths {
        if path.exists() {
            println!("Found resources/models at: {}", path.display());
            return path;
        }
    }

    panic!("Failed to find resources/models directory!");
}

/// Find the materials directory by searching common locations
fn find_materials_path() -> PathBuf {
    // List of possible paths to check, in order of preference
    let possible_paths = vec![
        // Current directory (for running from workspace root)
        PathBuf::from("resources/materials"),
        // Parent directory (for running from katla_app)
        PathBuf::from("../resources/materials"),
        // Grandparent directory (for running from target/debug)
        PathBuf::from("../../resources/materials"),
        // Absolute path using CARGO_MANIFEST_DIR
        {
            let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            path.pop(); // Go up from katla_app to workspace root
            path.push("resources/materials");
            path
        },
    ];

    for path in possible_paths {
        if path.exists() {
            println!("Found resources/materials at: {}", path.display());
            return path;
        }
    }

    panic!("Failed to find resources/materials directory!");
}

/// Find the resources root directory (parent of materials, models, shaders)
fn find_resources_root_path() -> PathBuf {
    let materials_path = find_materials_path();
    let mut root = materials_path.clone();
    root.pop(); // Remove 'materials'
    root
}

struct ApplicationInfo {
    name: String,
    validation_layer_enabled: bool,
    max_frames: Option<usize>, // Some(n) = exit after n frames, None = run indefinitely
}

pub struct Application {
    window: Option<Window>,
    renderer: Option<VulkanRenderer>,
    camera: Rc<RefCell<Camera>>,
    gltf_cache: FileCache<GLTFModel>,
    material_manager: MaterialManager,
    stage_upload: bool,
    timer: Timer,
    info: ApplicationInfo,
    world: World,
    input_mapper: InputMapper,
    current_modifiers: ModifiersState,
    frame_count: usize, // Track frames rendered for max_frames limit
}

impl ApplicationHandler for Application {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let window = event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title(&self.info.name)
                        .with_resizable(true)
                        // Use primary monitor size for initial window size
                        .with_min_inner_size(LogicalSize {
                            width: 800.0,  // Use reasonable default width
                            height: 600.0,  // Use reasonable default height
                        })
                        .with_maximized(false),
                )
                .unwrap();

            let engine_name = CString::new("Katla Engine").unwrap();
            let mut renderer = VulkanRenderer::init(
                &event_loop,
                &window,
                self.info.validation_layer_enabled,
                CString::new(self.info.name.as_str()).unwrap(),
                engine_name,
            );
            let window_size = window.inner_size();
            let win_x = window_size.width as f32;
            let win_y = window_size.height as f32;
            self.camera
                .borrow_mut()
                .aspect_ratio_changed(&mut self.world, win_x / win_y);

            // Load materials from TOML files BEFORE creating models
            // This ensures templates are available when creating GLTF models and meshes
            let materials_path = find_materials_path();
            let loaded_count = renderer
                .material_registry
                .borrow_mut()
                .load_directory(
                    &materials_path,
                    renderer.context.clone(),
                    None, // Dynamic rendering: use VK_NULL_HANDLE for renderPass
                )
                .expect("Failed to load materials directory");
            println!(
                "Loaded {} material templates from {}",
                loaded_count,
                materials_path.display()
            );

            // Enable hot reload for materials and shaders
            // Watch the parent resources directory to catch changes in both materials/ and shaders/
            let resources_path = find_resources_root_path();
            renderer
                .material_registry
                .borrow_mut()
                .enable_hot_reload(&resources_path, 100)
                .expect("Failed to enable hot reload");
            println!("Hot reload enabled for materials and shaders");

            // Now find and load the Fox model (after templates are loaded)
            let models_path = find_resources_path();
            let fox_path = models_path.join("Fox.glb");

            let fox_transform = Transform::new_from_position(Vec3::new(0.0, 0.0, 0.0));
            let context = renderer.context.clone();
            let fox_model = self.gltf_cache.read(fox_path);

            // Create the model entity using the gltf_default template
            // We use the raw pointer approach similar to MeshBuilder
            let material_registry_ptr = &renderer.material_registry
                as *const std::cell::RefCell<katla_vulkan::MaterialRegistry>;

            Model::new_from_gltf_with_ptr(
                &mut self.world,
                fox_model,
                context,
                Some(&mut renderer),
                fox_transform,
                material_registry_ptr,
            );

            // Create meshes spaced out in a line with different colors
            // Get the raw pointer to material registry for mesh builders
            let material_registry_ptr = &renderer.material_registry
                as *const std::cell::RefCell<katla_vulkan::MaterialRegistry>;

            let _cube = MeshBuilder::new(renderer.context.clone())
                .with_material_registry_ptr(material_registry_ptr)
                .position(Vec3::new(0.0, 5.0, 0.0))
                .color([1.0, 0.3, 0.3]) // Red tint
                .with_shared_material("Checkerboard")
                .build(&mut self.world, &mut renderer);

            let _sphere = MeshBuilder::new(renderer.context.clone())
                .with_material_registry_ptr(material_registry_ptr)
                .position(Vec3::new(30.0, 5.0, 0.0))
                .color([0.3, 1.0, 0.3]) // Green tint
                .with_shared_material("Checkerboard")
                .sphere()
                .build(&mut self.world, &mut renderer);

            let _cylinder = MeshBuilder::new(renderer.context.clone())
                .with_material_registry_ptr(material_registry_ptr)
                .position(Vec3::new(-30.0, 5.0, 0.0))
                .color([0.3, 0.3, 1.0]) // Blue tint
                .with_shared_material("Checkerboard")
                .cylinder()
                .build(&mut self.world, &mut renderer);

            let _plane = MeshBuilder::new(renderer.context.clone())
                .with_material_registry_ptr(material_registry_ptr)
                .position(Vec3::new(0.0, -5.0, 0.0))
                .color([0.8, 0.8, 0.8]) // Gray tint
                .with_shared_material("Checkerboard")
                .plane()
                .size(Vec3::new(100.0, 100.0, 1.0))
                .build(&mut self.world, &mut renderer);

            let _torus = MeshBuilder::new(renderer.context.clone())
                .with_material_registry_ptr(material_registry_ptr)
                .position(Vec3::new(0.0, 15.0, 0.0))
                .color([1.0, 0.8, 0.3]) // Yellow tint
                .with_shared_material("Checkerboard")
                .torus()
                .build(&mut self.world, &mut renderer);

            // Add lighting to the scene
            // Directional light (sun)
            let sun_light = self.world.create_entity();
            self.world.add_component(
                sun_light,
                DirectionalLight::new(
                    Vec3::new(-0.3, -1.0, -0.2), // Angled down and to the side
                    [1.0, 0.95, 0.8],            // Warm white
                    1.0,                         // Full intensity
                ),
            );

            // Point lights for accent lighting
            let red_light = self.world.create_entity();
            self.world.add_component(
                red_light,
                TransformComponent {
                    transform: Transform::new_from_position(Vec3::new(10.0, 10.0, 10.0)),
                },
            );
            self.world.add_component(
                red_light,
                PointLight::new([1.0, 0.3, 0.3], 5.0, 20.0), // Red light, 5x intensity, 20 unit range
            );

            let blue_light = self.world.create_entity();
            self.world.add_component(
                blue_light,
                TransformComponent {
                    transform: Transform::new_from_position(Vec3::new(-10.0, 8.0, 10.0)),
                },
            );
            self.world.add_component(
                blue_light,
                PointLight::new([0.3, 0.5, 1.0], 4.0, 25.0), // Blue light, 4x intensity, 25 unit range
            );

            // Add ambient light resource
            self.world
                .insert_resource(crate::components::AmbientLight::gray(0.15)); // 15% gray ambient

            self.window = Some(window);

            // Create checkerboard material from template (template loaded from TOML)
            // The template has the pipeline and shader, we just add the procedural texture
            let checkerboard_texture = create_checkerboard_texture(renderer.context.clone());
            if self.material_manager.register_from_template(
                "Checkerboard",
                &renderer.material_registry.borrow(),
                Some(Rc::new(checkerboard_texture)),
                None,
            ).is_some() {
                println!("Registered checkerboard material from template");
            } else {
                println!("Warning: Checkerboard template not found, using fallback");
                // Fallback to direct creation if template doesn't exist
                let checkerboard =
                    create_checkerboard_material(renderer.context.clone());
                self.material_manager
                    .register_material("checkerboard", checkerboard);
            }

            // Store context reference in material manager for cleanup
            self.material_manager.set_context(renderer.context.clone());

            self.renderer = Some(renderer);

            // Setup render graph after renderer initialization
            self.setup_render_graph();
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta } = event {
            if self.world.get_input().is_action_pressed(Action::LookEnable) {
                let current_delta = self.world.get_input().mouse_delta;
                self.world.get_input_mut().mouse_delta = Vec2::new(
                    current_delta.x() + delta.0 as f32,
                    current_delta.y() + delta.1 as f32,
                );
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if let WindowEvent::MouseInput { state, button, .. } = &event {
            let mouse_combo = MouseCombo::with_modifiers(*button, self.current_modifiers);
            let binding = InputBinding::Mouse(mouse_combo);

            if let Some(action) = self.input_mapper.get_action(&binding) {
                let pressed = matches!(state, ElementState::Pressed);
                self.world.get_input_mut().set_action_state(action, pressed);
            }
        }

        if let Some(_renderer) = &mut self.renderer {
            match event {
                WindowEvent::Resized(logical_size) => {
                    let new_width = logical_size.width;
                    let new_height = logical_size.height as f32;

                    if new_width > 0 && new_height > 0.0 {
                        // Update camera aspect ratio
                        let win_x = logical_size.width as f32;
                        let win_y = logical_size.height as f32;
                        self.camera
                            .borrow_mut()
                            .aspect_ratio_changed(&mut self.world, win_x / win_y);

                        if let Some(ref mut renderer) = self.renderer {
                            println!(
                                "=== Window resized to {}x{}, recreating swapchain ===",
                                new_width, new_height as u32
                            );
                            // Pass the actual window size to ensure swapchain uses correct extent
                            renderer.recreate_swapchain();
                        }
                    }
                }
                WindowEvent::CloseRequested => {
                    event_loop.exit();
                }
                WindowEvent::KeyboardInput { event, .. } => {
                    if let PhysicalKey::Code(keycode) = event.physical_key {
                        let key_combo = KeyCombo::with_modifiers(keycode, self.current_modifiers);
                        let binding = InputBinding::Keyboard(key_combo);

                        if let Some(action) = self.input_mapper.get_action(&binding) {
                            let pressed = matches!(event.state, ElementState::Pressed);
                            self.world.get_input_mut().set_action_state(action, pressed);
                        }

                        if event.state == ElementState::Pressed {
                            match keycode {
                                KeyCode::Escape => {
                                    event_loop.exit();
                                }
                                KeyCode::KeyT => {
                                    self.stage_upload = true;
                                }
                                _ => {}
                            }
                        }
                    }
                }
                WindowEvent::ModifiersChanged(modifiers) => {
                    self.current_modifiers = modifiers.state();
                }
                WindowEvent::RedrawRequested => {
                    self.timer.add_timestamp();

                    let dt = self.timer.get_delta() as f32;

                    // Update world
                    self.world.update(dt);

                    // Render using render graph
                    self.render_with_render_graph();

                    // Handle max_frames limit: exit after rendering specified number of frames
                    if let Some(max) = self.info.max_frames {
                        self.frame_count += 1;
                        if self.frame_count >= max {
                            println!("Rendered {} frames, exiting", self.frame_count);
                            event_loop.exit();
                        }
                    }

                    if self.stage_upload {
                        let start = Instant::now();
                        let renderer = self.renderer.as_mut().expect("Renderer not initialized");
                        let material_registry_ptr = &renderer.material_registry
                            as *const std::cell::RefCell<katla_vulkan::MaterialRegistry>;

                        let _sphere = MeshBuilder::new(renderer.context.clone())
                            .with_material_registry_ptr(material_registry_ptr)
                            .position(Vec3::new(0.0, 5.0, 0.0))
                            .color([0.8, 0.2, 0.2])
                            .with_shared_material("Checkerboard")
                            .sphere()
                            .build(&mut self.world, renderer);

                        let renderer = self.renderer.as_mut().expect("Renderer not initialized");
                        let _cube = MeshBuilder::new(renderer.context.clone())
                            .with_material_registry_ptr(material_registry_ptr)
                            .position(Vec3::new(20.0, 5.0, 0.0))
                            .color([0.2, 0.8, 0.2])
                            .with_shared_material("Checkerboard")
                            .build(&mut self.world, renderer);

                        let renderer = self.renderer.as_mut().expect("Renderer not initialized");
                        let _plane = MeshBuilder::new(renderer.context.clone())
                            .with_material_registry_ptr(material_registry_ptr)
                            .position(Vec3::new(0.0, -5.0, 0.0))
                            .color([0.5, 0.5, 0.5])
                            .with_shared_material("Checkerboard")
                            .plane()
                            .size(Vec3::new(100.0, 100.0, 1.0))
                            .build(&mut self.world, renderer);

                        let millisecs = start.elapsed().as_micros() as f64 / 1000.0;

                        println!("Mesh creation took {millisecs} ms");
                        self.stage_upload = false;
                    }
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
                _ => {}
            }
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        // Wait for GPU to finish all work BEFORE destroying any Vulkan resources
        // This ensures no pipelines are in use when we destroy them
        if let Some(ref mut renderer) = self.renderer {
            renderer.wait_for_device();
        }

        // Now safe to destroy material manager (which destroys pipelines)
        self.material_manager.destroy();

        // Finally destroy the renderer
        if let Some(mut renderer) = self.renderer.take() {
            renderer.destroy();
        }
    }
}

impl Application {
    pub fn init(&mut self) {
        env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    }

    /// Setup the render graph with multiple framebuffers (one per swapchain image).
    /// This creates the graph upfront during initialization to avoid
    /// destroying Vulkan objects while the GPU is still using them.
    fn setup_render_graph(&mut self) {
        let renderer = match self.renderer.as_mut() {
            Some(r) => r,
            None => return,
        };

        // Setup render graph with multiple framebuffers
        renderer.setup_render_graph();
    }

    /// Render using the render graph system with draw call submission.
    fn render_with_render_graph(&mut self) {
        let renderer = match self.renderer.as_mut() {
            Some(r) => r,
            None => return,
        };

        // Get camera matrices
        let view = self
            .camera
            .borrow()
            .get_view_mat(&self.world)
            .clone()
            .inverse();
        let proj = self.camera.borrow().get_proj_mat(&self.world).clone();

        // Check for material hot reload
        if let Ok(reloaded) = renderer
            .material_registry
            .borrow_mut()
            .check_hot_reload(renderer.context.clone(), None)
        // Dynamic rendering: use VK_NULL_HANDLE
        {
            if reloaded > 0 {
                println!("Hot reloaded {} material template(s)", reloaded);
            }
        }

        // Build a draw list from ECS entities using the handle-based rendering system
        // This provides separation between high-level (DrawCall) and low-level (CommandBuffer) rendering
        use katla_vulkan::{DrawCall, DrawList};
        let mut draw_list = DrawList::new();

        // Query all drawable entities
        for (_entity, transform, drawable) in self.world.query::<(
            &crate::components::TransformComponent,
            &crate::components::DrawableComponent,
        )>() {
            // Get the model matrix
            let model_matrix = transform.transform.make_mat4();

            // Convert to katla_vulkan's Mat4 format
            let model_array: [f32; 16] = unsafe { std::mem::transmute_copy(&model_matrix) };

            // Convert view and proj matrices
            let view_array: [f32; 16] = unsafe { std::mem::transmute_copy(&view) };
            let proj_array: [f32; 16] = unsafe { std::mem::transmute_copy(&proj) };

            // TODO: Get mesh and material handles from DrawableComponent
            // For now, we skip this since handles aren't registered yet
            if let (Some(mesh_handle), Some(material_handle)) =
                (drawable.mesh_handle, drawable.material_handle)
            {
                let mut draw_call = DrawCall::new(mesh_handle, material_handle).with_matrices(
                    model_array,
                    view_array,
                    proj_array,
                );

                // Add color override if specified in DrawableComponent
                if let Some(color) = drawable.color {
                    draw_call.params.color = Some(color.to_array());
                }

                draw_list.push(draw_call);
            }
        }

        // Render using the draw list
        if let Err(e) = renderer.render_frame(draw_list) {
            match e {
                katla_vulkan::RenderGraphError::SwapchainOutOfDate => {
                    // Swapchain is out of date (e.g., window resize), skip this frame
                    // The swapchain will be recreated on the next frame
                }
                _ => {
                    eprintln!("Render frame failed: {:?}", e);
                }
            }
        }
    }
}
