mod rendering;
mod util;

use bitflags::bitflags;
use gl;
use glutin::{ContextBuilder, EventsLoop, WindowBuilder};
use mikpe_math::{Mat4, Vec3};
use rendering::drawable::Drawable;
use std::time::Instant;

bitflags! {
    struct Movement: u32
    {
        const STILL     = 0b0000_0000;
        const FORWARD   = 0b0000_0001;
        const BACKWARDS = 0b0000_0010;
        const LEFT      = 0b0000_0100;
        const RIGHT     = 0b0000_1000;
        const UP        = 0b0001_0000;
        const DOWN      = 0b0010_0000;
    }
}

enum Message {
    UploadMesh,
    UploadTexture,
    Exit,
}

enum UploadFinished {
    Acknowledgement(u32),
    Mesh(Box<dyn FnOnce() -> rendering::Mesh + Send>),
}
const GPU_MEM_INFO_TOTAL_AVAILABLE_MEM_NVX: gl::types::GLenum = 0x9048;
const GPU_MEM_INFO_CURRENT_AVAILABLE_MEM_NVX: gl::types::GLenum = 0x9049;
fn main() {
    let (sender, receiver) = std::sync::mpsc::channel();
    let (tex_sender, tex_receiver) = std::sync::mpsc::channel();

    let mut projection_matrix = Mat4::create_proj(60.0, 1.0, 0.5, 1000.0);
    let mut camera_pos = Vec3::new(0.0, 0.0, 0.0);
    let mut view_matrix;
    let mut events_loop = EventsLoop::new();
    let window = WindowBuilder::new().with_dimensions(glutin::dpi::LogicalSize::new(512.0, 512.0));
    let gl_context = ContextBuilder::new()
        .with_vsync(true)
        .with_gl_profile(glutin::GlProfile::Core)
        .with_gl(glutin::GlRequest::Specific(glutin::Api::OpenGl, (4, 6)))
        .build_windowed(window, &events_loop)
        .unwrap();

    let gl_window = unsafe { gl_context.make_current() }.unwrap();
    gl::load_with(|symbol| gl_window.get_proc_address(symbol) as *const _);

    let upload_events_loop = EventsLoop::new();
    let upload_context = ContextBuilder::new()
        .with_shared_lists(&gl_window)
        .with_vsync(true)
        .build_headless(
            &upload_events_loop,
            glutin::dpi::PhysicalSize::new(0.0, 0.0),
        )
        .unwrap();
    unsafe {
        let mut total_mem_kb = 0;
        let mut current_mem_kb = 0;
        gl::GetIntegerv(GPU_MEM_INFO_TOTAL_AVAILABLE_MEM_NVX, &mut total_mem_kb);
        gl::GetIntegerv(GPU_MEM_INFO_CURRENT_AVAILABLE_MEM_NVX, &mut current_mem_kb);
        println!("Got {}MB total mem", total_mem_kb / 1024);
        println!("Got {}MB current mem", current_mem_kb / 1024);
    };
    let mut meshes: Vec<rendering::Mesh> = vec![];
    let mut plane_mesh = rendering::Mesh::new();
    plane_mesh.set_pos(Vec3::new(0.0, -2.0, 0.0));
    plane_mesh.read_gltf("resources/models/Regular_plane.glb");
    plane_mesh = unsafe { plane_mesh.rebind_gl() };
    //TODO: Return a tuple of sender, receiver and the uploader?
    //TODO: Fix a way so one can register an upload-function for an enum?
    //TODO: Spawn the thread inside of the uploader and provide a join function? Do we want to join-on-drop?

    // let resource_uploader = rendering::ResourceUploader::new(receiver);

    let upload_thread = std::thread::spawn(move || {
        let _upload_context = unsafe { upload_context.make_current() }.unwrap();
        let mut current_green = 0u8;
        let mut should_exit = false;
        let max_textures_per_flush = 50;
        let max_meshes_per_flush = 10;
        loop {
            let mut uploads = vec![];
            let mut uploaded_textures = vec![];
            let mut uploaded_meshes = vec![];
            let start = Instant::now();

            for message in receiver.try_iter() {
                match message {
                    Message::UploadTexture => unsafe {
                        let mut tex = 0u32;
                        gl::CreateTextures(gl::TEXTURE_2D, 1, &mut tex);
                        uploaded_textures.push(tex);
                        if uploaded_textures.len() == max_textures_per_flush {
                            break;
                        }
                    },
                    Message::UploadMesh => {
                        let mesh = rendering::Mesh::new();
                        uploaded_meshes.push(mesh);
                        if uploaded_meshes.len() == max_meshes_per_flush {
                            break;
                        }
                    }
                    Message::Exit => {
                        should_exit = true;
                    }
                }
            }

            for tex in uploaded_textures {
                let num_mipmaps = 10;
                unsafe {
                    gl::TextureStorage2D(tex, num_mipmaps, gl::RGBA8, 1024, 1024);
                    let mut img: image::RgbaImage = image::ImageBuffer::new(1024, 1024);
                    for pixel in img.pixels_mut() {
                        *pixel = image::Rgba([255, current_green, 255, 255]);
                    }
                    current_green = current_green.wrapping_add(10);
                    gl::TextureSubImage2D(
                        tex,
                        0, // level
                        0, // xoffset
                        0, // yoffset
                        1024,
                        1024,
                        gl::RGBA,
                        gl::UNSIGNED_BYTE,
                        img.into_raw().as_ptr() as *const _,
                    );
                    gl::GenerateTextureMipmap(tex);
                    gl::Flush();
                }
                uploads.push(UploadFinished::Acknowledgement(tex));
            }
            for mut mesh in uploaded_meshes {
                mesh.read_gltf("resources/models/Fox.glb");
                mesh.set_scale(0.1);
                unsafe {
                    gl::Flush();
                };
                uploads.push(UploadFinished::Mesh(Box::new(move || unsafe {
                    mesh.rebind_gl()
                })));
            }

            if !uploads.is_empty() {
                unsafe {
                    //This glFinish ensures all previously recorded calls are realized by the server
                    gl::Finish();
                    let end = start.elapsed().as_micros() as f64 / 1000.0;
                    println!("Generation + upload took {}ms", end);
                }
            }

            for upload in uploads {
                tex_sender
                    .send(upload)
                    .expect("Could not send upload finished");
            }

            if should_exit {
                break;
            }
        }
        println!("Exiting upload thread!");
    });

    let mut tex_list = vec![];
    let mut running = true;
    let program = rendering::Program::new();
    let mut angle = 60.0;
    let mut rotangle = 0.0;
    let mut timer = util::Timer::new(300);
    let mut movement_vec;
    let mut current_movement = Movement::STILL;
    unsafe {
        gl::Enable(gl::DEPTH_TEST);
    }
    while running {
        let start = Instant::now();
        events_loop.poll_events(|event| {
            use glutin::{Event, WindowEvent};
            if let Event::WindowEvent { event, .. } = event {
                match event {
                    WindowEvent::CloseRequested => {
                        running = false;
                    }
                    WindowEvent::KeyboardInput {
                        device_id: _,
                        input,
                    } => {
                        if input.state == glutin::ElementState::Pressed {
                            match input.virtual_keycode {
                                Some(keycode) => match keycode {
                                    glutin::VirtualKeyCode::Escape => {
                                        running = false;
                                    }
                                    glutin::VirtualKeyCode::W => {
                                        current_movement |= Movement::FORWARD;
                                    }
                                    glutin::VirtualKeyCode::S => {
                                        current_movement |= Movement::BACKWARDS;
                                    }
                                    glutin::VirtualKeyCode::A => {
                                        current_movement |= Movement::LEFT;
                                    }
                                    glutin::VirtualKeyCode::D => {
                                        current_movement |= Movement::RIGHT;
                                    }
                                    glutin::VirtualKeyCode::Q => {
                                        current_movement |= Movement::DOWN;
                                    }
                                    glutin::VirtualKeyCode::E => {
                                        current_movement |= Movement::UP;
                                    }
                                    glutin::VirtualKeyCode::N => {
                                        angle += 5.0;
                                        projection_matrix =
                                            Mat4::create_proj(angle, 1.0, 0.5, 1000.0);
                                    }
                                    glutin::VirtualKeyCode::M => {
                                        angle -= 5.0;
                                        projection_matrix =
                                            Mat4::create_proj(angle, 1.0, 0.5, 1000.0);
                                    }
                                    glutin::VirtualKeyCode::Space => {
                                        for _ in 0..10 {
                                            sender
                                                .send(Message::UploadTexture)
                                                .expect("Could not send Upload message");
                                        }
                                    }
                                    glutin::VirtualKeyCode::B => {
                                        sender
                                            .send(Message::UploadMesh)
                                            .expect("Could not send UploadMesh message");
                                    }
                                    glutin::VirtualKeyCode::Right => {
                                        rotangle += 0.1;
                                    }
                                    glutin::VirtualKeyCode::Left => {
                                        rotangle -= 0.1;
                                    }
                                    _ => {}
                                },
                                None => {}
                            };
                        }
                        if input.state == glutin::ElementState::Released {
                            match input.virtual_keycode {
                                Some(keycode) => match keycode {
                                    glutin::VirtualKeyCode::W => {
                                        current_movement -= Movement::FORWARD;
                                    }
                                    glutin::VirtualKeyCode::S => {
                                        current_movement -= Movement::BACKWARDS;
                                    }
                                    glutin::VirtualKeyCode::A => {
                                        current_movement -= Movement::LEFT;
                                    }
                                    glutin::VirtualKeyCode::D => {
                                        current_movement -= Movement::RIGHT;
                                    }
                                    glutin::VirtualKeyCode::Q => {
                                        current_movement -= Movement::DOWN;
                                    }
                                    glutin::VirtualKeyCode::E => {
                                        current_movement -= Movement::UP;
                                    }
                                    _ => {}
                                },
                                None => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
        });
        movement_vec = Vec3::new(0.0, 0.0, 0.0);
        if current_movement.contains(Movement::FORWARD) {
            movement_vec[2] -= 1.0;
        }
        if current_movement.contains(Movement::BACKWARDS) {
            movement_vec[2] += 1.0;
        }
        if current_movement.contains(Movement::DOWN) {
            movement_vec[1] -= 1.0;
        }
        if current_movement.contains(Movement::UP) {
            movement_vec[1] += 1.0;
        }
        if current_movement.contains(Movement::LEFT) {
            movement_vec[0] -= 1.0;
        }
        if current_movement.contains(Movement::RIGHT) {
            movement_vec[0] += 1.0;
        }
        movement_vec = movement_vec.normalize();
        camera_pos = camera_pos + movement_vec;

        for tex_result in tex_receiver.try_iter() {
            match tex_result {
                UploadFinished::Acknowledgement(result) => {
                    tex_list.push(result);
                    unsafe {
                        gl::BindTextureUnit(0, result);
                    }
                }
                UploadFinished::Mesh(mesh_fn) => {
                    let mut mesh = mesh_fn();
                    let x_offset = meshes.len() as f32;
                    mesh.set_pos(mikpe_math::Vec3::new(-5.0 + 5.0 * x_offset, 0.0, -5.0));
                    meshes.push(mesh);
                }
            }
        }
        view_matrix = Mat4::create_lookat(
            camera_pos.clone(),
            camera_pos.clone() + Vec3::new(0.0, 0.0, -1.0),
            Vec3::new(0.0, 1.0, 0.0),
        )
        .inverse();
        unsafe {
            program.uniform_mat(&"u_projMatrix".to_owned(), &projection_matrix);
            program.uniform_mat(&"u_viewMatrix".to_owned(), &view_matrix);
            program.bind();
            gl::ClearColor(0.3, 0.5, 0.3, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);
            plane_mesh.update_model_matrix(&program);
            plane_mesh.draw();
            for mesh in &mut meshes {
                // mesh.set_pos(current_pos.clone());
                mesh.rotate_z(rotangle);
                mesh.update_model_matrix(&program);
                mesh.draw();
            }
        }

        gl_window.swap_buffers().unwrap();
        unsafe {
            //Ensure explicit CPU<->GPU synchronization happens
            //as to always sync cpu time to vsync
            gl::Finish();
        }
        let end = start.elapsed().as_micros() as f64 / 1000.0;
        if end > 20.0 {
            println!("Long CPU frametime: {} ms", end);
        }
        timer.add_timestamp(end);
        gl_window.window().set_title(
            format!(
                "Got {} textures, mean frametime: {:.3} (max {:.3}, min {:.3})",
                tex_list.len(),
                timer.current_mean(),
                timer.current_max(),
                timer.current_min(),
            )
            .as_str(),
        );
    }
    sender
        .send(Message::Exit)
        .expect("Could not send Exit message!");
    upload_thread.join().expect("Could not join threads!");
}
