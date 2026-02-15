// UI vertex shader
// Renders screen-space UI elements

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

// Hardcoded screen size for now - will be replaced with push constants later
const SCREEN_WIDTH = 1200.0;
const SCREEN_HEIGHT = 900.0;

@vertex
fn vs_main(in: UiVertex) -> VertexOutput {
    var out: VertexOutput;

    // Convert screen coords (pixels) to NDC (-1 to 1)
    let ndc_x = (in.position.x / SCREEN_WIDTH) * 2.0 - 1.0;
    let ndc_y = (in.position.y / SCREEN_HEIGHT) * 2.0 - 1.0;

    // Flip Y (screen Y is down, NDC Y is up)
    out.clip_position = vec4f(ndc_x, -ndc_y, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // Output vertex color directly (no texture sampling for now)
    return in.color;
}
