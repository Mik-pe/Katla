pub mod builder;
pub mod model;

use std::{cell::RefCell, ffi::CString, path::PathBuf, rc::Rc, time::Instant};

pub use builder::*;
use env_logger::Env;
use katla_ecs::World;
use katla_math::Vec3;
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
    cameracontroller::{fpscontrol::FpsControl, Camera, CameraController},
    components::DrawableComponent,
    entities::ModelEntity,
    input::InputController,
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
    controller: Rc<RefCell<FpsControl>>,
    input_controller: InputController,
    gltf_cache: FileCache<GLTFModel>,
    stage_upload: bool,
    timer: Timer,
    info: ApplicationInfo,
    world: World,
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
            let renderer = VulkanRenderer::init(
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
                    .read(PathBuf::from("resources/models/Fox.glb")),
                renderer.context.clone(),
                //TODO: (mikpe) - should not have to send these when creating a mesh... The scene should be enough and "Mesh" should be a higher level abstraction
                &renderer.render_pass,
                Vec3::new(0.0, 0.0, 0.0),
            );
            ModelEntity::new(&mut self.world, model);

            self.window = Some(window);
            self.renderer = Some(renderer);
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        self.controller.borrow_mut().handle_device_event(&event);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        self.controller.borrow_mut().handle_window_event(&event);
        if let Some(renderer) = &mut self.renderer {
            self.input_controller.handle_event(&event);
            match event {
                WindowEvent::Resized(logical_size) => {
                    let win_x = logical_size.width as f32;
                    let win_y = logical_size.height as f32;
                    if win_x > 0.0 && win_y > 0.0 {
                        self.camera
                            .borrow_mut()
                            .aspect_ratio_changed(&mut self.world, win_x / win_y);

                        renderer.recreate_swapchain();
                    }
                }
                WindowEvent::CloseRequested => {
                    event_loop.exit();
                }
                WindowEvent::KeyboardInput { event, .. } => {
                    if event.state == ElementState::Pressed {
                        if let PhysicalKey::Code(keycode) = event.physical_key {
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
                WindowEvent::RedrawRequested => {
                    renderer.swap_frames();
                    self.timer.add_timestamp();

                    let dt = self.timer.get_delta() as f32;
                    self.controller.borrow_mut().tick_camera(
                        &self.camera.borrow(),
                        &mut self.world,
                        dt,
                    );

                    self.world.update(dt);
                    let view = self
                        .camera
                        .borrow()
                        .get_view_mat(&self.world)
                        .clone()
                        .inverse();
                    let proj = self.camera.borrow().get_proj_mat(&self.world).clone();

                    let command_buffer = renderer.get_commandbuffer_opaque_pass();
                    for (_, drawable) in self.world.query::<&mut DrawableComponent>() {
                        drawable.0.update(&view, &proj, dt);
                        drawable.0.draw(&command_buffer);
                    }
                    command_buffer.end_render_pass();
                    command_buffer.end_command();

                    renderer.submit_frame(vec![&command_buffer]);
                    if self.stage_upload {
                        let start = Instant::now();
                        let model = Model::new_from_gltf(
                            self.gltf_cache
                                .read(PathBuf::from("resources/models/Tiger.glb")),
                            renderer.context.clone(),
                            &renderer.render_pass,
                            Vec3::new(100.0, 0.0, 0.0),
                        );
                        ModelEntity::new(&mut self.world, model);
                        let millisecs = start.elapsed().as_micros() as f64 / 1000.0;

                        println!("Mesh new took {millisecs} ms");
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
        if let Some(mut renderer) = self.renderer.take() {
            renderer.wait_for_device();
            renderer.destroy();
        }
    }
}

impl Application {
    pub fn init(&mut self) {
        env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init();
    }
}
