#version 450

uniform mat4 u_projMatrix;
uniform mat4 u_viewMatrix;
uniform mat4 u_modelMatrix;

layout(location=0) in vec2 in_pos;
layout(location=1) in vec2 in_tex_coord;
layout(location=2) in vec4 in_color;


out vec2 vs_tex_coords;
out vec4 vs_color;
void main()
{
    vs_tex_coords = in_tex_coord;
    vs_color = in_color;
    gl_Position = u_projMatrix * vec4(in_pos, 0.0, 1.0);
}