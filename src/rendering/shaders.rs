use crate::gl;
use std::ffi::CStr;

// if(gl_VertexID == 0){
//     tex_coords = vec2(0.0, 0.0);
//     gl_Position = vec4(-0.5, -0.5, 0.0, 1.0);
// }
// else if(gl_VertexID == 1){
//     tex_coords = vec2(0.5, 1.0);
//     gl_Position = vec4(0.0, 0.0, 0.0, 1.0);
// }
// else if(gl_VertexID == 2){
//     tex_coords = vec2(1.0, 0.0);
//     gl_Position = vec4(0.5, -0.5, 0.0, 1.0);
// }else{
//     gl_Position = vec4(0.0, 0.0, 0.0, 1.0);
// }

static VS_SHADER_SRC: &'static [u8] = b"
#version 450
layout(location=0) in vec3 vert_pos;
layout(location=1) in vec3 vert_normal;

out vec2 tex_coords;

void main()
{
    tex_coords = vec2(vert_normal.x, 0.0);
    gl_Position = vec4(vert_pos, 1.0) + vec4(0.0, 0.0, 10.0, 0.0);
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

pub fn create_shader_program() -> u32 {
    let vs_shader = make_shader(gl::VERTEX_SHADER, VS_SHADER_SRC);
    let fs_shader = make_shader(gl::FRAGMENT_SHADER, FS_SHADER_SRC);
    link_program(vs_shader, fs_shader)
}
