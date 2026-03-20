// Shared bindless texture declarations.

@group(1) @binding(0)
var bindless_textures: binding_array<texture_2d<f32>, 256>;

@group(1) @binding(1)
var shared_sampler: sampler;
