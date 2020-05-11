#version 450
layout(binding=0) uniform sampler2D tex_sampler;

layout(location=0) in vec2 vs_tex_coords;
layout(location=1) in vec4 vs_color;

layout(location=0) out vec4 out_col;
void main()
{
    vec4 color = texture(tex_sampler, vs_tex_coords);
    out_col = vs_color * color.r;
}
