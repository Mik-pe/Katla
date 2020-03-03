#version 450
layout(binding=0) uniform sampler2D tex_sampler;

in vec2 vs_tex_coords;
in vec4 vs_color;

out vec4 out_col;
void main()
{
    vec4 color = texture(tex_sampler, vs_tex_coords);
    out_col = vs_color * color.r;
}