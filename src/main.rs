mod rendering;

use gl;
use glutin::{ContextBuilder, EventsLoop, WindowBuilder};
use std::time::Instant;

enum Message {
    Upload,
    Exit,
}

enum TextureUploaded {
    Request(u32),
    Acknowledgement(u32),
}

fn main() {
    let (sender, receiver) = std::sync::mpsc::channel();
    let (tex_sender, tex_receiver) = std::sync::mpsc::channel();

    let mut projection_matrix = mikpe_math::Mat4::create_proj(60.0, 1.0, 0.5, 1000.0);
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
        .build_headless(
            &upload_events_loop,
            glutin::dpi::PhysicalSize::new(1024.0, 1024.0),
        )
        .unwrap();

    let mut meshes = vec![];
    let mut my_mesh = rendering::Mesh::new();
    my_mesh.read_gltf("resources/models/Box.glb");
    my_mesh.set_pos(mikpe_math::Vec3::new(0.0, 0.0, -5.0));
    meshes.push(my_mesh);
    // for x in -5..6 {
    //     for y in -5..6 {
    //         let mut my_mesh = rendering::Mesh::new();
    //         my_mesh.read_gltf("resources/models/Box.glb");
    //         my_mesh.set_pos(mikpe_math::Vec3::new(x as f32, y as f32, -5.0));
    //         meshes.push(my_mesh);
    //     }
    // }

    let upload_thread = std::thread::spawn(move || {
        let _upload_context = unsafe { upload_context.make_current() }.unwrap();
        let mut current_green = 0u8;
        let mut should_exit = false;
        let max_textures_per_flush = 50;
        let mut uploaded_textures = vec![];
        loop {
            let start = Instant::now();

            for message in receiver.try_iter() {
                match message {
                    Message::Upload => unsafe {
                        let mut tex = 0u32;
                        gl::CreateTextures(gl::TEXTURE_2D, 1, &mut tex);
                        uploaded_textures.push(tex);
                        if uploaded_textures.len() == max_textures_per_flush {
                            break;
                        }
                    },
                    Message::Exit => {
                        should_exit = true;
                    }
                }
            }

            for tex in &uploaded_textures {
                let num_mipmaps = 10;
                unsafe {
                    gl::TextureStorage2D(*tex, num_mipmaps, gl::RGBA8, 1024, 1024);
                    let mut img: image::RgbaImage = image::ImageBuffer::new(1024, 1024);
                    for pixel in img.pixels_mut() {
                        *pixel = image::Rgba([255, current_green, 255, 255]);
                    }
                    current_green = current_green.wrapping_add(10);
                    gl::TextureSubImage2D(
                        *tex,
                        0, // level
                        0, // xoffset
                        0, // yoffset
                        1024,
                        1024,
                        gl::RGBA,
                        gl::UNSIGNED_BYTE,
                        img.into_raw().as_ptr() as *const _,
                    );
                    // let mipmap_start = Instant::now();
                    gl::GenerateTextureMipmap(*tex);
                    gl::Flush();
                    // let mipmap_end = mipmap_start.elapsed().as_micros() as f64 / 1000.0;
                    // println!("Mipmap generation took {}ms", mipmap_end);
                }
            }

            if uploaded_textures.len() > 0 {
                println!("Uploaded {} textures this time", uploaded_textures.len());
                unsafe {
                    //This glFinish ensures all previously recorded calls are realized by the server
                    gl::Finish();
                    let end = start.elapsed().as_micros() as f64 / 1000.0;
                    println!("Generation + upload took {}ms", end);
                }
            }
            for tex in &uploaded_textures {
                tex_sender
                    .send(TextureUploaded::Acknowledgement(*tex))
                    .expect("Could not send Texture Ack");
            }
            uploaded_textures.clear();
            if should_exit {
                break;
            }
        }
        println!("Exiting upload thread!");
    });

    let mut tex_list = vec![];
    let mut running = true;
    let mut highest_frametime = 0.0;
    let program = rendering::Program::new();
    let mut angle = 60.0;
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
                                    glutin::VirtualKeyCode::N => {
                                        angle += 5.0;
                                        projection_matrix =
                                            mikpe_math::Mat4::create_proj(angle, 1.0, 0.5, 1000.0);
                                    }
                                    glutin::VirtualKeyCode::M => {
                                        angle -= 5.0;
                                        projection_matrix =
                                            mikpe_math::Mat4::create_proj(angle, 1.0, 0.5, 1000.0);
                                    }
                                    glutin::VirtualKeyCode::Space => {
                                        for _ in 0..10 {
                                            sender
                                                .send(Message::Upload)
                                                .expect("Could not send Upload message");
                                        }
                                    }
                                    _ => {}
                                },
                                None => {}
                            };
                        }
                    }
                    _ => {}
                }
            }
        });
        for tex_result in tex_receiver.try_iter() {
            match tex_result {
                TextureUploaded::Acknowledgement(result) => {
                    tex_list.push(result);
                    unsafe {
                        gl::BindTextureUnit(0, result);
                    }
                }
                _ => {}
            }
        }
        unsafe {
            program.uniform_mat(&"u_projMatrix".to_owned(), &projection_matrix);
            program.bind();
            gl::ClearColor(0.3, 0.5, 0.3, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);
            for mesh in &mut meshes {
                mesh.rotate_z(0.01);
                mesh.update_model_matrix(&program);
                mesh.draw();
            }
        }
        gl_window
            .window()
            .set_title(format!("Got {} textures", tex_list.len()).as_str());
        gl_window.swap_buffers().unwrap();
        let end = start.elapsed().as_micros() as f64 / 1000.0;
        if end > 20.0 {
            println!("Long CPU frametime: {} ms", end);
        }
        highest_frametime = f64::max(highest_frametime, end);
    }
    sender
        .send(Message::Exit)
        .expect("Could not send Exit message!");
    upload_thread.join().expect("Could not join threads!");
    println!(
        "All threads joined and finished, highest time was {}",
        highest_frametime
    );
}
