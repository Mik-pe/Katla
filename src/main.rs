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

use std::{cell::RefCell, ffi::CString, path::PathBuf, rc::Rc, time::Instant};
use winit::{event::VirtualKeyCode, event_loop::EventLoop};

fn main() {
    let event_loop = EventLoop::new();
    let app_name = CString::new("Mikpe Renderer").unwrap();
    let engine_name = CString::new("MikpEngine").unwrap();
    let mut renderer = vulkanstuff::VulkanRenderer::init(&event_loop, true, app_name, engine_name);
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
    let mut textures = vec![];

    let mut model_cache = util::ModelCache::new();

    let size = renderer.window.inner_size();
    let win_x: f64 = size.width.into();
    let win_y: f64 = size.height.into();
    let mut projection_matrix = Mat4::create_proj(60.0, (win_x / win_y) as f32, 0.01, 1000.0);
    // let fox = Mesh::new_from_cache(
    //     model_cache.read_gltf(PathBuf::from("resources/models/FoxFixed.glb")),
    //     &mut renderer,
    //     Vec3::new(-1.0, 0.0, 0.0),
    // );
    let tiger = Mesh::new_from_cache(
        model_cache.read_gltf(PathBuf::from("resources/models/Tiger.glb")),
        &mut renderer,
        Vec3::new(10.0, 0.0, 0.0),
    );

    let mut last_frame = Instant::now();
    let mut timer = util::Timer::new(100);
    let mut frame_number = 0;

    let mut scene = Scene::new();
    scene.add_object(SceneObject::new(Box::new(tiger)));
    // scene.add_object(SceneObject::new(Box::new(fox)));

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
                        let win_x = logical_size.width as f64;
                        let win_y = logical_size.height as f64;
                        projection_matrix =
                            Mat4::create_proj(60.0, (win_x / win_y) as f32, 0.1, 1000.0);
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
                                    VirtualKeyCode::L => {
                                        for _ in 0..100 {
                                            textures.push(Texture::create_image(
                                                &mut renderer.context,
                                                img_width,
                                                img_height,
                                                erupt::vk1_0::Format::R8G8B8A8_SRGB,
                                                img.clone().into_raw().as_slice(),
                                            ));
                                        }
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

                frame_number += 1;
                let end = last_frame.elapsed().as_micros() as f64 / 1000.0;
                timer.add_timestamp(end);

                let delta_time = last_frame.elapsed().as_micros() as f32 / 1_000_000.0;
                camera.borrow_mut().update(delta_time);

                last_frame = Instant::now();
                scene.update(
                    &renderer.context.device,
                    &projection_matrix,
                    &camera.borrow().get_view_mat().inverse(),
                );
                let command_buffer = renderer.get_commandbuffer_opaque_pass();
                scene.render(&renderer.context.device, command_buffer);
                unsafe {
                    renderer.context.device.cmd_end_render_pass(command_buffer);
                    renderer
                        .context
                        .device
                        .end_command_buffer(command_buffer)
                        .unwrap();
                }
                renderer.submit_frame(vec![command_buffer]);
            }
            Event::RedrawRequested { .. } => {}
            Event::LoopDestroyed => {
                println!("Loop destroyed!");
                let mut tex_removal = vec![];
                std::mem::swap(&mut textures, &mut tex_removal);
                for texture in tex_removal {
                    texture.destroy(&renderer.context);
                }
                renderer.destroy();
                // renderer.destroy(mesh_data)
                // renderer.destroy(meshes.as_mut_slice());
            }
            _ => {}
        }
    });
}
