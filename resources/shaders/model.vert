#version 450

uniform mat4 u_projMatrix;
uniform mat4 u_viewMatrix;
uniform mat4 u_modelMatrix;

layout(location=0) in vec3 vert_pos;
layout(location=1) in vec3 vert_normal;
layout(location=2) in vec4 vert_tangent;
layout(location=3) in vec2 vert_texcoord0;

const vec3 light_position = vec3(0.0, 2.0, 10.0);

out vec2 tex_coords;
out vec3 vs_pos;
out vec3 light_pos;
out vec3 vs_normal;
out vec3 vs_tangent;
out vec3 vs_bitangent;
void main()
{
    tex_coords = vert_texcoord0;
    vs_tangent = normalize((u_modelMatrix * vec4(vert_tangent.xyz, 0.0)).xyz);
    vs_normal = normalize((u_modelMatrix * vec4(vert_normal, 0.0)).xyz);
    vs_tangent = normalize(vs_tangent - dot(vs_tangent, vs_normal) * vs_normal);
    vs_bitangent = (cross(vert_normal, vert_tangent.xyz) * vert_tangent.w);
    vs_pos = (u_modelMatrix * vec4(vert_pos, 1.0)).xyz;
    light_pos = light_position.xyz;
    gl_Position = u_projMatrix * u_viewMatrix * u_modelMatrix * vec4(vert_pos, 1.0);
}
