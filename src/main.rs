mod application;
mod cameracontroller;
mod gui;
mod inputcontroller;
mod rendering;
mod util;
mod vulkanstuff;

use application::{Scene, SceneObject};
use mikpe_math::{Mat4, Vec3};
use rendering::vertextypes;
use rendering::Mesh;
use vulkanstuff::Texture;

use std::{
    cell::RefCell, ffi::CString, path::PathBuf, rc::Rc, sync::Arc, sync::Mutex, thread,
    time::Instant,
};
use winit::{event::VirtualKeyCode, event_loop::EventLoop};

const FAR_PLANE: f32 = 10000.0;
const NEAR_PLANE: f32 = 0.001;
//An example message
//TODO: implement this as async/await?
enum UploadMessage {
    Texture(PathBuf),
    Model(PathBuf),
    Exit(),
}

enum FinishedUpload {
    Texture(Texture),
    Model(Mesh),
}

fn main() {
    let event_loop = EventLoop::new();
    let app_name = CString::new("Mikpe Renderer").unwrap();
    let engine_name = CString::new("MikpEngine").unwrap();
    let mut renderer = Arc::new(Mutex::new(vulkanstuff::VulkanRenderer::init(
        &event_loop,
        true,
        app_name,
        engine_name,
    )));
    let mut input_controller = inputcontroller::InputController::new();

    input_controller.assign_axis_input(VirtualKeyCode::A, "SteerHori".into(), -1.0);
    input_controller.assign_axis_input(VirtualKeyCode::D, "SteerHori".into(), 1.0);

    input_controller.assign_axis_input(VirtualKeyCode::W, "SteerFwd".into(), 1.0);
    input_controller.assign_axis_input(VirtualKeyCode::S, "SteerFwd".into(), -1.0);

    input_controller.assign_axis_input(VirtualKeyCode::Q, "SteerVert".into(), -1.0);
    input_controller.assign_axis_input(VirtualKeyCode::E, "SteerVert".into(), 1.0);

    let camera = Rc::new(RefCell::new(cameracontroller::Camera::new()));
    cameracontroller::setup_camera_bindings(camera.clone(), &mut input_controller);

    let img = image::open(PathBuf::from("resources/images/TestImage.png"))
        .unwrap()
        .to_rgba();
    let (img_width, img_height) = img.dimensions();
    // let mut textures = vec![];

    let size = renderer.lock().unwrap().window.inner_size();
    let win_x: f64 = size.width.into();
    let win_y: f64 = size.height.into();
    let mut projection_matrix =
        Mat4::create_proj(60.0, (win_x / win_y) as f32, NEAR_PLANE, FAR_PLANE);

    let mut last_frame = Instant::now();
    let mut timer = util::Timer::new(100);
    let mut frame_number = 0;

    let mut scene = Scene::new();

    let (tx, rx) = std::sync::mpsc::channel();
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    let upload_renderer = renderer.clone();
    let upload_thread = thread::spawn(move || {
        let renderer = upload_renderer;
        let mut model_cache = util::ModelCache::new();
        {
            let mut locked_renderer = renderer.lock().unwrap();
            finished_tx
                .send(FinishedUpload::Model(Mesh::new_from_cache(
                    model_cache.read_gltf(PathBuf::from("resources/models/Tiger.glb")),
                    &mut locked_renderer,
                    Vec3::new(-100.0, 0.0, 0.0),
                )))
                .unwrap();
            finished_tx
                .send(FinishedUpload::Model(Mesh::new_from_cache(
                    model_cache.read_gltf(PathBuf::from("resources/models/FoxFixed.glb")),
                    &mut locked_renderer,
                    Vec3::new(-1.0, 0.0, 0.0),
                )))
                .unwrap();
            finished_tx
                .send(FinishedUpload::Model(Mesh::new_from_cache(
                    model_cache.read_gltf(PathBuf::from("resources/models/Avocado.glb")),
                    &mut locked_renderer,
                    Vec3::new(-1.0, 0.0, 0.0),
                )))
                .unwrap();
        }
        let mut x_offset = 0.0;
        loop {
            let message = rx.recv().unwrap();
            match message {
                UploadMessage::Texture(_) => {}
                UploadMessage::Model(path) => {
                    //TODO: This lock might be held for a while, minimize this!
                    let mut locked_renderer = renderer.lock().unwrap();
                    let mesh = Mesh::new_from_cache(
                        model_cache.read_gltf(path),
                        &mut locked_renderer,
                        Vec3::new(x_offset, 0.0, 0.0),
                    );
                    x_offset += 100.0;
                    finished_tx.send(FinishedUpload::Model(mesh)).unwrap();
                }
                UploadMessage::Exit() => {
                    break;
                }
            }
        }
    });

    event_loop.run(move |event, _, control_flow| {
        let renderer = renderer.clone();
        use winit::event::{Event, WindowEvent};
        use winit::event_loop::ControlFlow;

        match finished_rx.try_recv() {
            Ok(asset_type) => match asset_type {
                FinishedUpload::Model(mesh) => {
                    let bounds = mesh.bounds.clone();
                    scene.add_object(SceneObject::new(Box::new(mesh), bounds));
                }
                _ => {}
            },
            Err(_) => {}
        }

        camera.borrow_mut().handle_event(&event);
        match event {
            Event::NewEvents(winit::event::StartCause::Init) => {
                *control_flow = ControlFlow::Poll;
            }
            Event::WindowEvent { event, .. } => {
                input_controller.handle_event(&event);
                match event {
                    WindowEvent::Resized(logical_size) => {
                        let win_x = logical_size.width as f64;
                        let win_y = logical_size.height as f64;
                        projection_matrix =
                            Mat4::create_proj(60.0, (win_x / win_y) as f32, NEAR_PLANE, FAR_PLANE);
                        renderer.lock().unwrap().recreate_swapchain();
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
                                        tx.send(UploadMessage::Model(PathBuf::from(
                                            "resources/models/Tiger.glb",
                                        )))
                                        .unwrap();
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
                let mut locked_renderer = renderer.lock().unwrap();
                locked_renderer.swap_frames();

                frame_number += 1;
                let end = last_frame.elapsed().as_micros() as f64 / 1000.0;
                timer.add_timestamp(end);

                let delta_time = last_frame.elapsed().as_micros() as f32 / 1_000_000.0;
                camera.borrow_mut().update(delta_time);

                last_frame = Instant::now();
                scene.update(
                    &locked_renderer.context.device,
                    &projection_matrix,
                    &camera.borrow().get_view_mat().inverse(),
                );
                let command_buffer = locked_renderer.get_commandbuffer_opaque_pass();
                scene.render(&locked_renderer.context.device, command_buffer);
                unsafe {
                    locked_renderer
                        .context
                        .device
                        .cmd_end_render_pass(command_buffer);
                    locked_renderer
                        .context
                        .device
                        .end_command_buffer(command_buffer)
                        .unwrap();
                }
                locked_renderer.submit_frame(vec![command_buffer]);
            }
            Event::RedrawRequested { .. } => {}
            Event::LoopDestroyed => {
                tx.send(UploadMessage::Exit()).unwrap();
                // upload_thread.join();
                let mut locked_renderer = renderer.lock().unwrap();
                locked_renderer.wait_for_device();
                scene.teardown();
                println!("Loop destroyed!");
                // let mut tex_removal = vec![];
                // std::mem::swap(&mut textures, &mut tex_removal);
                // for texture in tex_removal {
                //     texture.destroy(&renderer.context);
                // }
                locked_renderer.destroy();
            }
            _ => {}
        }
    });
    upload_thread.join().unwrap();
    println!("All threads joined and everything is OK!");
}
