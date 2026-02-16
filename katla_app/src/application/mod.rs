pub mod builder;

use std::{cell::RefCell, collections::HashMap, ffi::CString, rc::Rc, time::Instant};

use log::{error, info, warn};
use winit::keyboard::ModifiersState;

pub use builder::*;
use katla_ecs::{input::Action, World};
use katla_math::{Transform, Vec2, Vec3};
use katla_vulkan::{MaterialRegistry, SkeletonBuffer, VulkanRenderer};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{DeviceEvent, DeviceId, ElementState, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use crate::{
    animation::{AnimationManager, Skeleton},
    components::{DirectionalLight, DrawableComponent, PointLight, TransformComponent},
    entities::{Camera, Model},
    input::{InputBinding, InputMapper, KeyCombo, MouseCombo},
    rendering::{
        create_checkerboard_material, create_checkerboard_texture, MaterialManager, MeshBuilder,
        SkyMaterial,
    },
    resources::ResourceManager,
    util::{FileCache, GLTFModel, Timer},
};
use katla_ecs::System;

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
    resources: ResourceManager, // Centralized resource paths
    /// Skeleton buffers for animated meshes, indexed by entity ID
    skeleton_buffers: HashMap<katla_ecs::EntityId, Rc<RefCell<SkeletonBuffer>>>,
    /// Immediate mode UI context
    ui_context: katla_ui::UiContext,
    /// Debug overlay UI (simplified stats)
    debug_overlay: crate::ui::DebugOverlay,
    /// Game engine editor UI
    editor_ui: crate::ui::EditorUI,
    /// Use editor UI mode (vs simple debug overlay)
    use_editor_ui: bool,
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

            // Initialize storage uniform system for modern rendering
            // This enables storage buffers with instance indexing
            renderer.init_storage_standard()
                .expect("Failed to initialize storage uniform system");

            // Load materials from TOML files with storage buffer mode
            // This creates pipelines with two-set layout (uniforms + textures)
            let loaded_count = renderer
                .material_registry
                .borrow_mut()
                .load_directory_storage(
                    &self.resources.materials,
                    renderer.context.clone()
                )
                .expect("Failed to load materials directory");
            info!(
                "Loaded {} material templates from {}",
                loaded_count,
                self.resources.materials.display()
            );

            // Enable hot reload for materials and shaders
            // Watch the parent resources directory to catch changes in both materials/ and shaders/
            renderer
                .material_registry
                .borrow_mut()
                .enable_hot_reload(&self.resources.root, 100)
                .expect("Failed to enable hot reload");
            info!("Hot reload enabled for materials and shaders");

            // Now find and load the Fox model (after templates are loaded)
            let fox_path = self.resources.model_path("Fox.glb");

            let fox_transform = Transform::new_from_position(Vec3::new(0.0, 5.0, 0.0))
                .with_scale(Vec3::new(0.05, 0.05, 0.05)); // Fox model is huge, scale it down
            let context = renderer.context.clone();
            let fox_model = self.gltf_cache.read(fox_path);

            // Create the skinned model entity using the gltf_skinned template
            // Fox has skeletal animation, so we need skinned mesh + shader
            // Get raw pointer to material registry for skinned model creation
            let material_registry_ptr: *const std::cell::RefCell<MaterialRegistry> =
                &renderer.material_registry;
            let fox = Model::new_skinned_from_gltf_with_ptr(
                &mut self.world,
                fox_model.clone(),
                context,
                Some(&mut renderer),
                fox_transform,
                material_registry_ptr,
            );

            info!("Fox model entity: {:?} loaded, setting up animation...", fox.entity);

            // Setup animation components for the fox model
            AnimationManager::setup_animated_model(
                &mut self.world,
                fox.entity,
                &fox_model,
                Some("Run"), // Play "Walk" animation by default
            );

            info!("Fox model entity: {:?} with animation setup complete", fox.entity);

            // Setup GPU skeleton buffer for the fox model
            if let Some(skeleton) = self.world.get_component::<Skeleton>(fox.entity) {
                let joint_count = skeleton.joint_transforms.len();
                info!("Fox has {} joints, creating skeleton buffer", joint_count);

                // Create skeleton buffer
                let skeleton_buffer = Rc::new(RefCell::new(SkeletonBuffer::new(
                    renderer.context.clone(),
                    joint_count,
                )));

                // Get skeleton_set_layout from the material's pipeline
                if let Some(drawable) = self.world.get_component::<DrawableComponent>(fox.entity) {
                    if let Some(material_handle) = drawable.material_handle {
                        if let Some(skeleton_layout) = renderer.asset_registry.get_skeleton_set_layout(material_handle) {
                            // Register skeleton with renderer
                            if let Some(skeleton_handle) = renderer.register_skeleton(
                                skeleton_buffer.clone(),
                                skeleton_layout,
                            ) {
                                info!("Registered skeleton with handle {:?}", skeleton_handle);

                                // Set handle on DrawableComponent
                                if let Some(drawable) = self.world.get_component_mut::<DrawableComponent>(fox.entity) {
                                    drawable.skeleton_handle = Some(skeleton_handle);
                                }

                                // Store buffer for later uploads
                                self.skeleton_buffers.insert(fox.entity, skeleton_buffer);
                            } else {
                                warn!("Failed to register skeleton with renderer");
                            }
                        } else {
                            warn!("Material does not have skeleton_set_layout (not a skinned material?)");
                        }
                    }
                }
            } else {
                warn!("Fox entity has no Skeleton component");
            }

            // Create meshes spaced out around the scene

            // Cube moved away from fox (fox is near origin)
            let _cube = MeshBuilder::new(renderer.context.clone())
                .position(Vec3::new(15.0, 5.0, -15.0))
                .color([1.0, 0.3, 0.3]) // Red tint
                .with_shared_material("Checkerboard")
                .build(&mut self.world, &mut renderer);

            let _sphere = MeshBuilder::new(renderer.context.clone())
                .position(Vec3::new(30.0, 5.0, 0.0))
                .color([0.3, 1.0, 0.3]) // Green tint
                .with_shared_material("Checkerboard")
                .sphere()
                .build(&mut self.world, &mut renderer);

            let _cylinder = MeshBuilder::new(renderer.context.clone())
                .position(Vec3::new(-30.0, 5.0, 0.0))
                .color([0.3, 0.3, 1.0]) // Blue tint
                .with_shared_material("Checkerboard")
                .cylinder()
                .build(&mut self.world, &mut renderer);

            let _plane = MeshBuilder::new(renderer.context.clone())
                .position(Vec3::new(0.0, -5.0, 0.0))
                .color([0.8, 0.8, 0.8]) // Gray tint
                .with_shared_material("Checkerboard")
                .plane()
                .size(Vec3::new(10.0, 10.0, 1.0))
                .build(&mut self.world, &mut renderer);

            let _torus = MeshBuilder::new(renderer.context.clone())
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

            // Create particle emitter
            let _particle_emitter = crate::entities::create_particle_emitter(
                &mut self.world,
                renderer.context.clone(),
                Vec3::new(0.0, 10.0, 0.0), // Above the fox
                100.0,                       // 100 particles per second
            );
            info!("Created particle emitter entity");

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
                info!("Registered checkerboard material from template");
            } else {
                info!("Warning: Checkerboard template not found, using fallback");
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

            // Pass mouse button to UI
            let ui_button = match button {
                winit::event::MouseButton::Left => Some(katla_ui::input::mouse_button::LEFT),
                winit::event::MouseButton::Right => Some(katla_ui::input::mouse_button::RIGHT),
                winit::event::MouseButton::Middle => Some(katla_ui::input::mouse_button::MIDDLE),
                _ => None,
            };
            if let Some(btn) = ui_button {
                let pressed = matches!(state, ElementState::Pressed);
                self.ui_context.input.set_mouse_button(btn, pressed);
            }
        }

        if let Some(_renderer) = &mut self.renderer {
            match event {
                WindowEvent::Resized(logical_size) => {
                    let new_width = logical_size.width;
                    let new_height = logical_size.height as f32;

                    if new_width > 0 && new_height > 0.0 {
                        if let Some(ref mut renderer) = self.renderer {
                            info!(
                                "=== Window resized to {}x{}, recreating swapchain ===",
                                new_width, new_height as u32
                            );
                            // Pass the actual window size to ensure swapchain uses correct extent
                            renderer.recreate_swapchain();
                            // Also resize viewport render target
                            let _ = renderer.init_viewport_target(new_width as u32, new_height as u32);
                            // Resize output render target for UI composition
                            let _ = renderer.init_output_target(new_width as u32, new_height as u32);
                            // Rebuild render graph to update resource references
                            renderer.setup_render_graph();

                            // Update camera aspect ratio based on viewport texture size
                            if let Some(viewport_extent) = renderer.viewport_extent() {
                                let aspect = viewport_extent.width as f32 / viewport_extent.height as f32;
                                self.camera
                                    .borrow_mut()
                                    .aspect_ratio_changed(&mut self.world, aspect);
                            }
                        }
                    }
                }
                WindowEvent::CursorMoved { position, .. } => {
                    // Update UI mouse position
                    self.ui_context.input.set_mouse_pos(Vec2::new(
                        position.x as f32,
                        position.y as f32,
                    ));
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    // Update UI scroll delta
                    let scroll = match delta {
                        winit::event::MouseScrollDelta::LineDelta(x, y) => {
                            Vec2::new(x * 20.0, y * 20.0) // Scale for UI
                        }
                        winit::event::MouseScrollDelta::PixelDelta(pos) => {
                            Vec2::new(pos.x as f32, pos.y as f32)
                        }
                    };
                    self.ui_context.input.scroll_delta = scroll;
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

                        // Pass keyboard input to UI
                        let ui_key = Self::winit_to_ui_key(keycode);
                        if let Some(key) = ui_key {
                            match event.state {
                                ElementState::Pressed => {
                                    self.ui_context.input.add_key_press(key);
                                }
                                ElementState::Released => {
                                    self.ui_context.input.add_key_release(key);
                                }
                            }
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
                // Note: ReceivedCharacter was removed in winit 0.30
                // Text input handling will need to be added later if needed
                WindowEvent::RedrawRequested => {
                    self.timer.add_timestamp();

                    let dt = self.timer.get_delta() as f32;

                    // Update world (runs animation systems)
                    self.world.update(dt);

                    // Upload skeleton transforms to GPU buffers
                    self.upload_skeleton_transforms();

                    // Render using render graph
                    self.render_with_render_graph();

                    // Render debug UI overlay
                    self.render_debug_ui(dt);

                    // Handle max_frames limit: exit after rendering specified number of frames
                    if let Some(max) = self.info.max_frames {
                        self.frame_count += 1;
                        if self.frame_count >= max {
                            info!("Rendered {} frames, exiting", self.frame_count);
                            event_loop.exit();
                        }
                    }

                    if self.stage_upload {
                        let start = Instant::now();
                        let renderer = self.renderer.as_mut().expect("Renderer not initialized");

                        let _sphere = MeshBuilder::new(renderer.context.clone())
                                        .position(Vec3::new(0.0, 5.0, 0.0))
                            .color([0.8, 0.2, 0.2])
                            .with_shared_material("Checkerboard")
                            .sphere()
                            .build(&mut self.world, renderer);

                        let renderer = self.renderer.as_mut().expect("Renderer not initialized");
                        let _cube = MeshBuilder::new(renderer.context.clone())
                                        .position(Vec3::new(20.0, 5.0, 0.0))
                            .color([0.2, 0.8, 0.2])
                            .with_shared_material("Checkerboard")
                            .build(&mut self.world, renderer);

                        let renderer = self.renderer.as_mut().expect("Renderer not initialized");
                        let _plane = MeshBuilder::new(renderer.context.clone())
                                        .position(Vec3::new(0.0, -5.0, 0.0))
                            .color([0.5, 0.5, 0.5])
                            .with_shared_material("Checkerboard")
                            .plane()
                            .size(Vec3::new(100.0, 100.0, 1.0))
                            .build(&mut self.world, renderer);

                        let millisecs = start.elapsed().as_micros() as f64 / 1000.0;

                        info!("Mesh creation took {millisecs} ms");
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
        // Logger is now initialized in main() before building the application
    }

    /// Convert winit KeyCode to UI KeyCode.
    fn winit_to_ui_key(keycode: KeyCode) -> Option<katla_ui::input::KeyCode> {
        use katla_ui::input::KeyCode as UiKey;
        Some(match keycode {
            KeyCode::ShiftLeft | KeyCode::ShiftRight => UiKey::Shift,
            KeyCode::ControlLeft | KeyCode::ControlRight => UiKey::Control,
            KeyCode::AltLeft | KeyCode::AltRight => UiKey::Alt,
            KeyCode::SuperLeft | KeyCode::SuperRight => UiKey::Super,
            KeyCode::Tab => UiKey::Tab,
            KeyCode::ArrowLeft => UiKey::ArrowLeft,
            KeyCode::ArrowRight => UiKey::ArrowRight,
            KeyCode::ArrowUp => UiKey::ArrowUp,
            KeyCode::ArrowDown => UiKey::ArrowDown,
            KeyCode::Home => UiKey::Home,
            KeyCode::End => UiKey::End,
            KeyCode::PageUp => UiKey::PageUp,
            KeyCode::PageDown => UiKey::PageDown,
            KeyCode::Enter | KeyCode::NumpadEnter => UiKey::Enter,
            KeyCode::Escape => UiKey::Escape,
            KeyCode::Backspace => UiKey::Backspace,
            KeyCode::Delete => UiKey::Delete,
            KeyCode::Insert => UiKey::Insert,
            KeyCode::Space => UiKey::Space,
            KeyCode::KeyA => UiKey::A,
            _ => return None,
        })
    }

    /// Setup the render graph with multiple framebuffers (one per swapchain image).
    /// This creates the graph upfront during initialization to avoid
    /// destroying Vulkan objects while the GPU is still using them.
    fn setup_render_graph(&mut self) {
        let renderer = match self.renderer.as_mut() {
            Some(r) => r,
            None => return,
        };

        // Create and set up sky material for procedural sky background
        let sky_material = SkyMaterial::new(renderer.context.clone());
        renderer.set_sky_pipeline(sky_material.pipeline());

        // Create and set up UI material for overlay rendering
        let ui_material = crate::rendering::UiMaterial::new(renderer.context.clone());
        renderer.set_ui_pipeline(ui_material.pipeline());

        // Initialize UI buffers (256KB vertex, 128KB index - enough for complex UIs)
        renderer.init_ui_buffers(256 * 1024, 128 * 1024);

        // Initialize UI textures (512x512 font atlas)
        renderer.init_ui_textures(512, 512)
            .expect("Failed to initialize UI textures");

        // Initialize viewport render target for game engine editor
        // This creates an offscreen texture the UI can sample for the viewport panel
        let viewport_size = self.window.as_ref().unwrap().inner_size();
        renderer.init_viewport_target(viewport_size.width, viewport_size.height)
            .expect("Failed to initialize viewport render target");

        // Initialize output render target for final UI composition
        // This is where UI renders, then present_pass copies to swapchain
        renderer.init_output_target(viewport_size.width, viewport_size.height)
            .expect("Failed to initialize output render target");

        // Set camera aspect ratio based on viewport texture size (not window size!)
        if let Some(viewport_extent) = renderer.viewport_extent() {
            let aspect = viewport_extent.width as f32 / viewport_extent.height as f32;
            self.camera
                .borrow_mut()
                .aspect_ratio_changed(&mut self.world, aspect);
        }

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

        // Compute inverse view-projection matrix for camera-relative sky
        // VP = proj * view, then inv_VP = VP.inverse()
        let view_proj = &proj * &view;
        let inv_view_proj = view_proj.inverse();

        // Extract camera position from inverse view matrix
        let view_arr: [[f32; 4]; 4] = view.clone().into();
        let cam_x = -(view_arr[0][0]*view_arr[3][0] + view_arr[0][1]*view_arr[3][1] + view_arr[0][2]*view_arr[3][2]);
        let cam_y = -(view_arr[1][0]*view_arr[3][0] + view_arr[1][1]*view_arr[3][1] + view_arr[1][2]*view_arr[3][2]);
        let cam_z = -(view_arr[2][0]*view_arr[3][0] + view_arr[2][1]*view_arr[3][1] + view_arr[2][2]*view_arr[3][2]);

        // Set frame uniforms once per frame (view/proj/lighting shared by all draws)
        renderer.set_frame_uniforms(katla_vulkan::FrameUniforms {
            view_matrix: view.to_array(),
            proj_matrix: proj.to_array(),
            inv_view_proj_matrix: inv_view_proj.to_array(),
            camera_position: [cam_x, cam_y, cam_z, 0.0],
            light_direction: [-0.3, -1.0, -0.2, 0.0],
            light_color: [1.0, 0.95, 0.9, 0.0],
            light_intensity: 1.5,
        });

        // Check for material hot reload
        if let Ok(reloaded) = renderer
            .material_registry
            .borrow_mut()
            .check_hot_reload(renderer.context.clone())
        {
            if reloaded > 0 {
                info!("Hot reloaded {} material template(s)", reloaded);
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
            // Get the model matrix and convert to array
            let model_matrix = transform.transform.make_mat4();
            let model_array: [f32; 16] = model_matrix.to_array();

            if let (Some(mesh_handle), Some(material_handle)) =
                (drawable.mesh_handle, drawable.material_handle)
            {
                let mut draw_call = DrawCall::new(mesh_handle, material_handle)
                    .with_transform(model_array);

                // Add color override if specified in DrawableComponent
                if let Some(color) = drawable.color {
                    draw_call = draw_call.with_color(color.to_array());
                }

                // Add skeleton handle for GPU skeletal animation
                if let Some(skeleton) = drawable.skeleton_handle {
                    draw_call = draw_call.with_skeleton(skeleton);
                }

                draw_list.push(draw_call);
            } else {
                // Entity missing mesh or material handle - skip
            }
        }

        // Collect particle dispatches and renders from particle emitters
        use katla_vulkan::{ParticleDispatch, ParticleRender};
        let delta_time = self.timer.get_delta() as f32;

        // Get storage descriptor for frame uniforms (before mutable borrow)
        let storage_descriptor = renderer.storage_descriptor_set.as_ref().map(|s| s.set());

        for (_entity, emitter) in self.world.query::<&mut crate::components::ParticleEmitter>() {
            // Update emitter (updates frame data buffer)
            emitter.update(delta_time);

            // Add compute dispatch for particle simulation
            let dispatch = ParticleDispatch {
                pipeline: emitter.compute_pipeline(),
                pipeline_layout: emitter.compute_layout(),
                descriptor_set: emitter.compute_descriptor(),
                frame_data: [0.0; 4], // Frame data is in uniform buffer
                workgroup_count: emitter.workgroup_count(),
            };
            draw_list.push_particle(dispatch);

            // Add particle render if we have the storage descriptor
            if let Some(frame_descriptor) = storage_descriptor {
                let render = ParticleRender {
                    pipeline: emitter.render_pipeline(),
                    pipeline_layout: emitter.render_layout(),
                    frame_descriptor_set: frame_descriptor,
                    particle_descriptor_set: emitter.render_particle_descriptor(),
                    particle_count: emitter.particle_count(),
                };
                draw_list.push_particle_render(render);
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
                    error!("Render frame failed: {:?}", e);
                }
            }
        }
    }

    /// Upload skeleton joint transforms to GPU buffers.
    ///
    /// This is called after world.update() which runs SkeletalAnimationSystem
    /// to compute the joint transforms.
    fn upload_skeleton_transforms(&mut self) {
        // Convert Mat4 to GPU-friendly [[f32; 4]; 4] format
        fn mat4_to_array(matrix: &katla_math::Mat4) -> [[f32; 4]; 4] {
            let data: [[f32; 4]; 4] = matrix.clone().into();
            data
        }

        // For each entity with a stored skeleton buffer, upload the transforms
        for (entity, buffer) in &self.skeleton_buffers {
            if let Some(skeleton) = self.world.get_component::<Skeleton>(*entity) {
                // Convert Mat4 joint transforms to GPU format
                let joint_matrices: Vec<[[f32; 4]; 4]> = skeleton
                    .joint_transforms
                    .iter()
                    .map(mat4_to_array)
                    .collect();

                // Upload to GPU
                buffer.borrow_mut().upload(&joint_matrices);
            }
        }
    }

    /// Render debug UI overlay with stats and controls.
    fn render_debug_ui(&mut self, dt: f32) {
        let screen_size = if let Some(ref window) = self.window {
            let size = window.inner_size();
            Vec2::new(size.width as f32, size.height as f32)
        } else {
            Vec2::new(1920.0, 1080.0)
        };

        // Calculate stats
        let fps = if dt > 0.0 { 1.0 / dt } else { 0.0 };
        let entity_count = self.world.entity_count();

        // Collect entity info for editor UI
        let entity_info = self.collect_entity_info();

        // Render UI (editor or debug overlay based on mode)
        // We extract the vertices immediately to release the borrow on editor_ui
        let (vertices, indices, use_editor) = if self.use_editor_ui {
            let draw_list = self.editor_ui.render(
                &mut self.ui_context,
                screen_size,
                &entity_info,
                fps,
                self.frame_count,
            );
            (draw_list.vertices.clone(), draw_list.indices.clone(), true)
        } else {
            let draw_list = self.debug_overlay.render(
                &mut self.ui_context,
                screen_size,
                fps,
                self.frame_count,
                entity_count,
            );
            (draw_list.vertices.clone(), draw_list.indices.clone(), false)
        };

        // Extract editor actions (safe now since editor_ui borrow is released)
        let editor_actions = if use_editor {
            self.editor_ui.take_actions()
        } else {
            Vec::new()
        };

        // Process editor actions
        for action in editor_actions {
            use crate::ui::EditorAction;
            match action {
                EditorAction::SpawnModel(model_type, position) => {
                    self.spawn_model_from_editor(model_type, position);
                }
                EditorAction::DeleteEntity(entity_id) => {
                    self.world.destroy_entity(entity_id);
                    info!("Deleted entity {:?}", entity_id);
                }
                EditorAction::SelectEntity(entity_id) => {
                    info!("Selected entity {:?}", entity_id);
                }
                EditorAction::MoveEntity(_entity_id, _position) => {
                    // TODO: Implement entity moving
                }
                EditorAction::TogglePlay => {
                    info!("Toggle play mode");
                }
            }
        }

        // Pass UI data to renderer if we have data and a renderer
        if !vertices.is_empty() {
            use crate::rendering::ui_material::UiShaderVertex;

            // Transform vertices from screen space to NDC
            // Screen: (0,0) = top-left, Y increases downward
            // Standard viewport: NDC y=-1 is top, y=+1 is bottom
            let shader_vertices: Vec<UiShaderVertex> = vertices
                .iter()
                .map(|v| {
                    let ndc_x = (v.position.x() / screen_size.x()) * 2.0 - 1.0;
                    let ndc_y = (v.position.y() / screen_size.y()) * 2.0 - 1.0;

                    UiShaderVertex::new(
                        [ndc_x, ndc_y],
                        [v.uv.x(), v.uv.y()],
                        [v.color.r, v.color.g, v.color.b, v.color.a],
                    )
                })
                .collect();

            // Convert vertices to raw bytes
            let vertex_bytes = unsafe {
                std::slice::from_raw_parts(
                    shader_vertices.as_ptr() as *const u8,
                    shader_vertices.len() * std::mem::size_of::<UiShaderVertex>(),
                )
            }.to_vec();

            // Convert indices to raw bytes
            let index_bytes = unsafe {
                std::slice::from_raw_parts(
                    indices.as_ptr() as *const u8,
                    indices.len() * std::mem::size_of::<u32>(),
                )
            }.to_vec();

            // Pass to renderer
            if let Some(ref renderer) = self.renderer {
                renderer.set_ui_data(
                    vertex_bytes,
                    index_bytes,
                    [screen_size.x(), screen_size.y()],
                );
            }
        }

        // Update font atlas texture if needed (render may have added new glyphs)
        if self.ui_context.fonts.atlas_needs_update() {
            if let Some(ref mut renderer) = self.renderer {
                let atlas_data = self.ui_context.fonts.atlas_data().to_vec();
                renderer.update_font_atlas(&atlas_data);
            }
            self.ui_context.fonts.mark_atlas_updated();
        }

        // Clear input state for next frame
        self.ui_context.input.clear_frame_state();
    }

    /// Collect entity information for the editor UI.
    fn collect_entity_info(&self) -> Vec<crate::ui::EntityInfo> {
        use crate::components::{NameComponent, TransformComponent, DrawableComponent};

        let mut entities = Vec::new();

        // Query all entities with transforms
        for entity_id in self.world.entity_ids() {
            let transform = match self.world.get_component::<TransformComponent>(entity_id) {
                Some(t) => t,
                None => continue,
            };

            let name = self.world.get_component::<NameComponent>(entity_id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| format!("Entity {}", entity_id.id()));

            let pos = transform.transform.position;
            let euler = transform.transform.rotation.to_euler();
            let rot = Vec3::new(euler.0, euler.1, euler.2);
            let scale = transform.transform.scale;

            // Check if drawable
            let model_type = self.world.get_component::<DrawableComponent>(entity_id)
                .map(|_| "Mesh".to_string())
                .unwrap_or_else(|| "Empty".to_string());

            entities.push(crate::ui::EntityInfo {
                id: entity_id,
                name,
                position: pos,
                rotation: rot,
                scale,
                model_type,
            });
        }

        entities
    }

    /// Spawn a model from the editor UI.
    fn spawn_model_from_editor(&mut self, model_type: crate::ui::SpawnableModel, position: Vec3) {
        use crate::ui::SpawnableModel;
        use crate::rendering::MeshBuilder;

        let context = match &self.renderer {
            Some(r) => r.context.clone(),
            None => return,
        };

        let entity_id = self.world.create_entity();

        // Add name
        let name = format!("{}_{}", model_type.name(), entity_id.id());
        self.world.add_component(entity_id, crate::components::NameComponent::new(&name));

        // Add transform
        let transform = katla_math::Transform {
            position,
            rotation: katla_math::Quat::new(), // Identity quaternion
            scale: Vec3::new(1.0, 1.0, 1.0),
        };
        self.world.add_component(entity_id, crate::components::TransformComponent::new(transform));

        // Create mesh using MeshBuilder
        let builder = MeshBuilder::new(context.clone()).position(position);

        let spawned_id = match model_type {
            SpawnableModel::Fox => {
                info!("Spawning Fox at {:?} (using cube placeholder)", position);
                builder.cube().build(&mut self.world, self.renderer.as_mut().unwrap())
            }
            SpawnableModel::Cube => {
                builder.cube().build(&mut self.world, self.renderer.as_mut().unwrap())
            }
            SpawnableModel::Sphere => {
                builder.sphere().build(&mut self.world, self.renderer.as_mut().unwrap())
            }
            SpawnableModel::Cylinder => {
                builder.cylinder().build(&mut self.world, self.renderer.as_mut().unwrap())
            }
            SpawnableModel::Plane => {
                builder.plane().build(&mut self.world, self.renderer.as_mut().unwrap())
            }
            SpawnableModel::Torus => {
                builder.torus().build(&mut self.world, self.renderer.as_mut().unwrap())
            }
        };

        info!("Spawned {} (entity {}) at {:?}", model_type.name(), spawned_id.id(), position);
    }
}
