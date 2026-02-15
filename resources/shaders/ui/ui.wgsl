// UI vertex shader
// Renders screen-space UI elements with texture support

struct UiVertex {
    @location(0) position: vec2f,
    @location(1) uv: vec2f,
    @location(2) color: vec4f,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) uv: vec2f,
    @location(1) color: vec4f,
}

// Set 0: UI textures
@group(0) @binding(0) var font_texture: texture_2d<f32>;
@group(0) @binding(1) var font_sampler: sampler;

@vertex
fn vs_main(in: UiVertex) -> VertexOutput {
    var out: VertexOutput;

    // Pass through vertex position directly in clip space
    // The vertex data should already be in NDC (-1 to 1)
    out.clip_position = vec4f(in.position, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // Sample from font texture
    // Font atlas contains white glyphs with alpha channel
    // For solid color elements, use white texture (alpha = 1.0)
    let tex_color = textureSample(font_texture, font_sampler, in.uv);

    // Multiply texture alpha with vertex color
    // This allows colored text and solid UI elements with the same shader
    return in.color * tex_color;
}
