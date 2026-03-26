// Skinned outline draw pass for selection highlights.
// Same as outline_draw.wgsl but with skeletal animation support.

#include <frame_uniforms.wgsl>

@group(0) @binding(0)
var<storage, read> frame_data: FrameUniforms;

@group(0) @binding(1)
var<storage, read> objects: array<ObjectUniforms>;

@group(2) @binding(0)
var<storage, read> joint_matrices: array<mat4x4f>;

// Outline width in normalized device coordinates (pixels / screen height).
const OUTLINE_WIDTH: f32 = 0.004;

struct VertexInput {
    @location(0) position: vec3f,
    @location(4) joint_indices: vec4u,
    @location(5) joint_weights: vec4f,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) @interpolate(flat) instance_idx: u32,
}

@vertex
fn vs_main(
    in: VertexInput,
    @builtin(instance_index) instance_idx: u32,
) -> VertexOutput {
    let obj = objects[instance_idx];

    var skinned_pos = vec4f(0.0);
    let weights = in.joint_weights;
    for (var i = 0u; i < 4u; i = i + 1u) {
        let joint_idx = in.joint_indices[i];
        let joint_mat = joint_matrices[joint_idx];
        skinned_pos = skinned_pos + weights[i] * (joint_mat * vec4f(in.position, 1.0));
    }
    skinned_pos.w = 1.0;

    let world_pos = obj.model * skinned_pos;
    let clip = frame_data.proj * frame_data.view * world_pos;

    // Compute clip-space direction from the projected object center.
    let obj_origin_clip = frame_data.proj * frame_data.view * obj.model[3];
    let clip_dir = clip.xyz - obj_origin_clip.xyz;
    let clip_dir_len = length(clip_dir.xy);

    var final_clip = clip;
    if clip_dir_len > 0.001 {
        let offset = normalize(clip_dir.xy) * OUTLINE_WIDTH * clip.w;
        final_clip = vec4f(clip.x + offset.x, clip.y + offset.y, clip.z, clip.w);
    }

    var out: VertexOutput;
    out.clip_position = final_clip;
    out.instance_idx = instance_idx;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    return vec4f(1.0, 0.55, 0.0, 1.0);
}
