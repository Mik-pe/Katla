// Scene coverage for depth, shadow and object-ID fragment entries.
#include <frame_uniforms.wgsl>
#include <bindless.wgsl>
#include <material_surface.wgsl>
@group(5) @binding(0) var albedo_sampler: sampler;
@group(0) @binding(1) var<storage, read> objects: array<ObjectUniforms>;
@group(0) @binding(2) var<storage, read> surfaces: array<SurfaceParameters>;
struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) @interpolate(flat) instance_idx: u32,
    @location(1) tex_coords: vec2f,
    @location(2) tex_coords1: vec2f,
}
fn coverage(in: VertexOutput) {
    let obj = objects[in.instance_idx];
    let surface = surfaces[in.instance_idx];
    let uv = material_uv(surface, 0u, in.tex_coords, in.tex_coords1);
    let alpha = textureSample(bindless_textures[obj.texture_indices.x], albedo_sampler, uv).a * obj.base_color.a;
    let covered_alpha = surface_alpha(alpha, surfaces[in.instance_idx]);
}
@fragment
fn fs_depth(in: VertexOutput) { coverage(in); }
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4u {
    coverage(in);
    return vec4u(in.instance_idx + 1u, 0u, 0u, 1u);
}
