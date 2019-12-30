mod gl;
mod rendering;

use glutin::{ContextBuilder, EventsLoop, WindowBuilder};
use std::ffi::CStr;
use std::time::{Duration, Instant};

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

    let mut events_loop = EventsLoop::new();
    let window = WindowBuilder::new();
    let gl_context = ContextBuilder::new()
        .with_vsync(true)
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
                    gl::GenerateTextureMipmap(*tex);
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
    let program = rendering::create_shader_program();
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
            gl::UseProgram(program);
            gl::ClearColor(0.3, 0.5, 0.3, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
            glchk!(gl::DrawArrays(gl::TRIANGLES, 0, 3););
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
