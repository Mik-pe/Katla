use gl;

use glutin::{ContextBuilder, EventsLoop, WindowBuilder};
use std::ffi::CStr;
use std::time::{Duration, Instant};

static VS_SHADER_SRC: &'static [u8] = b"
#version 450

out vec2 tex_coords;
void main()
{
    tex_coords = vec2(0.0, 0.0);
    if(gl_VertexID == 0){
        tex_coords = vec2(0.0, 0.0);
        gl_Position = vec4(-0.5, -0.5, 0.0, 1.0);   
    }
    else if(gl_VertexID == 1){
        tex_coords = vec2(0.5, 1.0);
        gl_Position = vec4(0.0, 0.0, 0.0, 1.0);   
    }
    else if(gl_VertexID == 2){
        tex_coords = vec2(1.0, 0.0);
        gl_Position = vec4(0.5, -0.5, 0.0, 1.0);   
    }else{
        gl_Position = vec4(0.0, 0.0, 0.0, 1.0);   
    }
}\0";

static FS_SHADER_SRC: &'static [u8] = b"
#version 450
layout(binding=0) uniform sampler2D tex_sampler;

in vec2 tex_coords;

out vec4 out_col;

void main()
{
    vec4 color = texture(tex_sampler, tex_coords);
    out_col = vec4(color.rgb, 1.0);
    // out_col = vec4(tex_coords.r,tex_coords.g, 0.0, 1.0);
}\0";

macro_rules! glchk {
    ($($s:stmt;)*) => {
        $(
            $s;
            if cfg!(debug_assertions) {
                let err = gl::GetError();
                if err != gl::NO_ERROR {
                    let err_str = match err {
                        gl::INVALID_ENUM => "GL_INVALID_ENUM",
                        gl::INVALID_VALUE => "GL_INVALID_VALUE",
                        gl::INVALID_OPERATION => "GL_INVALID_OPERATION",
                        gl::INVALID_FRAMEBUFFER_OPERATION => "GL_INVALID_FRAMEBUFFER_OPERATION",
                        gl::OUT_OF_MEMORY => "GL_OUT_OF_MEMORY",
                        gl::STACK_UNDERFLOW => "GL_STACK_UNDERFLOW",
                        gl::STACK_OVERFLOW => "GL_STACK_OVERFLOW",
                        _ => "unknown error"
                    };
                    println!("{}:{} - {} caused {}",
                             file!(),
                             line!(),
                             stringify!($s),
                             err_str);
                }
            }
        )*
    }
}

enum Message {
    Upload,
    Exit,
}

enum TextureUploaded {
    Request(u32),
    Acknowledgement(u32),
}

fn make_shader(shader_type: gl::types::GLenum, shader_src: &[u8]) -> u32 {
    unsafe {
        let shader_id = gl::CreateShader(shader_type);
        //src to CStr
        let src_cstr = CStr::from_bytes_with_nul(shader_src).unwrap();
        let shader_len = src_cstr.to_bytes().len() as i32;
        gl::ShaderSource(
            shader_id,
            1,
            &src_cstr.as_ptr() as *const *const _,
            &shader_len as *const _,
        );
        gl::CompileShader(shader_id);
        let mut success = 0;
        gl::GetShaderiv(shader_id, gl::COMPILE_STATUS, &mut success);
        if success <= 0 {
            let mut info_log = [0i8; 512];
            let mut placeholder = 0;
            gl::GetShaderInfoLog(shader_id, 512, &mut placeholder, info_log.as_mut_ptr());
            let cstrinfo = CStr::from_ptr(info_log.as_ptr());
            println!("Shader compilation error: \n{}", cstrinfo.to_str().unwrap());
        };
        shader_id
    }
}

fn link_program(vs_shader: u32, fs_shader: u32) -> u32 {
    unsafe {
        let program = gl::CreateProgram();
        gl::AttachShader(program, vs_shader);
        gl::AttachShader(program, fs_shader);
        gl::LinkProgram(program);
        let mut success = 0;
        gl::GetProgramiv(program, gl::LINK_STATUS, &mut success);
        let mut placeholder = 0;
        if success <= 0 {
            let mut info_log = [0i8; 512];
            gl::GetProgramInfoLog(program, 512, &mut placeholder, info_log.as_mut_ptr());
            let cstrinfo = CStr::from_ptr(info_log.as_ptr());
            println!("Program link error: \n{}", cstrinfo.to_str().unwrap());
        }
        program
    }
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
        //TODO: Implement texture upload and sync channel
        let mut current_green = 0u8;
        let mut should_exit = false;
        let max_textures_per_flush = 50;
        let mut uploaded_textures = vec![];
        loop {
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
                // let end = start.elapsed().as_micros() as f64 / 1000.0;
            }
            if uploaded_textures.len() > 0 {
                println!("Uploaded {} textures this time", uploaded_textures.len());
                unsafe {
                    //This glFinish ensures all previously recorded calls are realized by the server
                    gl::Finish();
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
    let vs_shader = make_shader(gl::VERTEX_SHADER, VS_SHADER_SRC);
    let fs_shader = make_shader(gl::FRAGMENT_SHADER, FS_SHADER_SRC);
    let program = link_program(vs_shader, fs_shader);
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
                    } => match input.virtual_keycode {
                        Some(keycode) => match keycode {
                            glutin::VirtualKeyCode::Escape => {
                                running = false;
                            }
                            glutin::VirtualKeyCode::Space => {
                                sender
                                    .send(Message::Upload)
                                    .expect("Could not send Upload message");
                            }
                            _ => {}
                        },
                        None => {}
                    },
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
