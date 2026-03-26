// Stencil mark pass for selection outlines.
// Renders selected entities to the stencil buffer (reference=1).
// Color write is disabled; only depth and stencil are written.
// Depth test uses EQUALS to match the depth prepass result.

#include <frame_uniforms.wgsl>

@group(0) @binding(0)
var<storage, read> frame_data: FrameUniforms;

@group(0) @binding(1)
var<storage, read> objects: array<ObjectUniforms>;

struct VertexInput {
    @location(0) position: vec3f,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
}

@vertex
fn vs_main(
    in: VertexInput,
    @builtin(instance_index) instance_idx: u32,
) -> VertexOutput {
    let obj = objects[instance_idx];
    let world_pos = obj.model * vec4f(in.position, 1.0);

    var out: VertexOutput;
    out.clip_position = frame_data.proj * frame_data.view * world_pos;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // No color output — stencil and depth are written by the rasterizer.
    // Stencil ref=1, ALWAYS replace (configured in pipeline state).
    return vec4f(0.0);
}
