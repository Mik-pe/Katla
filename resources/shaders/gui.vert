#version 450

layout(set = 0, binding = 0) uniform Data {
    mat4 projMatrix;
}u;

layout(location=0) in vec2 in_pos;
layout(location=1) in vec2 in_tex_coord;
layout(location=2) in vec4 in_color;


layout(location=0) out vec2 vs_tex_coords;
layout(location=1) out vec4 vs_color;
void main()
{
    vs_tex_coords = in_tex_coord;
    vs_color = in_color;
    gl_Position = u.projMatrix * vec4(in_pos, 0.0, 1.0);
}
