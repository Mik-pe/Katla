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

@fragment
fn fs_main(in: FullscreenVertexOutput) -> @location(0) vec4f {
    let exposure = frame_data.tonemap.x;
    let mode = u32(frame_data.tonemap.z);
    let hdr_texture_idx = u32(frame_data.tonemap.w);

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

    // Output is linear; sRGB conversion is handled by the GPU
    // when writing to sRGB-format textures (both Metal and Vulkan).
    return vec4f(color, 1.0);
}
