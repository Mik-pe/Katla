// Depth-only shader for CSM shadow map rendering.
// Each cascade is rendered as a separate draw with cascade_index set via a storage buffer.

#include <shadow_cascade_data.wgsl>

struct ShadowParams {
    cascade_index: u32,
    bias: f32,
    _pad: vec2f,
}

@group(0) @binding(1)
var<storage, read> objects: array<ObjectUniforms>;

@group(2) @binding(0)
var<storage, read> shadow_cascades: array<ShadowCascadeData, 4>;

@group(2) @binding(1)
var<storage, read> shadow_params: ShadowParams;

struct VertexInput {
    @location(0) position: vec3f,
}

@vertex
fn vs_main(
    in: VertexInput,
    @builtin(instance_index) instance_idx: u32,
) -> @builtin(position) vec4f {
    let obj = objects[instance_idx];
    let cascade = shadow_cascades[shadow_params.cascade_index];

    let world_pos = obj.model * vec4f(in.position, 1.0);
    return cascade.view_proj * world_pos;
}

@fragment
fn fs_main() {
}
