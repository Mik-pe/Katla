pub mod builder;
pub mod model;

use std::{cell::RefCell, ffi::CString, path::PathBuf, rc::Rc, time::Instant};

use winit::keyboard::ModifiersState;

pub use builder::*;
use env_logger::Env;
use katla_ecs::{input::Action, World};
use katla_math::{Transform, Vec3};
use katla_vulkan::VulkanRenderer;
pub use model::*;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{DeviceEvent, DeviceId, ElementState, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use crate::{
    entities::{create_model_entity, Camera},
    input::{InputBinding, InputMapper, KeyCombo, MouseCombo},
    rendering::{create_checkerboard_material, MaterialManager, MeshBuilder, ShaderRegistry},
    util::{FileCache, GLTFModel, Timer},
};

struct ApplicationInfo {
    name: String,
    validation_layer_enabled: bool,
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
}

impl ApplicationHandler for Application {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let window = event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title(&self.info.name)
                        .with_resizable(true)
                        .with_min_inner_size(LogicalSize {
                            width: 1.0,
                            height: 1.0,
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
            let model = Model::new_from_gltf(
                self.gltf_cache
                    .read(PathBuf::from("../resources/models/Fox.glb")),
                renderer.context.clone(),
                &renderer.render_pass,
            );
            let fox_transform = Transform::new_from_position(Vec3::new(0.0, 0.0, 0.0));
            create_model_entity(&mut self.world, model, Some(&mut renderer), fox_transform);

            // Create meshes spaced out in a line with different colors
            let _cube = MeshBuilder::new(renderer.context.clone())
                .position(Vec3::new(0.0, 5.0, 0.0))
                .color([1.0, 0.3, 0.3]) // Red tint
                .with_shared_material("checkerboard")
                .build(&mut self.world, &mut renderer);

            let _sphere = MeshBuilder::new(renderer.context.clone())
                .position(Vec3::new(30.0, 5.0, 0.0))
                .color([0.3, 1.0, 0.3]) // Green tint
                .with_shared_material("checkerboard")
                .sphere()
                .build(&mut self.world, &mut renderer);

            let _cylinder = MeshBuilder::new(renderer.context.clone())
                .position(Vec3::new(-30.0, 5.0, 0.0))
                .color([0.3, 0.3, 1.0]) // Blue tint
                .with_shared_material("checkerboard")
                .cylinder()
                .build(&mut self.world, &mut renderer);

            let _plane = MeshBuilder::new(renderer.context.clone())
                .position(Vec3::new(0.0, -5.0, 0.0))
                .color([0.8, 0.8, 0.8]) // Gray tint
                .with_shared_material("checkerboard")
                .plane()
                .size(Vec3::new(100.0, 100.0, 1.0))
                .build(&mut self.world, &mut renderer);

            let _torus = MeshBuilder::new(renderer.context.clone())
                .position(Vec3::new(0.0, 15.0, 0.0))
                .color([1.0, 0.8, 0.3]) // Yellow tint
                .with_shared_material("checkerboard")
                .torus()
                .build(&mut self.world, &mut renderer);

            self.window = Some(window);

            // Create shared materials
            let checkerboard = create_checkerboard_material(
                renderer.context.clone(),
                &renderer.render_pass,
                &ShaderRegistry::new(),
            );
            self.material_manager.register_material("checkerboard", checkerboard);

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
                self.world.get_input_mut().mouse_delta.x += delta.0 as f32;
                self.world.get_input_mut().mouse_delta.y += delta.1 as f32;
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

                    if self.stage_upload {
                        let start = Instant::now();
                        let renderer = self.renderer.as_mut().expect("Renderer not initialized");

                        let _sphere = MeshBuilder::new(renderer.context.clone())
                            .position(Vec3::new(0.0, 5.0, 0.0))
                            .color([0.8, 0.2, 0.2])
                            .with_shared_material("checkerboard")
                            .sphere()
                            .build(&mut self.world, renderer);

                        let renderer = self.renderer.as_mut().expect("Renderer not initialized");
                        let _cube = MeshBuilder::new(renderer.context.clone())
                            .position(Vec3::new(20.0, 5.0, 0.0))
                            .color([0.2, 0.8, 0.2])
                            .with_shared_material("checkerboard")
                            .build(&mut self.world, renderer);

                        let renderer = self.renderer.as_mut().expect("Renderer not initialized");
                        let _plane = MeshBuilder::new(renderer.context.clone())
                            .position(Vec3::new(0.0, -5.0, 0.0))
                            .color([0.5, 0.5, 0.5])
                            .with_shared_material("checkerboard")
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
        // Clean up material manager before destroying renderer
        // This destroys all MaterialPipelines which own Vulkan resources
        self.material_manager.destroy();

        if let Some(mut renderer) = self.renderer.take() {
            renderer.wait_for_device();
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
        let view = self.camera.borrow().get_view_mat(&self.world).clone().inverse();
        let proj = self.camera.borrow().get_proj_mat(&self.world).clone();

        // Build a draw list from ECS entities using the handle-based rendering system
        // This provides separation between high-level (DrawCall) and low-level (CommandBuffer) rendering
        use katla_vulkan::{DrawCall, DrawList};
        let mut draw_list = DrawList::new();

        // Query all drawable entities
        for (_entity, transform, drawable) in self
            .world
            .query::<(&crate::components::TransformComponent, &crate::components::DrawableComponent)>(
        )
        {
            // Get the model matrix
            let model_matrix = transform.transform.make_mat4();

            // Convert to katla_vulkan's Mat4 format
            let model_array: [f32; 16] = unsafe {
                std::mem::transmute_copy(&model_matrix)
            };

            // Convert view and proj matrices
            let view_array: [f32; 16] = unsafe { std::mem::transmute_copy(&view) };
            let proj_array: [f32; 16] = unsafe { std::mem::transmute_copy(&proj) };

            // TODO: Get mesh and material handles from DrawableComponent
            // For now, we skip this since handles aren't registered yet
            if let (Some(mesh_handle), Some(material_handle)) =
                (drawable.mesh_handle, drawable.material_handle)
            {
                let draw_call = DrawCall::new(mesh_handle, material_handle)
                    .with_matrices(
                        model_array,
                        view_array,
                        proj_array,
                    );

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
