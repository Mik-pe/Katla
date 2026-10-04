@group(1) @binding(0) var images: binding_array<texture_2d<f32>,4096>;
@group(1) @binding(1) var image_sampler: sampler;
@fragment fn main(@builtin(position) p: vec4f) -> @location(0) vec4f {
    return textureSample(images[u32(p.x)],image_sampler,p.xy);
}
