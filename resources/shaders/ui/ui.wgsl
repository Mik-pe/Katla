// UI shader for screen-space rendering
// Supports font atlas and viewport texture sampling

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
// binding 0: font atlas (SAMPLED_IMAGE)
// binding 1: sampler
// binding 2: viewport texture (SAMPLED_IMAGE)
@group(0) @binding(0) var font_atlas: texture_2d<f32>;
@group(0) @binding(1) var font_sampler: sampler;
@group(0) @binding(2) var viewport_texture: texture_2d<f32>;

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
    var tex_color: vec4f;

    // UV.x >= 1.0 signals viewport texture sampling
    // The actual viewport UV is (uv.x - 1.0, uv.y)
    if (in.uv.x >= 1.0) {
        let viewport_uv = vec2f(in.uv.x - 1.0, in.uv.y);
        tex_color = textureSample(viewport_texture, font_sampler, viewport_uv);
    } else {
        // Sample from font atlas
        tex_color = textureSample(font_atlas, font_sampler, in.uv);
    }

    // Multiply texture with vertex color
    return in.color * tex_color;
}
