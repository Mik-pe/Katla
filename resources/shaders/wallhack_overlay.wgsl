// Wallhack overlay shader.
//
// Fullscreen pass that reads the LDR viewport texture and the stencil indicator
// R8 mask, then applies an orange tint where the stencil indicator is active
// (occluded parts of selected objects).

#include <frame_uniforms.wgsl>
#include <bindless.wgsl>
#include <fullscreen_triangle.wgsl>
#include <outline_params.wgsl>

@group(0) @binding(0)
var<storage, read> frame_data: FrameUniforms;

@group(0) @binding(1)
var<storage, read> objects: array<ObjectUniforms>;

@fragment
fn fs_main(in: FullscreenVertexOutput) -> @location(0) vec4f {
    let params = objects[0].base_color;
    let ldr_texture_idx = u32(params.r);
    let indicator_texture_idx = u32(params.g);

    if (indicator_texture_idx == 0u) {
        return textureSample(bindless_textures[ldr_texture_idx], shared_sampler, in.uv);
    }

    let ldr_color = textureSample(bindless_textures[ldr_texture_idx], shared_sampler, in.uv);
    let indicator = textureSample(bindless_textures[indicator_texture_idx], shared_sampler, in.uv).r;

    if (indicator > 0.5) {
        let tinted = mix(ldr_color.rgb, OUTLINE_COLOR, WALLHACK_ALPHA);
        return vec4f(tinted, 1.0);
    }

    return ldr_color;
}
