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

// Push constants for screen size
var<push_constant> screen_size: vec2f;

@vertex
fn vs_main(in: UiVertex) -> VertexOutput {
    var out: VertexOutput;

    // Convert screen coords (pixels) to NDC (-1 to 1)
    // Screen: (0,0) = top-left, Y increases downward
    // Vulkan viewport: maps NDC y=+1 to top, y=-1 to bottom
    // So we need: screen y=0 → ndc y=+1, screen y=height → ndc y=-1
    let ndc_x = (in.position.x / screen_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (in.position.y / screen_size.y) * 2.0;

    out.clip_position = vec4f(ndc_x, ndc_y, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // Output vertex color directly (no texture sampling for now)
    return in.color;
}
