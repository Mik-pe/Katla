struct Uniforms {
    world: mat4x4f,
    view: mat4x4f,
    proj: mat4x4f,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(0) @binding(1)
var albedo_texture: texture_2d<f32>;

@group(0) @binding(2)
var albedo_sampler: sampler;

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
    @location(2) vert_tangent: vec4f,
    @location(3) vert_texcoord0: vec2f,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) vs_pos: vec3f,
    @location(1) tex_coords: vec2f,
    @location(2) vs_norm: vec3f,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    // Transform position to clip space
    let world_pos = uniforms.world * vec4f(in.position, 1.0);
    out.clip_position = uniforms.proj * uniforms.view * world_pos;

    // Note: Original shader had a bug here: "vs_pos = vs_pos;" which was a no-op
    // Preserving original behavior (uninitialized vs_pos)
    out.vs_pos = out.vs_pos;

    // Normal remapped to 0.5-1.0 range (preserving original behavior)
    out.vs_norm = in.normal * 0.5 + 0.5;

    // Pass through texture coordinates
    out.tex_coords = in.vert_texcoord0;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // Sample the albedo texture
    let color = textureSample(albedo_texture, albedo_sampler, in.tex_coords);

    // Output the texture color with full alpha
    return vec4f(color.rgb, 1.0);
}
