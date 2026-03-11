// UI shader for screen-space rendering using bindless textures
//
// All textures (font atlas, viewport, thumbnails) are accessed via a single
// bindless texture array. The texture index is passed per-vertex.
//
// Texture modes (signaled by color.a):
// - color.a >= 0: Font/text mode - samples from bindless array, multiplies with color
// - color.a < 0: Opaque image mode - samples from bindless array, forces alpha = 1.0
//   This is used for viewport, thumbnails, and other images that should not blend.

struct UiVertex {
    @location(0) position: vec2f,  // Screen coordinates (pixels)
    @location(1) uv: vec2f,
    @location(2) color: vec4f,
    @location(3) texture_index: u32,  // Index into bindless texture array
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) uv: vec2f,
    @location(1) color: vec4f,
    @location(2) texture_index: u32,
}

// Screen size uniform buffer (set 0, binding 3)
struct UiUniforms {
    screen_size: vec2f,
    _padding: vec2f,  // WGSL requires 16-byte alignment for uniform buffers
}

// Set 0: Static UI resources (bound once)
// binding 1: sampler
// binding 3: uniforms (UNIFORM_BUFFER)
@group(0) @binding(1) var font_sampler: sampler;
@group(0) @binding(3) var<uniform> uniforms: UiUniforms;

// Set 1: Bindless texture array (shared with 3D materials)
// binding 0: texture_2d array (4096 textures)
@group(1) @binding(0) var bindless_textures: binding_array<texture_2d<f32>, 4096>;

// Sentinel value for opaque image mode (matches Color::OPAQUE_IMAGE_ALPHA in Rust)
// This is approximately 1.0/255.0 which is the smallest non-zero alpha that can be
// reliably distinguished from 0.0 after round-tripping through byte conversion
const OPAQUE_IMAGE_ALPHA: f32 = 1.0 / 255.0;

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
    out.texture_index = in.texture_index;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // Sample from the bindless texture array using the per-vertex index
    let texture = bindless_textures[in.texture_index];

    // Check for opaque image mode (special alpha value signals this)
    if (in.color.a <= OPAQUE_IMAGE_ALPHA) {
        // Opaque image mode - sample from bindless array, force alpha = 1.0
        // Used for viewport, thumbnails, and other textures that should not blend
        let tex_color = textureSample(texture, font_sampler, in.uv);
        // Use the absolute value of alpha for any tinting (usually 1.0 anyway)
        let tint = vec4f(in.color.rgb, 1.0);
        // Force output alpha to 1.0 to disable blending
        return vec4f(tex_color.rgb * tint.rgb, 1.0);
    } else {
        // Font/text mode - sample from bindless array and multiply with vertex color
        let tex_color = textureSample(texture, font_sampler, in.uv);
        return in.color * tex_color;
    }
}
