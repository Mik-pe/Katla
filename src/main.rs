mod cameracontroller;
mod gui;
mod rendering;
mod util;
mod vulkanstuff;
use mikpe_math::{Mat4, Vec3};
use rendering::vertextypes;
use rendering::Mesh;

use std::ffi::CString;
use std::time::Instant;
use winit::event_loop::EventLoop;

// use imgui::{im_str, Condition, Context};
// use imgui_winit_support::{HiDpiMode, WinitPlatform};
// enum Message {
//     UploadMesh(String),
//     Exit,
// }

// enum UploadFinished {
//     Acknowledgement(rendering::Texture),
//     Mesh(Box<dyn FnOnce() -> rendering::Mesh + Send>),
// }

fn main() {
    let event_loop = EventLoop::new();
    let mut camera = cameracontroller::Camera::new();
    let app_name = CString::new("Mikpe Renderer").unwrap();
    let engine_name = CString::new("MikpEngine").unwrap();
    let mut vulkan_ctx = vulkanstuff::VulkanCtx::init(&event_loop, true, app_name, engine_name);

    let size = vulkan_ctx.window.inner_size();
    let mut win_x: f64 = size.width.into();
    let mut win_y: f64 = size.height.into();
    let mut projection_matrix = Mat4::create_proj(60.0, (win_x / win_y) as f32, 0.01, 1000.0);
    let _vert_data = vec![
        vertextypes::VertexNormal {
            position: [-0.5, -0.5, 0.0],
            normal: [1.0, 0.0, 0.0],
        },
        vertextypes::VertexNormal {
            position: [0.0, 0.5, 0.0],
            normal: [0.0, 1.0, 0.0],
        },
        vertextypes::VertexNormal {
            position: [0.5, -0.5, 0.0],
            normal: [0.0, 0.0, 1.0],
        },
    ];
    let pos_data = vec![
        vertextypes::VertexPosition {
            position: [0.5, 0.5, 0.0],
        },
        vertextypes::VertexPosition {
            position: [0.0, -0.5, 0.0],
        },
        vertextypes::VertexPosition {
            position: [-0.5, 0.5, 0.0],
        },
    ];

    let mut meshes = vec![
        Mesh::new_from_data(&mut vulkan_ctx, pos_data.clone(), Vec3::new(0.0, 1.0, 0.0)),
        Mesh::new_from_data(&mut vulkan_ctx, pos_data, Vec3::new(0.0, -1.0, 0.0)),
    ];
    //Delta time, in seconds
    let mut delta_time = 0.0;
    let mut last_frame = Instant::now();
    event_loop.run(move |event, _, control_flow| {
        use winit::event::{Event, VirtualKeyCode, WindowEvent};
        use winit::event_loop::ControlFlow;
        vulkan_ctx.handle_event(
            &event,
            meshes.as_mut_slice(),
            delta_time,
            &projection_matrix,
            &camera.get_view_mat().inverse(),
        );
        match event {
            Event::NewEvents(_) => {
                delta_time = last_frame.elapsed().as_micros() as f32 / 1_000_000.0;
                camera.update(delta_time);
                last_frame = Instant::now();
                *control_flow = ControlFlow::Poll;
            }
            Event::WindowEvent { event, .. } => {
                camera.handle_event(&event);
                match event {
                    WindowEvent::Resized(logical_size) => {
                        win_x = logical_size.width as f64;
                        win_y = logical_size.height as f64;
                        projection_matrix =
                            Mat4::create_proj(60.0, (win_x / win_y) as f32, 0.1, 1000.0);
                    }
                    WindowEvent::CloseRequested => {
                        *control_flow = ControlFlow::Exit;
                    }
                    WindowEvent::KeyboardInput { input, .. } => {
                        if let Some(keycode) = input.virtual_keycode {
                            match keycode {
                                VirtualKeyCode::Escape => {
                                    *control_flow = ControlFlow::Exit;
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => (),
                }
            }
            Event::RedrawRequested { .. } => {}
            Event::LoopDestroyed => {
                println!("Loop destroyed!");
                vulkan_ctx.destroy(meshes.as_mut_slice());
            }
            _ => {}
        }
    });
    // let (sender, receiver) = std::sync::mpsc::channel();
    // let (upload_sender, upload_recv) = std::sync::mpsc::channel();
    // let mut projection_matrix = Mat4::create_proj(60.0, 1.0, 0.5, 1000.0);
    // let event_loop = EventLoop::new();
    // let mut win_x = 512.0f64;
    // let mut win_y = 512.0f64;
    // let window = WindowBuilder::new().with_inner_size(glutin::dpi::LogicalSize::new(win_x, win_y));
    // let gl_context = ContextBuilder::new()
    //     .with_vsync(true)
    //     .with_gl_profile(glutin::GlProfile::Core)
    //     .with_gl(glutin::GlRequest::Specific(glutin::Api::OpenGl, (4, 6)))
    //     .build_windowed(window, &event_loop)
    //     .unwrap();
    // let mut current_dpi_scale = gl_context.window().current_monitor().scale_factor();
    // win_x = win_x * current_dpi_scale;
    // win_y = win_y * current_dpi_scale;

    // let gl_window = unsafe { gl_context.make_current() }.unwrap();
    // gl::load_with(|symbol| gl_window.get_proc_address(symbol) as *const _);

    // let upload_events_loop = EventLoop::new();
    // let upload_context = ContextBuilder::new()
    //     .with_shared_lists(&gl_window)
    //     .with_vsync(true)
    //     .build_headless(&upload_events_loop, glutin::dpi::PhysicalSize::new(0, 0))
    //     .unwrap();
    // unsafe {
    //     let mut total_mem_kb = 0;
    //     let mut current_mem_kb = 0;
    //     gl::GetIntegerv(GPU_MEM_INFO_TOTAL_AVAILABLE_MEM_NVX, &mut total_mem_kb);
    //     gl::GetIntegerv(GPU_MEM_INFO_CURRENT_AVAILABLE_MEM_NVX, &mut current_mem_kb);
    //     println!("Got {}MB total mem", total_mem_kb / 1024);
    //     println!("Got {}MB current mem", current_mem_kb / 1024);
    // };
    // let mut model_cache = util::ModelCache::new();
    // let mut meshes: Vec<rendering::Mesh> = vec![];
    // let mut plane_mesh = rendering::Mesh::new();
    // plane_mesh.set_pos(Vec3::new(0.0, -2.0, 0.0));
    // plane_mesh.init_from_cache(
    //     model_cache.read_gltf(PathBuf::from("resources/models/Regular_plane.glb")),
    // );
    // plane_mesh = unsafe { plane_mesh.rebind_gl() };

    // let mut box_mesh = rendering::Mesh::new();
    // box_mesh.read_gltf("resources/models/Box.glb");
    // box_mesh = unsafe { box_mesh.rebind_gl() };
    // let mut light_pos = Vec3::new(0.0, 200.0, 10.0);
    // //TODO: Return a tuple of sender, receiver and the uploader?
    // //TODO: Fix a way so one can register an upload-function for an enum?
    // //TODO: Spawn the thread inside of the uploader and provide a join function? Do we want to join-on-drop?

    // // let resource_uploader = rendering::ResourceUploader::new(receiver);

    // let upload_thread = std::thread::spawn(move || {
    //     let _upload_context = unsafe { upload_context.make_current() }.unwrap();
    //     let mut should_exit = false;
    //     let max_meshes_per_flush = 10;
    //     loop {
    //         let mut uploads = vec![];
    //         let mut uploaded_meshes = vec![];
    //         let start = Instant::now();

    //         for message in receiver.try_iter() {
    //             match message {
    //                 Message::UploadMesh(path) => {
    //                     let mut mesh = rendering::Mesh::new();
    //                     mesh.init_from_cache(model_cache.read_gltf(PathBuf::from(path)));
    //                     uploaded_meshes.push(mesh);
    //                     if uploaded_meshes.len() == max_meshes_per_flush {
    //                         break;
    //                     }
    //                 }
    //                 Message::Exit => {
    //                     should_exit = true;
    //                 }
    //             }
    //         }
    //         let mut sync = std::ptr::null_mut() as *const _;
    //         if !uploaded_meshes.is_empty() {
    //             sync = unsafe {
    //                 gl::Flush();
    //                 gl::FenceSync(gl::SYNC_GPU_COMMANDS_COMPLETE, 0)
    //             };
    //         }
    //         for mut mesh in uploaded_meshes {
    //             mesh.set_scale(0.1);
    //             println!("Uploaded mesh!");
    //             uploads.push(UploadFinished::Mesh(Box::new(move || unsafe {
    //                 mesh.rebind_gl()
    //             })));
    //         }

    //         if !uploads.is_empty() {
    //             unsafe {
    //                 //This glFinish ensures all previously recorded calls are realized by the server
    //                 gl::Flush();
    //                 gl::WaitSync(sync, 0, gl::TIMEOUT_IGNORED);
    //                 let end = start.elapsed().as_micros() as f64 / 1000.0;
    //                 println!("Generation + upload took {}ms", end);
    //             }
    //         }
    //         for upload in uploads {
    //             upload_sender
    //                 .send(upload)
    //                 .expect("Could not send upload finished");
    //         }

    //         if should_exit {
    //             break;
    //         }
    //     }
    //     println!("Exiting upload thread!");
    // });

    // let mut imgui = Context::create();
    // unsafe {
    //     let mut fonts = imgui.fonts();
    //     let font_atlas = fonts.build_alpha8_texture();

    //     let mut tex = 0;
    //     gl::CreateTextures(gl::TEXTURE_2D, 1, &mut tex);

    //     gl::TextureStorage2D(
    //         tex,
    //         1,
    //         gl::R8,
    //         font_atlas.width as i32,
    //         font_atlas.height as i32,
    //     );

    //     gl::TextureSubImage2D(
    //         tex,
    //         0, // level
    //         0, // xoffset
    //         0, // yoffset
    //         font_atlas.width as i32,
    //         font_atlas.height as i32,
    //         gl::RED,
    //         gl::UNSIGNED_BYTE,
    //         font_atlas.data.as_ptr() as *const _,
    //     );
    //     fonts.tex_id = (tex as usize).into();
    // };

    // let mut platform = WinitPlatform::init(&mut imgui);
    // platform.attach_window(imgui.io_mut(), gl_window.window(), HiDpiMode::Default);

    // let mut last_frame = Instant::now();
    // let mut angle = 60.0;
    // let mut tex_list = vec![];
    // let mut timer = util::Timer::new(300);
    // let mut rotangle = 0.0;
    // let model_program = rendering::Program::new(
    //     include_bytes!("../resources/shaders/model.vert"),
    //     include_bytes!("../resources/shaders/model.frag"),
    // );
    // let mut tex_vis = 0u32;
    // let mut gui = gui::Gui::new();
    // let mut total_time = 0.0;
    // let mut camera = cameracontroller::Camera::new();
    // event_loop.run(move |event, _, control_flow| {
    //     use glutin::event::{ElementState, Event, MouseButton, VirtualKeyCode, WindowEvent};
    //     platform.handle_event(imgui.io_mut(), &gl_window.window(), &event);
    //     total_time += imgui.io().delta_time;
    //     match event {
    //         Event::NewEvents(_) => {
    //             // other application-specific logic
    //             last_frame = imgui.io_mut().update_delta_time(last_frame);
    //             camera.update(imgui.io().delta_time);
    //         }
    //         Event::MainEventsCleared => {
    //             // other application-specific logic
    //             platform
    //                 .prepare_frame(imgui.io_mut(), &gl_window.window())
    //                 .expect("Failed to prepare frame");
    //             gl_window.window().request_redraw();
    //         }
    //         Event::WindowEvent { event, .. } => {
    //             camera.handle_event(&event);
    //             match event {
    //                 WindowEvent::ScaleFactorChanged {
    //                     scale_factor,
    //                     new_inner_size: _,
    //                 } => {
    //                     current_dpi_scale = scale_factor;
    //                 }
    //                 WindowEvent::Resized(logical_size) => {
    //                     win_x = logical_size.width as f64;
    //                     win_y = logical_size.height as f64;
    //                     projection_matrix =
    //                         Mat4::create_proj(60.0, (win_x / win_y) as f32, 0.1, 1000.0);
    //                 }
    //                 WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
    //                 WindowEvent::KeyboardInput {
    //                     device_id: _,
    //                     input,
    //                     is_synthetic: _,
    //                 } => {
    //                     if input.state == ElementState::Pressed {
    //                         match input.virtual_keycode {
    //                             Some(keycode) => match keycode {
    //                                 VirtualKeyCode::Escape => {
    //                                     *control_flow = ControlFlow::Exit;
    //                                 }
    //                                 VirtualKeyCode::N => {
    //                                     angle += 5.0;
    //                                     projection_matrix = Mat4::create_proj(
    //                                         60.0,
    //                                         (win_x / win_y) as f32,
    //                                         0.1,
    //                                         1000.0,
    //                                     );
    //                                 }
    //                                 VirtualKeyCode::M => {
    //                                     angle -= 5.0;
    //                                     projection_matrix = Mat4::create_proj(
    //                                         60.0,
    //                                         (win_x / win_y) as f32,
    //                                         0.1,
    //                                         1000.0,
    //                                     );
    //                                 }
    //                                 VirtualKeyCode::Right => {
    //                                     rotangle += 0.1;
    //                                 }
    //                                 VirtualKeyCode::Left => {
    //                                     rotangle -= 0.1;
    //                                 }
    //                                 VirtualKeyCode::Up => {
    //                                     tex_vis += 1;
    //                                 }
    //                                 VirtualKeyCode::Down => {
    //                                     if let Some(res) = tex_vis.checked_sub(1) {
    //                                         tex_vis = res;
    //                                     }
    //                                 }
    //                                 _ => {}
    //                             },
    //                             None => {}
    //                         };
    //                     }
    //                 }
    //                 _ => {}
    //             }
    //         }
    //         Event::RedrawRequested(_) => {
    //             let ui = imgui.frame();
    //             imgui::Window::new(im_str!("Hello world"))
    //                 .size([300.0, 100.0], Condition::FirstUseEver)
    //                 .build(&ui, || {
    //                     ui.text(im_str!("Hello world!"));
    //                     ui.text(im_str!("This...is...imgui-rs!"));
    //                     ui.text(format!("Current render mode: {}", tex_vis));
    //                     ui.text(format!(
    //                         "Got {} textures, mean frametime: {:.3} (max {:.3}, min {:.3})",
    //                         tex_list.len(),
    //                         timer.current_mean(),
    //                         timer.current_max(),
    //                         timer.current_min(),
    //                     ));
    //                     if ui.button(im_str!("Load Tiger"), [0.0, 0.0]) {
    //                         sender
    //                             .send(Message::UploadMesh("resources/models/Tiger.glb".to_owned()))
    //                             .expect("Could not send UploadMesh message");
    //                     }
    //                     if ui.button(im_str!("Load 10 Tigers"), [0.0, 0.0]) {
    //                         for _ in 0..10 {
    //                             sender
    //                                 .send(Message::UploadMesh(
    //                                     "resources/models/Tiger.glb".to_owned(),
    //                                 ))
    //                                 .expect("Could not send UploadMesh message");
    //                         }
    //                     }
    //                     if ui.button(im_str!("Load Fox"), [0.0, 0.0]) {
    //                         sender
    //                             .send(Message::UploadMesh("resources/models/Fox.glb".to_owned()))
    //                             .expect("Could not send UploadMesh message");
    //                     }
    //                     ui.separator();
    //                     let mouse_pos = ui.io().mouse_pos;
    //                     ui.text(format!(
    //                         "Mouse Position: ({:.1},{:.1})",
    //                         mouse_pos[0], mouse_pos[1]
    //                     ));
    //                 });

    //             for tex_result in upload_recv.try_iter() {
    //                 match tex_result {
    //                     UploadFinished::Acknowledgement(result) => {
    //                         tex_list.push(result);
    //                         unsafe {
    //                             tex_list.last().unwrap().bind();
    //                         }
    //                     }
    //                     UploadFinished::Mesh(mesh_fn) => {
    //                         let mut mesh = mesh_fn();
    //                         let x_offset = meshes.len() as f32;
    //                         mesh.set_pos(mikpe_math::Vec3::new(5.0 * x_offset, 0.0, 0.0));
    //                         meshes.push(mesh);
    //                     }
    //                 }
    //             }
    //             let view_matrix = camera.get_view_mat().inverse();
    //             unsafe {
    //                 gl::Enable(gl::DEPTH_TEST);
    //                 gl::Disable(gl::SCISSOR_TEST);

    //                 gl::Viewport(0, 0, win_x as i32, win_y as i32);
    //                 model_program.uniform_mat("u_projMatrix", &projection_matrix);
    //                 model_program.uniform_mat("u_viewMatrix", &view_matrix);
    //                 model_program.uniform_vec3("u_lightpos", light_pos.clone());
    //                 model_program.bind();
    //                 gl::ClearColor(0.3, 0.5, 0.3, 1.0);
    //                 gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);
    //                 plane_mesh.update_model_matrix(&model_program);
    //                 model_program.uniform_u32("u_texVis", tex_vis);
    //                 model_program.uniform_vec3("u_camPos", camera.get_cam_pos());
    //                 plane_mesh.draw();
    //                 light_pos = Vec3::new(
    //                     0.0,
    //                     f32::sin(total_time * 0.1) * 30.0,
    //                     f32::cos(total_time * 0.1) * 30.0,
    //                 );
    //                 box_mesh.set_pos(light_pos.clone());
    //                 box_mesh.update_model_matrix(&model_program);
    //                 box_mesh.draw();
    //                 for mesh in &mut meshes {
    //                     mesh.rotate_z(rotangle);
    //                     mesh.update_model_matrix(&model_program);
    //                     mesh.draw();
    //                 }
    //             }

    //             //----IMGUI DRAW---//
    //             platform.prepare_render(&ui, &gl_window.window());
    //             unsafe { gui.render_gui(ui) };
    //             //----IMGUI DRAW---//

    //             gl_window.swap_buffers().unwrap();
    //             unsafe {
    //                 //Ensure explicit CPU<->GPU synchronization happens
    //                 //as to always sync cpu time to vsync
    //                 gl::Finish();
    //             }
    //             let end = last_frame.elapsed().as_micros() as f64 / 1000.0;
    //             if end > 20.0 {
    //                 println!("Long CPU frametime: {} ms", end);
    //             }
    //             timer.add_timestamp(end);
    //         }
    //         Event::LoopDestroyed => {
    //             sender
    //                 .send(Message::Exit)
    //                 .expect("Could not send Exit message!");
    //             return;
    //         }
    //         _ => {}
    //     }
    // });
    // upload_thread.join().expect("Could not join threads!");
}
