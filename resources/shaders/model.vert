#version 450

uniform mat4 u_projMatrix;
uniform mat4 u_viewMatrix;
uniform mat4 u_modelMatrix;

layout(location=0) in vec3 vert_pos;
layout(location=1) in vec3 vert_normal;

out vec2 tex_coords;
out vec3 vs_normal;
void main()
{
    tex_coords = vec2(vert_normal.x, 0.0);
    vs_normal = vert_normal;
    gl_Position = u_projMatrix * u_viewMatrix * u_modelMatrix * vec4(vert_pos, 1.0);
}