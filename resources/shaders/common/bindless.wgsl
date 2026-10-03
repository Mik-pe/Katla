// Shared bindless texture declarations.

@group(1) @binding(0)
var bindless_textures: binding_array<texture_2d<f32>, 4096>;

@group(1) @binding(1)
var shared_sampler: sampler;

fn sample_texture(idx: u32, coords: vec2f) -> vec4f {
    return textureSample(bindless_textures[idx], shared_sampler, coords);
}
