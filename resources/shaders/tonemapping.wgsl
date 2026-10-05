// Tonemapping shader for HDR to LDR conversion.
//
// Fullscreen triangle that reads HDR texture and outputs tonemapped LDR.
// Supports multiple tonemapping operators:
// - 0: ACES Filmic (default, cinematic look)
// - 1: Reinhard (simple, preserves colors)
// - 2: TonyMcMapface (popular, good balance)
// - 3: Linear (no tonemapping, exposure only)

#include <frame_uniforms.wgsl>
#include <bindless.wgsl>
#include <fullscreen_triangle.wgsl>

// Set 0: Uniforms (storage buffers)
@group(0) @binding(0)
var<storage, read> frame_data: FrameUniforms;

// === Tonemapping Operators ===

fn aces_filmic(x: vec3f) -> vec3f {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;

    return clamp(
        (x * (a * x + b)) / (x * (c * x + d) + e),
        vec3f(0.0),
        vec3f(1.0),
    );
}

fn reinhard(x: vec3f) -> vec3f {
    return x / (x + vec3f(1.0));
}

fn tony_mcmapface(x: vec3f) -> vec3f {
    let a = aces_filmic(x);
    let contrast = 1.2;
    let b = pow(a, vec3f(contrast));
    let c = 1.0 - exp(-b * 1.5);
    return clamp(c, vec3f(0.0), vec3f(1.0));
}

fn map_color(hdr: vec3f) -> vec3f {
    let color = hdr * frame_data.tonemap.x;
    switch u32(frame_data.tonemap.z) {
        case 0u: { return aces_filmic(color); }
        case 1u: { return reinhard(color); }
        case 2u: { return tony_mcmapface(color); }
        default: { return clamp(color, vec3f(0.0), vec3f(1.0)); }
    }
}

fn encode_srgb(color: vec3f) -> vec3f {
    return select(color * 12.92, 1.055 * pow(color, vec3f(1.0 / 2.4)) - 0.055,
                  color > vec3f(0.0031308));
}

fn decode_srgb(color: vec3f) -> vec3f {
    return select(color / 12.92, pow((color + 0.055) / 1.055, vec3f(2.4)),
                  color > vec3f(0.04045));
}

fn mapped_texel(index: u32, coord: vec2i) -> vec3f {
    let dimensions = vec2i(textureDimensions(bindless_textures[index]));
    let bounded = clamp(coord, vec2i(0), dimensions - vec2i(1));
    return encode_srgb(map_color(textureLoad(bindless_textures[index], bounded, 0).rgb));
}

// Filter display colors, not HDR radiance: a bright texel must not overwhelm
// coverage after tonemapping. UI is composited later and keeps its own sharpness.
fn filtered_display(index: u32, pixel: vec2f) -> vec3f {
    let base = vec2i(floor(pixel));
    let weight = fract(pixel);
    let top = mix(mapped_texel(index, base), mapped_texel(index, base + vec2i(1, 0)), weight.x);
    let bottom = mix(mapped_texel(index, base + vec2i(0, 1)),
                     mapped_texel(index, base + vec2i(1, 1)), weight.x);
    return mix(top, bottom, weight.y);
}

fn luminance(color: vec3f) -> f32 {
    return dot(color, vec3f(0.299, 0.587, 0.114));
}

@fragment
fn fs_main(in: FullscreenVertexOutput) -> @location(0) vec4f {
    let index = u32(frame_data.tonemap.w);
    let pixel = in.uv * vec2f(textureDimensions(bindless_textures[index])) - vec2f(0.5);
    let coord = vec2i(round(pixel));
    let center = mapped_texel(index, coord);
    let nw = luminance(mapped_texel(index, coord + vec2i(-1, -1)));
    let ne = luminance(mapped_texel(index, coord + vec2i(1, -1)));
    let sw = luminance(mapped_texel(index, coord + vec2i(-1, 1)));
    let se = luminance(mapped_texel(index, coord + vec2i(1, 1)));
    let mid = luminance(center);
    let low = min(mid, min(min(nw, ne), min(sw, se)));
    let high = max(mid, max(max(nw, ne), max(sw, se)));
    if high - low < max(0.03125, high * 0.125) {
        return vec4f(decode_srgb(center), 1.0);
    }
    let gradient = vec2f(-(nw + ne - sw - se), nw + sw - ne - se);
    let reduction = max((nw + ne + sw + se) * 0.03125, 0.0078125);
    let direction = clamp(gradient / (min(abs(gradient.x), abs(gradient.y)) + reduction),
                          vec2f(-8.0), vec2f(8.0));
    let narrow = 0.5 * (filtered_display(index, pixel - direction / 6.0)
                     + filtered_display(index, pixel + direction / 6.0));
    let wide = narrow * 0.5 + 0.25 * (filtered_display(index, pixel - direction * 0.5)
                                   + filtered_display(index, pixel + direction * 0.5));
    let wide_luma = luminance(wide);
    let result = select(wide, narrow, wide_luma < low || wide_luma > high);
    return vec4f(decode_srgb(result), 1.0);
}
