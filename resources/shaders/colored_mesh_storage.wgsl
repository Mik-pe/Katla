// Storage buffer-based shading with instance indexing.
//
// Uses storage buffers for uniform data with instance_index for per-object selection.
// Two descriptor sets: uniforms (set 0) and textures (set 1).
//
// NOTE: FrameUniforms and ObjectUniforms must match the Rust structs exactly!

// Frame-level uniforms (shared across all objects) - must match Rust FrameUniforms
struct FrameUniforms {
    view: mat4x4f,
    proj: mat4x4f,
    inv_view_proj: mat4x4f,  // Inverse VP for sky/world ray calculation
    camera_position: vec4f,
    light_direction: vec4f,
    light_color: vec4f,
    light_intensity: vec4f,
}

// Per-object uniforms - must match Rust ObjectUniforms
struct ObjectUniforms {
    model: mat4x4f,
    base_color: vec4f,
    material_params: vec4f,  // metallic, roughness, ao, padding (not used by this shader)
}

// Set 0: Uniforms (storage buffers)
@group(0) @binding(0)
var<storage, read> frame_data: FrameUniforms;

@group(0) @binding(1)
var<storage, read> objects: array<ObjectUniforms>;

// Set 1: Textures
@group(1) @binding(0)
var albedo_texture: texture_2d<f32>;

@group(1) @binding(1)
var albedo_sampler: sampler;

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
    @location(2) tangent: vec4f,
    @location(3) uv: vec2f,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) uv: vec2f,
    @location(1) normal: vec3f,
    @location(2) world_pos: vec3f,
    @location(3) @interpolate(flat) base_color: vec4f,
}

@vertex
fn vs_main(
    in: VertexInput,
    @builtin(instance_index) instance_idx: u32,
) -> VertexOutput {
    var out: VertexOutput;

    let obj = objects[instance_idx];

    let world_pos = obj.model * vec4f(in.position, 1.0);
    out.world_pos = world_pos.xyz;
    out.clip_position = frame_data.proj * frame_data.view * world_pos;

    out.uv = in.uv;
    out.normal = normalize((obj.model * vec4f(in.normal, 0.0)).xyz);
    out.base_color = obj.base_color;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let texture_color = textureSample(albedo_texture, albedo_sampler, in.uv);

    // Use light direction from frame uniforms (normalized, points TO the light)
    let light_dir = normalize(frame_data.light_direction.xyz);
    let normal = normalize(in.normal);

    let n_dot_l = max(dot(normal, light_dir), 0.0);

    // Simple diffuse + ambient lighting
    let ambient = vec3f(0.15) * texture_color.rgb;
    let diffuse = frame_data.light_color.rgb * frame_data.light_intensity.x * texture_color.rgb * n_dot_l;

    let final_color = ambient + diffuse;

    return vec4f(final_color * in.base_color.rgb, in.base_color.a * texture_color.a);
}
