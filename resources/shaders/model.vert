#version 450

uniform mat4 u_projMatrix;

layout(location=0) in vec3 vert_pos;
layout(location=1) in vec3 vert_normal;

out vec2 tex_coords;

void main()
{
    tex_coords = vec2(vert_normal.x, 0.0);
    gl_Position = vec4(vert_pos, 1.0) + vec4(0.0, 0.0, -0.5, 0.0);
}
