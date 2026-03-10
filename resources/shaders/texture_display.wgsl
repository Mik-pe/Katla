// Simple texture display shader
// Samples a texture and displays it fullscreen (no tonemapping)
// Used as a temporary placeholder until UI rendering is implemented

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) uv: vec2f,
}

struct TonemapUniforms {
    exposure: f32,
    gamma: f32,
    mode: u32,
    texture_index: f32,
    _padding: f32,
}

// Set 0: Storage uniforms (frame_data + objects array)
@group(0) @binding(2) var<uniform> uniforms: TonemapUniforms;

// Set 1: Bindless textures
@group(1) @binding(0) var bindless_textures: binding_array<texture_2d<f32>>;
@group(1) @binding(1) var texture_sampler: sampler;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;

    // Generate fullscreen triangle from vertex index
    // Index 0: (0, 0), Index 1: (2, 0), Index 2: (0, 2)
    let uv = vec2f(
        f32((vertex_index << 1u) & 2u),
        f32(vertex_index & 2u),
    );

    out.clip_position = vec4f(uv * 2.0 - 1.0, 0.0, 1.0);
    out.uv = uv;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // Sample from bindless texture using the texture index
    let texture_index = u32(uniforms.texture_index);
    let tex_color = textureSampleLevel(bindless_textures[texture_index], texture_sampler, in.uv, 0.0);

    // Apply exposure
    let exposed = tex_color * uniforms.exposure;

    // Simple gamma correction
    let gamma_corrected = pow(exposed, vec4f(1.0 / uniforms.gamma));

    return vec4f(gamma_corrected.rgb, 1.0);
}
