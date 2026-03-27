// Stencil indicator pass — writes white (1.0) to an R8 texture where stencil == 2.
// The tonemap shader samples this texture to apply the wallhack overlay tint in-shader,
// avoiding alpha blending in HDR space which produces incorrect results.

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
    return vec4f(1.0, 0.0, 0.0, 1.0);
}
