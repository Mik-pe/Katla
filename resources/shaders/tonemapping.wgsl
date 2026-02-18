// Tonemapping shader for HDR to LDR conversion.
//
// Fullscreen triangle that reads HDR texture and outputs tonemapped LDR.
// Supports multiple tonemapping operators:
// - 0: ACES Filmic (default, cinematic look)
// - 1: Reinhard (simple, preserves colors)
// - 2: TonyMcMapface (popular, good balance)
// - 3: Linear (no tonemapping, just gamma)
//
// Uniforms (push constants or uniform buffer):
// - exposure: HDR exposure multiplier
// - gamma: Gamma correction value (typically 2.2)
// - mode: Tonemapping operator selection

struct TonemappingUniforms {
    exposure: f32,
    gamma: f32,
    mode: u32,
    _pad: u32,
}

@group(0) @binding(0)
var<uniform> params: TonemappingUniforms;

@group(0) @binding(1)
var hdr_texture: texture_2d<f32>;

@group(0) @binding(2)
var hdr_sampler: sampler;

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) uv: vec2f,
}

// Fullscreen triangle vertex shader
// Generates a single triangle that covers the entire screen
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;

    // Generate fullscreen triangle
    // Vertex 0: (-1, -1) -> UV (0, 1)
    // Vertex 1: (3, -1)  -> UV (2, 1)
    // Vertex 2: (-1, 3)  -> UV (0, -1)
    let uv = vec2f(
        f32((vertex_index << 1u) & 2u),
        f32(vertex_index & 2u),
    );

    out.clip_position = vec4f(uv * 2.0 - 1.0, 0.0, 1.0);
    // Flip Y for Vulkan coordinate system
    out.uv = vec2f(uv.x, 1.0 - uv.y);

    return out;
}

// === Tonemapping Operators ===

// ACES Filmic tonemapping (Academy Color Encoding System)
// Cinematic look with good highlight rolloff
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

// Reinhard tonemapping
// Simple and preserves colors well
fn reinhard(x: vec3f) -> vec3f {
    return x / (x + vec3f(1.0));
}

// TonyMcMapface tonemapping
// Popular operator with good balance of contrast and highlight preservation
// Based on https://www.shadertoy.com/view/WdjSW3
fn tony_mcmapface(x: vec3f) -> vec3f {
    // TonyMcMapface uses a specific curve
    // This is a simplified approximation

    // Start with Reinhard-like base
    let a = x * (2.51 * x + 0.03) / (x * (2.43 * x + 0.59) + 0.14);

    // Apply contrast enhancement
    let contrast = 1.2;
    let b = pow(a, vec3f(contrast));

    // Smooth shoulder for highlights
    let c = 1.0 - exp(-b * 1.5);

    return clamp(c, vec3f(0.0), vec3f(1.0));
}

// Apply gamma correction
fn gamma_correct(x: vec3f, gamma: f32) -> vec3f {
    return pow(x, vec3f(1.0 / gamma));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // Sample HDR texture
    let hdr_color = textureSample(hdr_texture, hdr_sampler, in.uv).rgb;

    // Apply exposure
    var color = hdr_color * params.exposure;

    // Apply tonemapping based on mode
    switch (params.mode) {
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

    // Apply gamma correction
    color = gamma_correct(color, params.gamma);

    return vec4f(color, 1.0);
}
