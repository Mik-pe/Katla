#version 450
layout(location=0) in vec3 position;

layout(set = 0, binding = 0) uniform Data {
    mat4 world;
    mat4 view;
    mat4 proj;
} uniforms;

layout(location=0) out vec3 vs_pos;
layout(location=1) out vec2 tex_coords;
// layout(location=2) out mat3 vs_TBN;
void main()
{
    // tex_coords = vert_texcoord0;
    // vec3 vs_tangent = normalize((uniforms.world * vec4(vert_tangent.xyz, 0.0)).xyz);
    // vec3 vs_normal = normalize((uniforms.world * vec4(normal, 0.0)).xyz);
    // vs_tangent = normalize(vs_tangent - dot(vs_tangent, vs_normal) * vs_normal);
    // vec3 vs_bitangent = (cross(normal, vert_tangent.xyz) * vert_tangent.w);
    // vs_TBN = mat3(vs_tangent, vs_bitangent, vs_normal);
    // vs_TBN = mat3(1.0);
    // vs_pos = (uniforms.world * vec4(position, 1.0)).xyz;

    tex_coords = vec2(0.0, 0.0);
    vs_pos = position;
    gl_Position = uniforms.proj * uniforms.view * uniforms.world * vec4(position, 1.0);
}
