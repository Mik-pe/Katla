// UI fragment shader
// Samples texture and applies vertex color

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) uv: vec2f,
    @location(1) color: vec4f,
}

@group(0) @binding(1)
var font_atlas: texture_2d<f32>;

@group(0) @binding(2)
var font_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // Sample texture
    let tex_color = textureSample(font_atlas, font_sampler, in.uv);

    // Multiply texture with vertex color
    return in.color * tex_color;
}
