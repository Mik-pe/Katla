#version 450
layout(binding=0) uniform sampler2D albedo_sampler;
layout(binding=1) uniform sampler2D normal_sampler;
layout(binding=2) uniform sampler2D roughness_sampler;
layout(binding=3) uniform sampler2D emissive_sampler;

in vec2 tex_coords;
// in vec3 vs_normal;

out vec4 out_col;

void main()
{
    vec4 color = texture(albedo_sampler, tex_coords);
    // vec3 normal_centered = (vs_normal * 0.5) + 0.5;
    out_col = color;
}
