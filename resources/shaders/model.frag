#version 450
layout(binding=0) uniform sampler2D tex_sampler;

in vec2 tex_coords;

out vec4 out_col;

void main()
{
    vec4 color = texture(tex_sampler, tex_coords);
    out_col = vec4(color.rgb, 1.0);
}
