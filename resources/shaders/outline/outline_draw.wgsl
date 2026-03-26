// Outline draw pass for selection highlights.
// Scales vertices slightly outward from the object center in clip space,
// then uses inverted culling (front faces only) with stencil NOT EQUAL 1.
// The stencil mask ensures the outline only appears outside the original object.
// Depth write is disabled to avoid fighting with the depth prepass.

#include <frame_uniforms.wgsl>

@group(0) @binding(0)
var<storage, read> frame_data: FrameUniforms;

@group(0) @binding(1)
var<storage, read> objects: array<ObjectUniforms>;

// Outline width in normalized device coordinates (pixels / screen height).
const OUTLINE_WIDTH: f32 = 0.004;

struct VertexInput {
    @location(0) position: vec3f,
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
    let world_pos = obj.model * vec4f(in.position, 1.0);

    let clip = frame_data.proj * frame_data.view * world_pos;

    // Compute clip-space direction from the projected object center.
    // This gives a per-vertex direction in screen space for the outline offset.
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
