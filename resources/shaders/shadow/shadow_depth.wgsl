// Surface-aware scene coverage shared by depth, picking and shadow passes.
#include <surface_coverage.wgsl>
#include <shadow_cascade_data.wgsl>
struct ShadowParams { cascade_index: u32, bias: f32, _pad: vec2f, }
@group(3) @binding(0) var<storage, read> shadow_cascades: array<ShadowCascadeData, 4>;
@group(3) @binding(1) var<storage, read> shadow_params: ShadowParams;
struct VertexInput {
    @location(0) position: vec3f,
    @location(3) tex_coords: vec2f,
    @location(6) tex_coords1: vec2f,
}
fn vertex_position(position: vec4f, tex_coords: vec2f, tex_coords1: vec2f, instance_idx: u32) -> VertexOutput {
    let world_pos = objects[instance_idx].model * position;
    var out: VertexOutput;
    out.clip_position = shadow_cascades[shadow_params.cascade_index].view_proj * world_pos;
    out.instance_idx = instance_idx;
    out.tex_coords = tex_coords;
    out.tex_coords1 = tex_coords1;
    return out;
}
@vertex
fn vs_main(in: VertexInput, @builtin(instance_index) instance_idx: u32) -> VertexOutput {
    return vertex_position(vec4f(in.position, 1.0), in.tex_coords, in.tex_coords1, instance_idx);
}
@vertex
fn vs_position(@location(0) position: vec3f, @builtin(instance_index) instance_idx: u32) -> VertexOutput {
    return vertex_position(vec4f(position, 1.0), vec2f(0.0), vec2f(0.0), instance_idx);
}
