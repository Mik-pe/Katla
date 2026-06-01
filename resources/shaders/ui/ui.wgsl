// UI shader for screen-space rendering using bindless textures
//
// Two rendering modes:
// 1. Instanced: unit quad + per-instance data for simple rects/textured quads
// 2. Non-instanced: per-vertex data for complex geometry (circles, rounded rects, gradients)
//
// The shader selects between modes based on instance_index.
// For instanced draws (instance_count > 1), the vertex shader reads per-instance data.
// For vertex draws (instance_count = 1), the vertex shader reads per-vertex data directly.
//
// ndc_y_flip: 1.0 for Vulkan (Y-down), -1.0 for Metal (Y-up).

struct UiVertex {
    @location(0) position: vec2f,
    @location(1) uv: vec2f,
    @location(2) color: vec4f,
}

struct UnitQuadVertex {
    @location(0) local_pos: vec2f,
}

struct InstanceData {
    position: vec2f,
    size: vec2f,
    uv_min: vec2f,
    uv_max: vec2f,
    color: vec4f,      // packed as u32, decoded in shader
    texture_index: u32,
    clip_rect: vec4f,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) uv: vec2f,
    @location(1) color: vec4f,
    @location(2) @interpolate(flat) texture_index: u32,
    @location(3) clip_rect: vec4f,
}

struct UiUniforms {
    screen_size: vec2f,
    ndc_y_flip: f32,
    texture_index: u32,
}

@group(0) @binding(1) var font_sampler: sampler;
@group(0) @binding(3) var<uniform> uniforms: UiUniforms;

@group(1) @binding(0) var bindless_textures: binding_array<texture_2d<f32>, 4096>;

// Instance data buffer at binding 4
@group(0) @binding(4) var<storage, read> instance_data: array<InstanceData>;

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        return c / 12.92;
    }
    return pow((c + 0.055) / 1.055, 2.4);
}

// Decode a packed u32 color (RGBA as u8) into a vec4f
fn decode_color(packed: u32) -> vec4f {
    let r = f32(packed & 0xFFu) / 255.0;
    let g = f32((packed >> 8u) & 0xFFu) / 255.0;
    let b = f32((packed >> 16u) & 0xFFu) / 255.0;
    let a = f32((packed >> 24u) & 0xFFu) / 255.0;
    return vec4f(r, g, b, a);
}

// Instanced vertex shader for simple quads
@vertex
fn vs_instanced(
    in: UnitQuadVertex,
    @builtin(instance_index) instance_idx: u32,
) -> VertexOutput {
    var out: VertexOutput;

    let inst = instance_data[instance_idx];

    // Transform unit quad: screen_pos = position + local_pos * size
    let screen_pos = inst.position + in.local_pos * inst.size;

    let ndc_x = (screen_pos.x / uniforms.screen_size.x) * 2.0 - 1.0;
    let ndc_y = ((screen_pos.y / uniforms.screen_size.y) * 2.0 - 1.0) * uniforms.ndc_y_flip;

    out.clip_position = vec4f(ndc_x, ndc_y, 0.0, 1.0);

    // Remap UV from unit quad [0,1] to instance UV range
    out.uv = inst.uv_min + in.local_pos * (inst.uv_max - inst.uv_min);

    // Decode color from packed u32 and apply sRGB to linear
    let raw_color = decode_color(u32(inst.color.x));
    out.color = vec4f(
        srgb_to_linear(raw_color.r),
        srgb_to_linear(raw_color.g),
        srgb_to_linear(raw_color.b),
        raw_color.a,
    );

    // Pass per-instance data to fragment shader via varyings
    out.texture_index = inst.texture_index;
    out.clip_rect = inst.clip_rect;

    return out;
}

// Non-instanced vertex shader for complex geometry
@vertex
fn vs_main(in: UiVertex) -> VertexOutput {
    var out: VertexOutput;

    let ndc_x = (in.position.x / uniforms.screen_size.x) * 2.0 - 1.0;
    let ndc_y = ((in.position.y / uniforms.screen_size.y) * 2.0 - 1.0) * uniforms.ndc_y_flip;

    out.clip_position = vec4f(ndc_x, ndc_y, 0.0, 1.0);
    out.uv = in.uv;
    out.color = vec4f(
        srgb_to_linear(in.color.r),
        srgb_to_linear(in.color.g),
        srgb_to_linear(in.color.b),
        in.color.a,
    );
    out.texture_index = 0u;
    out.clip_rect = vec4f(0.0);

    return out;
}

// Shared fragment shader
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let texture = bindless_textures[uniforms.texture_index];
    let tex_color = textureSample(texture, font_sampler, in.uv);
    return in.color * tex_color;
}

// Fragment shader for instanced draws (uses per-instance varyings)
@fragment
fn fs_instanced(in: VertexOutput) -> @location(0) vec4f {
    let texture = bindless_textures[in.texture_index];
    let tex_color = textureSample(texture, font_sampler, in.uv);

    // Shader-based clipping: discard fragments outside clip rect
    let clip = in.clip_rect;
    let pos = in.clip_position;
    // Reconstruct screen position from clip position
    let screen_x = (pos.x + 1.0) * 0.5 * uniforms.screen_size.x;
    let screen_y = (1.0 - pos.y * uniforms.ndc_y_flip) * 0.5 * uniforms.screen_size.y;

    if (screen_x < clip.x || screen_x > clip.x + clip.z ||
        screen_y < clip.y || screen_y > clip.y + clip.w) {
        discard;
    }

    return in.color * tex_color;
}
