// UI shader for screen-space rendering using bindless textures
//
// All textures (font atlas, viewport, thumbnails) are accessed via a single
// bindless texture array. The texture index is passed per-draw-call as a uniform
// (push constants at buffer 3) since Metal's vertex descriptor does not bind
// texture_index as a vertex attribute.
//
// ndc_y_flip: 1.0 for Vulkan (Y-down), -1.0 for Metal (Y-up).
//
// Texture modes (signaled by color.a):
// - color.a >= 0: Font/text mode - samples from bindless array, multiplies with color
// - color.a < 0: Opaque image mode - samples from bindless array, forces alpha = 1.0
//   This is used for viewport, thumbnails, and other images that should not blend.

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

struct UiUniforms {
    screen_size: vec2f,
    ndc_y_flip: f32,
    texture_index: u32,
}

@group(0) @binding(1) var font_sampler: sampler;
@group(0) @binding(3) var<uniform> uniforms: UiUniforms;

@group(1) @binding(0) var bindless_textures: binding_array<texture_2d<f32>, 4096>;

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        return c / 12.92;
    }
    return pow((c + 0.055) / 1.055, 2.4);
}

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

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let texture = bindless_textures[uniforms.texture_index];
    let tex_color = textureSample(texture, font_sampler, in.uv);
    return in.color * tex_color;
}
