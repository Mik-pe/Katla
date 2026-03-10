// UI shader for screen-space rendering
// Supports font atlas and dynamic texture sampling via push descriptors
//
// Texture modes (signaled by color.a):
// - color.a >= 0: Font/text mode - samples from font_atlas, multiplies with color
// - color.a < 0: Opaque image mode - samples from dynamic_texture, forces alpha = 1.0
//   This is used for viewport, thumbnails, and other images that should not blend.

struct UiVertex {
    @location(0) position: vec2f,  // Screen coordinates (pixels)
    @location(1) uv: vec2f,
    @location(2) color: vec4f,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) uv: vec2f,
    @location(1) color: vec4f,
}

// Screen size uniform buffer (set 0, binding 3)
struct UiUniforms {
    screen_size: vec2f,
    _padding: vec2f,  // WGSL requires 16-byte alignment for uniform buffers
}

// Set 0: Static UI resources (bound once)
// binding 0: font atlas (SAMPLED_IMAGE)
// binding 1: sampler
// binding 3: uniforms (UNIFORM_BUFFER)
@group(0) @binding(0) var font_atlas: texture_2d<f32>;
@group(0) @binding(1) var font_sampler: sampler;
@group(0) @binding(3) var<uniform> uniforms: UiUniforms;

// Set 1: Dynamic texture (push descriptors)
// This set is pushed per-draw-call to switch between viewport and thumbnails
@group(1) @binding(0) var dynamic_texture: texture_2d<f32>;

// Sentinel value for opaque image mode (matches Color::OPAQUE_IMAGE_ALPHA in Rust)
const OPAQUE_IMAGE_ALPHA: f32 = -1.0;

@vertex
fn vs_main(in: UiVertex) -> VertexOutput {
    var out: VertexOutput;

    // Transform from screen coordinates to NDC
    // Screen: (0,0) = top-left, Y increases downward
    // NDC: (-1,+1) = top-left, (+1,-1) = bottom-right
    let ndc_x = (in.position.x / uniforms.screen_size.x) * 2.0 - 1.0;
    let ndc_y = (in.position.y / uniforms.screen_size.y) * 2.0 - 1.0;

    out.clip_position = vec4f(ndc_x, ndc_y, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // Check for opaque image mode (negative alpha signals this)
    if (in.color.a < 0.0) {
        // Opaque image mode - sample from dynamic texture, force alpha = 1.0
        // Used for viewport, thumbnails, and other textures that should not blend
        let tex_color = textureSample(dynamic_texture, font_sampler, in.uv);
        // Use the absolute value of alpha for any tinting (usually 1.0 anyway)
        let tint = vec4f(in.color.rgb, 1.0);
        // Force output alpha to 1.0 to disable blending
        return vec4f(tex_color.rgb * tint.rgb, 1.0);
    } else {
        // Font/text mode - sample from font atlas and multiply with vertex color
        let tex_color = textureSample(font_atlas, font_sampler, in.uv);
        return in.color * tex_color;
    }
}
