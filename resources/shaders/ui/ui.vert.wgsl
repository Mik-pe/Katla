// UI vertex shader
// Renders screen-space UI elements

struct UiVertex {
    @location(0) position: vec2f,
    @location(1) uv: vec2f,
    @location(2) color: vec4f,
}

struct Uniforms {
    screen_size: vec2f,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) uv: vec2f,
    @location(1) color: vec4f,
}

@vertex
fn vs_main(in: UiVertex) -> VertexOutput {
    var out: VertexOutput;

    // Convert screen coords (pixels) to NDC (-1 to 1)
    let ndc = (in.position / uniforms.screen_size) * 2.0 - 1.0;

    // Flip Y (screen Y is down, NDC Y is up)
    out.clip_position = vec4f(ndc.x, -ndc.y, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;

    return out;
}
