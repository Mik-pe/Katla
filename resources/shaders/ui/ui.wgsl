// UI shader for screen-space rendering
// Supports font atlas and dynamic texture sampling via push descriptors

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
// Note: binding 2 removed - moved to set 1 for push descriptors
@group(0) @binding(0) var font_atlas: texture_2d<f32>;
@group(0) @binding(1) var font_sampler: sampler;
@group(0) @binding(3) var<uniform> uniforms: UiUniforms;

// Set 1: Dynamic texture (push descriptors)
// This set is pushed per-draw-call to switch between viewport and thumbnails
@group(1) @binding(0) var dynamic_texture: texture_2d<f32>;

@vertex
fn vs_main(in: UiVertex) -> VertexOutput {
    var out: VertexOutput;

    // Transform from screen coordinates to NDC
    // Screen: (0,0) = top-left, Y increases downward
    // NDC: (-1,-1) = bottom-left, (+1,+1) = top-right
    let ndc_x = (in.position.x / uniforms.screen_size.x) * 2.0 - 1.0;
    let ndc_y = (in.position.y / uniforms.screen_size.y) * 2.0 - 1.0;

    out.clip_position = vec4f(ndc_x, ndc_y, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    var tex_color: vec4f;

    // UV.x >= 1.0 signals dynamic texture sampling (viewport or thumbnail)
    // The actual UV is (uv.x - 1.0, uv.y)
    if (in.uv.x >= 1.0) {
        let dynamic_uv = vec2f(in.uv.x - 1.0, in.uv.y);
        tex_color = textureSample(dynamic_texture, font_sampler, dynamic_uv);
        tex_color.a = 1.0;
        return tex_color;
    } else {
        // Sample from font atlas
        tex_color = textureSample(font_atlas, font_sampler, in.uv);
        // Multiply texture with vertex color
        return in.color * tex_color;
    }

}
