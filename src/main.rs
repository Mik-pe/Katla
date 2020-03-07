mod gui;
mod rendering;
mod util;

use bitflags::bitflags;
use gl;
use glutin::{
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
    ContextBuilder,
};
use imgui::{im_str, Condition, Context};
use imgui_winit_support::{HiDpiMode, WinitPlatform};
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
    Acknowledgement(rendering::Texture),
    Mesh(Box<dyn FnOnce() -> rendering::Mesh + Send>),
}
const GPU_MEM_INFO_TOTAL_AVAILABLE_MEM_NVX: gl::types::GLenum = 0x9048;
const GPU_MEM_INFO_CURRENT_AVAILABLE_MEM_NVX: gl::types::GLenum = 0x9049;
fn main() {
    let (sender, receiver) = std::sync::mpsc::channel();
    let (tex_sender, tex_receiver) = std::sync::mpsc::channel();

    let mut projection_matrix = Mat4::create_proj(60.0, 1.0, 0.5, 1000.0);
    let mut camera_pos = Vec3::new(0.0, 0.0, 0.0);
    let event_loop = EventLoop::new();
    let mut win_x = 512.0f64;
    let mut win_y = 512.0f64;
    let window = WindowBuilder::new().with_inner_size(glutin::dpi::LogicalSize::new(win_x, win_y));
    let gl_context = ContextBuilder::new()
        .with_vsync(true)
        .with_gl_profile(glutin::GlProfile::Core)
        .with_gl(glutin::GlRequest::Specific(glutin::Api::OpenGl, (4, 6)))
        .build_windowed(window, &event_loop)
        .unwrap();
    let mut current_dpi_scale = gl_context.window().current_monitor().scale_factor();
    win_x = win_x * current_dpi_scale;
    win_y = win_y * current_dpi_scale;

    let gl_window = unsafe { gl_context.make_current() }.unwrap();
    gl::load_with(|symbol| gl_window.get_proc_address(symbol) as *const _);

    let upload_events_loop = EventLoop::new();
    let upload_context = ContextBuilder::new()
        .with_shared_lists(&gl_window)
        .with_vsync(true)
        .build_headless(&upload_events_loop, glutin::dpi::PhysicalSize::new(0, 0))
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
        let mut current_red = 0u8;
        let mut current_green = 0u8;
        let mut current_blue = 0u8;
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
                        let tex = rendering::Texture::new(rendering::TextureUsage::ALBEDO);
                        uploaded_textures.push(tex);
                        if uploaded_textures.len() == max_textures_per_flush {
                            break;
                        }
                    },
                    Message::UploadMesh => {
                        for y in 0..10 {
                            for x_offset in 0..10 {
                                let mut mesh = rendering::Mesh::new();
                                mesh.set_pos(mikpe_math::Vec3::new(
                                    -5.0 + 5.0 * x_offset as f32,
                                    -5.0 + 5.0 * y as f32,
                                    -5.0,
                                ));
                                uploaded_meshes.push(mesh);
                            }
                        }
                        // if uploaded_meshes.len() == max_meshes_per_flush {
                        //     break;
                        // }
                    }
                    Message::Exit => {
                        should_exit = true;
                    }
                }
            }

            for mut tex in uploaded_textures {
                let num_mipmaps = 10;
                unsafe {
                    let mut img: image::RgbaImage = image::ImageBuffer::new(1024, 1024);
                    let pixel_step = 1024 / 255;
                    let mut pixel_it = 0;
                    for pixel in img.pixels_mut() {
                        *pixel = image::Rgba([current_red, current_green, current_blue, 255]);

                        if pixel_it % pixel_step == 0 {
                            current_red = current_red.wrapping_add(1);
                            if current_red == 0 {
                                current_green = current_green.wrapping_add(1);
                                if current_green == 0 {
                                    current_blue = current_blue.wrapping_add(1);
                                }
                            }
                        }
                        pixel_it += 1;
                    }
                    tex.set_data(img);
                    gl::Flush();
                }
                uploads.push(UploadFinished::Acknowledgement(tex));
            }
            for mut mesh in uploaded_meshes {
                mesh.read_gltf("resources/models/Avocado.glb");
                mesh.set_scale(100.1);
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

    let mut imgui = Context::create();
    let imgui_font_texid = unsafe {
        let mut fonts = imgui.fonts();
        let font_atlas = fonts.build_alpha8_texture();

        let mut tex = 0;
        gl::CreateTextures(gl::TEXTURE_2D, 1, &mut tex);

        gl::TextureStorage2D(
            tex,
            1,
            gl::R8,
            font_atlas.width as i32,
            font_atlas.height as i32,
        );

        gl::TextureSubImage2D(
            tex,
            0, // level
            0, // xoffset
            0, // yoffset
            font_atlas.width as i32,
            font_atlas.height as i32,
            gl::RED,
            gl::UNSIGNED_BYTE,
            font_atlas.data.as_ptr() as *const _,
        );
        fonts.tex_id = (tex as usize).into();
        tex
    };

    let mut platform = WinitPlatform::init(&mut imgui);
    platform.attach_window(imgui.io_mut(), gl_window.window(), HiDpiMode::Default);

    let mut last_frame = Instant::now();
    let mut angle = 60.0;
    let mut tex_list = vec![];
    let mut movement_vec = mikpe_math::Vec3::new(0.0, 0.0, 0.0);
    let mut timer = util::Timer::new(300);
    let mut rotangle = 0.0;
    let mut current_movement = Movement::STILL;
    let model_program = rendering::Program::new(
        include_bytes!("../resources/shaders/model.vert"),
        include_bytes!("../resources/shaders/model.frag"),
    );
    let mut gui = gui::Gui::new();
    event_loop.run(move |event, _, control_flow| {
        use glutin::event::{ElementState, Event, VirtualKeyCode, WindowEvent};
        platform.handle_event(imgui.io_mut(), &gl_window.window(), &event);
        match event {
            Event::NewEvents(_) => {
                // other application-specific logic
                last_frame = imgui.io_mut().update_delta_time(last_frame);
            }
            Event::MainEventsCleared => {
                // other application-specific logic
                platform
                    .prepare_frame(imgui.io_mut(), &gl_window.window())
                    .expect("Failed to prepare frame");
                gl_window.window().request_redraw();
            }
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::ScaleFactorChanged {
                    scale_factor,
                    new_inner_size: _,
                } => {
                    current_dpi_scale = scale_factor;
                }

                WindowEvent::Resized(logical_size) => {
                    win_x = logical_size.width as f64;
                    win_y = logical_size.height as f64;
                    projection_matrix =
                        Mat4::create_proj(60.0, (win_x / win_y) as f32, 0.1, 1000.0);
                }
                WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                WindowEvent::KeyboardInput {
                    device_id: _,
                    input,
                    is_synthetic: _,
                } => {
                    if input.state == ElementState::Pressed {
                        match input.virtual_keycode {
                            Some(keycode) => match keycode {
                                VirtualKeyCode::Escape => {
                                    *control_flow = ControlFlow::Exit;
                                }
                                VirtualKeyCode::W => {
                                    current_movement |= Movement::FORWARD;
                                }
                                VirtualKeyCode::S => {
                                    current_movement |= Movement::BACKWARDS;
                                }
                                VirtualKeyCode::A => {
                                    current_movement |= Movement::LEFT;
                                }
                                VirtualKeyCode::D => {
                                    current_movement |= Movement::RIGHT;
                                }
                                VirtualKeyCode::Q => {
                                    current_movement |= Movement::DOWN;
                                }
                                VirtualKeyCode::E => {
                                    current_movement |= Movement::UP;
                                }
                                VirtualKeyCode::N => {
                                    angle += 5.0;
                                    projection_matrix = Mat4::create_proj(
                                        60.0,
                                        (win_x / win_y) as f32,
                                        0.1,
                                        1000.0,
                                    );
                                }
                                VirtualKeyCode::M => {
                                    angle -= 5.0;
                                    projection_matrix = Mat4::create_proj(
                                        60.0,
                                        (win_x / win_y) as f32,
                                        0.1,
                                        1000.0,
                                    );
                                }
                                VirtualKeyCode::Space => {
                                    for _ in 0..10 {
                                        sender
                                            .send(Message::UploadTexture)
                                            .expect("Could not send Upload message");
                                    }
                                }
                                VirtualKeyCode::B => {
                                    sender
                                        .send(Message::UploadMesh)
                                        .expect("Could not send UploadMesh message");
                                }
                                VirtualKeyCode::Right => {
                                    rotangle += 0.1;
                                }
                                VirtualKeyCode::Left => {
                                    rotangle -= 0.1;
                                }
                                _ => {}
                            },
                            None => {}
                        };
                    }
                    if input.state == ElementState::Released {
                        match input.virtual_keycode {
                            Some(keycode) => match keycode {
                                VirtualKeyCode::W => {
                                    current_movement -= Movement::FORWARD;
                                }
                                VirtualKeyCode::S => {
                                    current_movement -= Movement::BACKWARDS;
                                }
                                VirtualKeyCode::A => {
                                    current_movement -= Movement::LEFT;
                                }
                                VirtualKeyCode::D => {
                                    current_movement -= Movement::RIGHT;
                                }
                                VirtualKeyCode::Q => {
                                    current_movement -= Movement::DOWN;
                                }
                                VirtualKeyCode::E => {
                                    current_movement -= Movement::UP;
                                }
                                _ => {}
                            },
                            None => {}
                        }
                    }
                }
                _ => {}
            },
            Event::RedrawRequested(_) => {
                let ui = imgui.frame();
                imgui::Window::new(im_str!("Hello world"))
                    .size([300.0, 100.0], Condition::FirstUseEver)
                    .build(&ui, || {
                        ui.text(im_str!("Hello world!"));
                        ui.text(im_str!("This...is...imgui-rs!"));
                        ui.separator();
                        let mouse_pos = ui.io().mouse_pos;
                        ui.text(format!(
                            "Mouse Position: ({:.1},{:.1})",
                            mouse_pos[0], mouse_pos[1]
                        ));
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
                                tex_list.last().unwrap().bind();
                            }
                        }
                        UploadFinished::Mesh(mesh_fn) => {
                            let mut mesh = mesh_fn();
                            meshes.push(mesh);
                        }
                    }
                }
                let view_matrix = Mat4::create_lookat(
                    camera_pos.clone(),
                    camera_pos.clone() + Vec3::new(0.0, 0.0, -1.0),
                    Vec3::new(0.0, 1.0, 0.0),
                )
                .inverse();
                unsafe {
                    gl::Enable(gl::DEPTH_TEST);
                    gl::Disable(gl::SCISSOR_TEST);

                    gl::Viewport(0, 0, win_x as i32, win_y as i32);
                    model_program.uniform_mat(&"u_projMatrix".to_owned(), &projection_matrix);
                    model_program.uniform_mat(&"u_viewMatrix".to_owned(), &view_matrix);
                    model_program.bind();
                    gl::ClearColor(0.3, 0.5, 0.3, 1.0);
                    gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);
                    plane_mesh.update_model_matrix(&model_program);
                    plane_mesh.draw();
                    for mesh in &mut meshes {
                        mesh.rotate_z(rotangle);
                        mesh.update_model_matrix(&model_program);
                        mesh.draw();
                    }
                }

                //----IMGUI DRAW---//
                platform.prepare_render(&ui, &gl_window.window());
                unsafe { gui.render_gui(ui) };
                //----IMGUI DRAW---//

                gl_window.swap_buffers().unwrap();
                unsafe {
                    //Ensure explicit CPU<->GPU synchronization happens
                    //as to always sync cpu time to vsync
                    gl::Finish();
                }
                let end = last_frame.elapsed().as_micros() as f64 / 1000.0;
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
            Event::LoopDestroyed => {
                sender
                    .send(Message::Exit)
                    .expect("Could not send Exit message!");
                return;
            }
            _ => {}
        }
    });
    upload_thread.join().expect("Could not join threads!");
}
