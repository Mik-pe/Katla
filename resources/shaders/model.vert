#version 450

uniform mat4 u_projMatrix;
uniform mat4 u_viewMatrix;
uniform mat4 u_modelMatrix;

layout(location=0) in vec3 vert_pos;
layout(location=1) in vec3 vert_normal;
layout(location=2) in vec2 vert_texcoord0;

out vec2 tex_coords;
// out vec3 vs_normal;
void main()
{
    tex_coords = vert_texcoord0;

    // vs_normal = normalize(vert_pos);

    gl_Position = u_projMatrix * u_viewMatrix * u_modelMatrix * vec4(vert_pos, 1.0);
}
