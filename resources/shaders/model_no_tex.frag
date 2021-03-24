#version 450
layout(binding=0) uniform ColorBlend {
    vec4 u_basecolor;
} color_factor;

const vec3 light_color = vec3(1.0, 1.0, 1.0);
const float spec_strength = 0.5;

layout(location=0) in vec3 vs_pos;
layout(location=1) in vec2 tex_coords;
layout(location=2) in vec3 vs_norm;

layout(location=0) out vec4 out_col;

void main()
{
    out_col = color_factor.u_basecolor;
}
