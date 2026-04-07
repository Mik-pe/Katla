// Billboard depth prepass for GPU picking.
// Replicates the camera-facing vertex transform from billboard.wgsl
// but outputs instance_index + 1 as vec4u for picking.
// Fragment stage samples bindless icon texture and discards transparent pixels.

#include <frame_uniforms.wgsl>
#include <bindless.wgsl>

// Set 0: Uniforms (storage buffers)
@group(0) @binding(0)
var<storage, read> frame_data: FrameUniforms;

@group(0) @binding(1)
var<storage, read> objects: array<ObjectUniforms>;

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
    @location(2) vert_tangent: vec4f,
    @location(3) vert_texcoord0: vec2f,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) tex_coords: vec2f,
    @location(1) @interpolate(flat) instance_idx: u32,
}

@vertex
fn vs_main(
    in: VertexInput,
    @builtin(instance_index) instance_idx: u32,
) -> VertexOutput {
    var out: VertexOutput;

    let obj = objects[instance_idx];

    // Billboard center from model matrix translation
    let center = obj.model[3].xyz;

    // Extract camera right and up from view matrix (rows = columns of inverse).
    // Column-major: row i = vec3f(V[0][i], V[1][i], V[2][i]).
    let right = vec3f(frame_data.view[0][0], frame_data.view[1][0], frame_data.view[2][0]);
    let up = vec3f(frame_data.view[0][1], frame_data.view[1][1], frame_data.view[2][1]);

    // Offset quad vertices using input position.xy (unit quad in [-0.5, 0.5])
    let world_pos = center + right * in.position.x + up * in.position.y;

    out.clip_position = frame_data.proj * frame_data.view * vec4f(world_pos, 1.0);
    out.tex_coords = in.vert_texcoord0;
    out.instance_idx = instance_idx;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4u {
    let obj = objects[in.instance_idx];

    let albedo_idx = u32(obj.material_params.w);
    let albedo_sample = textureSample(bindless_textures[albedo_idx], shared_sampler, in.tex_coords);

    let alpha = albedo_sample.a * obj.base_color.a;

    if (alpha < 0.5) {
        discard;
    }

    // Encode instance_index + 1 into the R channel for picking.
    // Value 0 (cleared) means no object was hit.
    return vec4u(in.instance_idx + 1u, 0u, 0u, 1u);
}
