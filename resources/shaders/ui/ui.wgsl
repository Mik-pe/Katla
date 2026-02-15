// UI vertex shader
// Renders screen-space UI elements
// TODO: Add texture support for font atlas rendering

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

@vertex
fn vs_main(in: UiVertex) -> VertexOutput {
    var out: VertexOutput;

    // Pass through vertex position directly in clip space
    // The vertex data should already be in NDC (-1 to 1)
    // For UI: we expect vertices to be pre-transformed
    out.clip_position = vec4f(in.position, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // Output vertex color directly
    // TODO: Sample from font texture when texture support is added
    // let tex_color = textureSample(font_texture, font_sampler, in.uv);
    // return in.color * tex_color;
    return in.color;
}
