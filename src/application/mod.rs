pub mod model;
pub mod scene;

use std::{cell::RefCell, ffi::CString, path::PathBuf, rc::Rc, time::Instant};

use mikpe_math::Vec3;
pub use model::*;
pub use scene::*;
use winit::{event::VirtualKeyCode, event_loop::EventLoop};

use crate::{
    cameracontroller, cameracontroller::Camera, inputcontroller::InputController,
    renderer::VulkanRenderer, rendering::Mesh, util::FileCache, util::GLTFModel, util::Timer,
};

pub struct Application {
    renderer: VulkanRenderer,
    camera: Rc<RefCell<Camera>>,
    input_controller: InputController,
    event_loop: EventLoop<()>,
    scene: Scene,
}

impl Application {
    pub fn run(self) -> ! {
        let mut last_frame = Instant::now();
        let mut timer = Timer::new(100);
        let event_loop = self.event_loop;
        let camera = self.camera;
        let mut renderer = self.renderer;
        let mut scene = self.scene;
        let mut input_controller = self.input_controller;
        let mut model_cache = FileCache::<GLTFModel>::new();
        let mut offset = 0.0;
        let mesh = Mesh::new_from_cache(
            model_cache.read(PathBuf::from("resources/models/Fox.glb")),
            renderer.context.clone(),
            renderer.render_pass,
            renderer.num_images(),
            Vec3::new(offset, 0.0, 0.0),
        );
        offset -= 100.0;
        let bounds = mesh.bounds.clone();
        scene.add_object(SceneObject::new(Box::new(mesh), bounds));

        let mut stage_upload = false;
        event_loop.run(move |event, _, control_flow| {
            use winit::event::{Event, WindowEvent};
            use winit::event_loop::ControlFlow;

            camera.borrow_mut().handle_event(&event);
            match event {
                Event::NewEvents(winit::event::StartCause::Init) => {
                    *control_flow = ControlFlow::Poll;
                }
                Event::WindowEvent { event, .. } => {
                    input_controller.handle_event(&event);
                    match event {
                        WindowEvent::Resized(logical_size) => {
                            let win_x = logical_size.width as f32;
                            let win_y = logical_size.height as f32;
                            camera.borrow_mut().aspect_ratio_changed(win_x / win_y);

                            //TODO: don't recreate if we minimize...
                            renderer.recreate_swapchain();
                        }
                        WindowEvent::CloseRequested => {
                            *control_flow = ControlFlow::Exit;
                        }
                        WindowEvent::KeyboardInput { input, .. } => match input.state {
                            winit::event::ElementState::Pressed => {
                                if let Some(keycode) = input.virtual_keycode {
                                    match keycode {
                                        VirtualKeyCode::Escape => {
                                            *control_flow = ControlFlow::Exit;
                                        }
                                        VirtualKeyCode::T => {
                                            stage_upload = true;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            _ => {}
                        },
                        _ => (),
                    }
                }
                Event::MainEventsCleared => {
                    renderer.swap_frames();

                    let end = last_frame.elapsed().as_micros() as f64 / 1000.0;
                    timer.add_timestamp(end);

                    let delta_time = last_frame.elapsed().as_micros() as f32 / 1_000_000.0;
                    camera.borrow_mut().update(delta_time);

                    last_frame = Instant::now();
                    scene.update(
                        &renderer.context.device,
                        &camera.borrow().get_proj_mat(),
                        &camera.borrow().get_view_mat().inverse(),
                    );

                    let command_buffer = renderer.get_commandbuffer_opaque_pass();
                    scene.render(&renderer.context.device, command_buffer);
                    renderer.submit_frame(vec![command_buffer]);
                    if stage_upload {
                        let start = Instant::now();
                        let mesh = Mesh::new_from_cache(
                            model_cache.read(PathBuf::from("resources/models/Tiger.glb")),
                            renderer.context.clone(),
                            renderer.render_pass,
                            renderer.num_images(),
                            Vec3::new(offset, 0.0, 0.0),
                        );
                        let millisecs = start.elapsed().as_micros() as f64 / 1000.0;

                        println!("Mesh new took {} ms", millisecs);
                        offset -= 100.0;
                        let bounds = mesh.bounds.clone();
                        scene.add_object(SceneObject::new(Box::new(mesh), bounds));
                        stage_upload = false;
                    }
                }
                Event::RedrawRequested { .. } => {}
                Event::LoopDestroyed => {
                    renderer.wait_for_device();
                    scene.teardown();
                    println!("Loop destroyed!");
                    // let mut tex_removal = vec![];
                    // std::mem::swap(&mut textures, &mut tex_removal);
                    // for texture in tex_removal {
                    //     texture.destroy(&renderer.context);
                    // }
                    renderer.destroy();
                }
                _ => {}
            }
        });
    }
}

#[derive(Default)]
pub struct ApplicationBuilder {
    app_name: CString,
    validation_layer_enabled: bool,
    camera: Rc<RefCell<Camera>>,
    input_controller: InputController,
}

impl ApplicationBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_name(mut self, name: &str) -> Self {
        self.app_name = CString::new(name)
            .expect("Unexpected characters in application name (only ASCII allowed)");
        self
    }

    pub fn validation_layer(mut self, on: bool) -> Self {
        self.validation_layer_enabled = on;
        self
    }
    pub fn with_axis_input<S>(mut self, key_event: VirtualKeyCode, input: S, value: f32) -> Self
    where
        S: Into<String>,
    {
        self.input_controller
            .assign_axis_input(key_event, input.into(), value);
        self
    }

    pub fn build(self) -> Application {
        let event_loop = EventLoop::new();
        let engine_name = CString::new("MikpEngine").unwrap();
        let renderer = VulkanRenderer::init(&event_loop, true, self.app_name, engine_name);
        let mut input_controller = self.input_controller;

        cameracontroller::setup_camera_bindings(self.camera.clone(), &mut input_controller);

        Application {
            renderer,
            camera: self.camera,
            input_controller: input_controller,
            event_loop,
            scene: Scene::new(),
        }
    }
}
