#version 450
layout(binding=0) uniform sampler2D tex_sampler;

in vec2 tex_coords;
in vec3 vs_normal;

out vec4 out_col;

void main()
{
    vec4 color = texture(tex_sampler, tex_coords);
    vec3 normal_centered = (vs_normal * 0.5) + 0.5;
    out_col = vec4(normal_centered, 1.0);
    out_col = vec4(color.r, color.r, color.r, 1.0);
}