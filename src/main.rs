mod application;
mod cameracontroller;
mod gui;
mod rendering;
mod util;
mod vulkanstuff;

use application::{Scene, SceneObject};
use erupt::vk1_0::Vk10DeviceLoaderExt;
use mikpe_math::{Mat4, Vec3};
use rendering::vertextypes;
use rendering::Mesh;
use vulkanstuff::Texture;

use std::{ffi::CString, path::PathBuf, time::Instant};
use winit::event_loop::EventLoop;

fn main() {
    let event_loop = EventLoop::new();
    let mut camera = cameracontroller::Camera::new();
    let app_name = CString::new("Mikpe Renderer").unwrap();
    let engine_name = CString::new("MikpEngine").unwrap();
    let mut vulkan_ctx =
        vulkanstuff::VulkanRenderer::init(&event_loop, true, app_name, engine_name);
    let img = image::open(PathBuf::from("resources/images/TestImage.png"))
        .unwrap()
        .to_rgba();
    let (img_width, img_height) = img.dimensions();
    let mut textures = vec![];

    let mut model_cache = util::ModelCache::new();

    let size = vulkan_ctx.window.inner_size();
    let win_x: f64 = size.width.into();
    let win_y: f64 = size.height.into();
    let mut projection_matrix = Mat4::create_proj(60.0, (win_x / win_y) as f32, 0.01, 1000.0);
    let fox = Mesh::new_from_cache(
        model_cache.read_gltf(PathBuf::from("resources/models/FoxFixed.glb")),
        &mut vulkan_ctx,
        Vec3::new(-1.0, 0.0, 0.0),
    );
    let tiger = Mesh::new_from_cache(
        model_cache.read_gltf(PathBuf::from("resources/models/Tiger.glb")),
        &mut vulkan_ctx,
        Vec3::new(10.0, 0.0, 0.0),
    );

    //Delta time, in seconds
    let mut delta_time = 0.0;
    let mut last_frame = Instant::now();
    let mut timer = util::Timer::new(100);
    let mut frame_number = 0;

    let mut scene = Scene::new();
    // scene.add_object(SceneObject::new(Box::new(tiger)));
    // scene.add_object(SceneObject::new(Box::new(fox)));

    event_loop.run(move |event, _, control_flow| {
        use winit::event::{Event, VirtualKeyCode, WindowEvent};
        use winit::event_loop::ControlFlow;

        match event {
            Event::NewEvents(_) => {
                frame_number += 1;
                let end = last_frame.elapsed().as_micros() as f64 / 1000.0;
                timer.add_timestamp(end);
                // if frame_number % 100 == 0 {
                //     println!(
                //         "CPU time mean: {:.2}, min/max : {:.2}/{:.2}",
                //         timer.current_mean(),
                //         timer.current_min(),
                //         timer.current_max()
                //     );
                // }

                delta_time = last_frame.elapsed().as_micros() as f32 / 1_000_000.0;
                camera.update(delta_time);
                last_frame = Instant::now();
                *control_flow = ControlFlow::Poll;
            }
            Event::WindowEvent { event, .. } => {
                camera.handle_event(&event);
                match event {
                    WindowEvent::Resized(logical_size) => {
                        let win_x = logical_size.width as f64;
                        let win_y = logical_size.height as f64;
                        projection_matrix =
                            Mat4::create_proj(60.0, (win_x / win_y) as f32, 0.1, 1000.0);
                        vulkan_ctx.recreate_swapchain();
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
                                                &mut vulkan_ctx.context,
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
                vulkan_ctx.swap_frames();

                scene.update(
                    &vulkan_ctx.context.device,
                    &projection_matrix,
                    &camera.get_view_mat().inverse(),
                );
                let command_buffer = vulkan_ctx.get_commandbuffer_opaque_pass();
                scene.render(&vulkan_ctx.context.device, command_buffer);
                unsafe {
                    vulkan_ctx
                        .context
                        .device
                        .cmd_end_render_pass(command_buffer);
                    vulkan_ctx
                        .context
                        .device
                        .end_command_buffer(command_buffer)
                        .unwrap();
                }
                vulkan_ctx.submit_frame(vec![command_buffer]);
            }
            Event::RedrawRequested { .. } => {}
            Event::LoopDestroyed => {
                println!("Loop destroyed!");
                let mut tex_removal = vec![];
                std::mem::swap(&mut textures, &mut tex_removal);
                for texture in tex_removal {
                    texture.destroy(&vulkan_ctx.context);
                }
                vulkan_ctx.destroy();
                // vulkan_ctx.destroy(mesh_data)
                // vulkan_ctx.destroy(meshes.as_mut_slice());
            }
            _ => {}
        }
    });
}
