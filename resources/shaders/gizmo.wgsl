// Gizmo shader for 3D transform manipulation.
//
// Renders colored lines/shapes for translate/rotate/scale gizmos.
// Unlit rendering - always visible on top of scene.
// Uses storage buffer with instance indexing like other materials.

// Frame-level uniforms
struct FrameUniforms {
    view: mat4x4f,
    proj: mat4x4f,
    inv_view_proj: mat4x4f,
    camera_position: vec4f,
    light_direction: vec4f,
    light_color: vec4f,
    light_intensity: vec4f,
}

// Per-object uniforms (matches ObjectUniforms in Rust)
struct ObjectUniforms {
    model: mat4x4f,
    base_color: vec4f,
    material_params: vec4f,  // Not used by gizmos
}

@group(0) @binding(0)
var<storage, read> frame_data: FrameUniforms;

@group(0) @binding(1)
var<storage, read> objects: array<ObjectUniforms>;

// Set 1: Textures (dummy - gizmos don't use textures but need the layout)
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
    // Unlit - just output the vertex color
    return vec4f(in.color, 1.0);
}
