pub mod model;
pub mod scene;

use std::{cell::RefCell, ffi::CString, path::PathBuf, rc::Rc, time::Instant};

use env_logger::Env;
use katla_ecs::{System, SystemExecutionOrder, World};
use katla_math::{Quat, Vec3};
use katla_vulkan::VulkanRenderer;
pub use model::*;
pub use scene::*;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{DeviceEvent, DeviceId, ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use crate::{
    cameracontroller::{self, fpscontrol::FpsControl, Camera, CameraController},
    components::{Drawable, TransformComponent},
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
    scene: Scene,
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
            self.camera.borrow_mut().aspect_ratio_changed(win_x / win_y);
            let model = Model::new_from_gltf(
                self.gltf_cache
                    .read(PathBuf::from("resources/models/Fox.glb")),
                renderer.context.clone(),
                //TODO: (mikpe) - should not have to send these when creating a mesh... The scene should be enough and "Mesh" should be a higher level abstraction
                &renderer.render_pass,
                Vec3::new(0.0, 0.0, 0.0),
            );
            // let bounds = model.bounds.clone();
            let entity = self.world.create_entity();
            let transform = TransformComponent::new(katla_math::Transform {
                position: Vec3([0.0, 0.0, 0.0]),
                scale: Vec3([1.0, 1.0, 1.0]),
                rotation: Quat::new(),
            });

            self.world.add_component(entity, transform);
            self.world.add_component(entity, Drawable(Box::new(model)));

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
                        self.camera.borrow_mut().aspect_ratio_changed(win_x / win_y);

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
                    let proj = self.camera.borrow().get_proj_mat().clone();
                    // self.scene.update(proj, &view.inverse(), dt);

                    let command_buffer = renderer.get_commandbuffer_opaque_pass();
                    if let Some(drawables) = self.world.storage_mut().get_storage_mut::<Drawable>()
                    {
                        for (_, drawable) in drawables.iter_mut() {
                            drawable.0.update(&view, &proj, dt);
                            drawable.0.draw(&command_buffer);
                        }
                        command_buffer.end_render_pass();
                        command_buffer.end_command();
                    }

                    renderer.submit_frame(vec![&command_buffer]);
                    if self.stage_upload {
                        let start = Instant::now();
                        let mesh = Model::new_from_gltf(
                            self.gltf_cache
                                .read(PathBuf::from("resources/models/Tiger.glb")),
                            renderer.context.clone(),
                            &renderer.render_pass,
                            Vec3::new(100.0, 0.0, 0.0),
                        );
                        let millisecs = start.elapsed().as_micros() as f64 / 1000.0;

                        println!("Mesh new took {millisecs} ms");
                        // offset -= 100.0;
                        let bounds = mesh.bounds.clone();
                        self.scene
                            .add_object(SceneObject::new(Box::new(mesh), bounds));
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
            self.scene.teardown();
            renderer.destroy();
        }
    }
}

impl Application {
    pub fn init(&mut self) {
        env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init();
    }

    // fn swap_frames(&mut self) {
    //     self.renderer.swap_frames();
    // }

    // fn frame(&mut self, delta_time: f32) {
    //     self.camera.borrow_mut().update(delta_time);

    //     self.scene.update(
    //         &self.camera.borrow().get_proj_mat(),
    //         &self.camera.borrow().get_view_mat().inverse(),
    //     );

    //     let command_buffer = self.renderer.get_commandbuffer_opaque_pass();
    //     self.scene.render(&command_buffer);
    //     self.renderer.submit_frame(vec![&command_buffer]);
    // }
}

#[derive(Default)]
pub struct ApplicationBuilder {
    app_name: String,
    validation_layer_enabled: bool,
    controller: Rc<RefCell<FpsControl>>,
    input_controller: InputController,
    world: World,
}

impl ApplicationBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.app_name = name.into();
        self
    }

    pub fn validation_layer(mut self, on: bool) -> Self {
        self.validation_layer_enabled = on;
        self
    }

    pub fn with_axis_input<S>(mut self, key_event: KeyCode, input: S, value: f32) -> Self
    where
        S: Into<u32>,
    {
        self.input_controller
            .assign_axis_input(key_event, input.into(), value);
        self
    }

    pub fn with_system(mut self, system: Box<dyn System>, order: SystemExecutionOrder) -> Self {
        self.world.register_system(system, order);
        self
    }

    pub fn with_systems(mut self, systems: Vec<Box<dyn System>>) -> Self {
        for system in systems {
            self.world
                .register_system(system, SystemExecutionOrder::default());
        }
        self
    }

    fn build_event_loop() -> EventLoop<()> {
        let event_loop = EventLoop::new().unwrap();
        event_loop.set_control_flow(ControlFlow::Poll);
        event_loop
    }

    pub fn build(self) -> (Application, EventLoop<()>) {
        let event_loop = Self::build_event_loop();

        let mut input_controller = self.input_controller;

        cameracontroller::fpscontrol::setup_camera_bindings(
            self.controller.clone(),
            &mut input_controller,
        );
        let info = ApplicationInfo {
            name: self.app_name,
            validation_layer_enabled: self.validation_layer_enabled,
        };
        let mut world = self.world;
        let camera = Rc::new(RefCell::new(Camera::new(&mut world)));

        let app = Application {
            window: None,
            renderer: None,
            camera,
            controller: self.controller,
            input_controller,
            scene: Scene::new(),
            gltf_cache: FileCache::new(),
            stage_upload: false,
            timer: Timer::new(100),
            info,
            world,
        };

        (app, event_loop)
    }
}
