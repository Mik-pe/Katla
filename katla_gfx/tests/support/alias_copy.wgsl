@group(2) @binding(0) var source: texture_2d<f32>;
@vertex fn vs_main(@builtin(vertex_index) vertex: u32) -> @builtin(position) vec4f {
    let uv = vec2f(f32((vertex << 1u) & 2u), f32(vertex & 2u));
    return vec4f(uv * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs_main() -> @location(0) vec4f {
    return textureLoad(source, vec2i(textureDimensions(source)) - vec2i(1), 0);
}
