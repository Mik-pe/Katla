// Fullscreen triangle vertex shader utilities.

struct FullscreenVertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) uv: vec2f,
}

@vertex
fn fullscreen_vs(@builtin(vertex_index) vertex_index: u32) -> FullscreenVertexOutput {
    var out: FullscreenVertexOutput;

    let uv = vec2f(
        f32((vertex_index << 1u) & 2u),
        f32(vertex_index & 2u),
    );

    out.clip_position = vec4f(uv * 2.0 - 1.0, 0.0, 1.0);
    out.uv = uv;

    return out;
}
