// Tonemapping shader for HDR to LDR conversion.
//
// Fullscreen triangle that reads HDR texture and outputs tonemapped LDR.
// Supports multiple tonemapping operators:
// - 0: ACES Filmic (default, cinematic look)
// - 1: Reinhard (simple, preserves colors)
// - 2: TonyMcMapface (popular, good balance)
// - 3: Linear (no tonemapping, just gamma)

#include <frame_uniforms.wgsl>
#include <bindless.wgsl>
#include <fullscreen_triangle.wgsl>

// Set 0: Uniforms (storage buffers)
@group(0) @binding(0)
var<storage, read> frame_data: FrameUniforms;

@group(0) @binding(1)
var<storage, read> objects: array<ObjectUniforms>;

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

fn gamma_correct(x: vec3f, gamma: f32) -> vec3f {
    return pow(x, vec3f(1.0 / gamma));
}

@fragment
fn fs_main(in: FullscreenVertexOutput) -> @location(0) vec4f {
    let tonemap_params = objects[0].base_color;
    let exposure = tonemap_params.r;
    let gamma = tonemap_params.g;
    let mode = u32(tonemap_params.b);
    let hdr_texture_idx = u32(tonemap_params.a);

    // Stencil indicator texture index (emission_idx field, 0 = no indicator)
    let stencil_indicator_idx = u32(objects[0].material_params.w);

    let hdr_color = textureSample(bindless_textures[hdr_texture_idx], shared_sampler, in.uv).rgb;

    var color = hdr_color * exposure;

    switch (mode) {
        case 0u: {
            color = aces_filmic(color);
        }
        case 1u: {
            color = reinhard(color);
        }
        case 2u: {
            color = tony_mcmapface(color);
        }
        case 3u: {
            color = clamp(color, vec3f(0.0), vec3f(1.0));
        }
        default: {
            color = aces_filmic(color);
        }
    }

    color = gamma_correct(color, gamma);

    // Apply wallhack overlay tint where stencil indicator > 0 (occluded selected objects)
    if (stencil_indicator_idx != 0u) {
        let indicator = textureSample(bindless_textures[stencil_indicator_idx], shared_sampler, in.uv).r;
        if (indicator > 0.5) {
            let overlay_color = vec3f(1.0, 0.55, 0.0);
            let overlay_alpha = 0.4;
            color = mix(color, overlay_color, overlay_alpha);
        }
    }

    return vec4f(color, 1.0);
}
