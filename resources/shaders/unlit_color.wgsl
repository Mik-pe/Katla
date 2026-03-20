// Unlit color shader for debug lines and gizmos.
//
// Renders colored primitives with no lighting.
// Used by debug line drawing and transform gizmos.
// Uses storage buffer with instance indexing like other materials.

#include <frame_uniforms.wgsl>

// Set 0: Uniforms (storage buffers)
@group(0) @binding(0)
var<storage, read> frame_data: FrameUniforms;

@group(0) @binding(1)
var<storage, read> objects: array<ObjectUniforms>;

// Set 1: Textures (dummy - not used but needed for layout compatibility)
@group(1) @binding(0)
var dummy_texture: texture_2d<f32>;

@group(1) @binding(1)
var dummy_sampler: sampler;

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) color: vec3f,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) color: vec3f,
}

@vertex
fn vs_main(
    in: VertexInput,
    @builtin(instance_index) instance_idx: u32,
) -> VertexOutput {
    var out: VertexOutput;

    let obj = objects[instance_idx];
    let world_pos = obj.model * vec4f(in.position, 1.0);
    out.clip_position = frame_data.proj * frame_data.view * world_pos;
    out.color = in.color;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    return vec4f(in.color, 1.0);
}
