// Simple texture display shader.
// Samples a texture and displays it fullscreen (no tonemapping).

#include <frame_uniforms.wgsl>
#include <bindless.wgsl>

// Set 0: Uniforms (storage buffers)
@group(0) @binding(0)
var<storage, read> frame_data: FrameUniforms;

@group(0) @binding(1)
var<storage, read> objects: array<ObjectUniforms>;

@fragment
fn fs_main(@builtin(position) clip_position: vec4f, @location(0) uv: vec2f) -> @location(0) vec4f {
    let params = objects[0];

    let texture_index = u32(params.base_color.a);
    let exposure = params.base_color.r;
    let gamma = params.base_color.g;

    let tex_color = textureSampleLevel(bindless_textures[texture_index], shared_sampler, uv * 0.5, 0.0);

    let exposed = tex_color * exposure;
    let gamma_corrected = pow(exposed, vec4f(1.0 / gamma));

    return vec4f(gamma_corrected.rgb, 1.0);
}
