// Storage buffer-based shading with instance indexing.
//
// Uses storage buffers for uniform data with instance_index for per-object selection.
// Two descriptor sets: uniforms (set 0) and textures (set 1).

// Frame-level uniforms (shared across all objects)
struct FrameUniforms {
    view: mat4x4f,
    proj: mat4x4f,
}

// Per-object uniforms
struct ObjectUniforms {
    model: mat4x4f,
    color: vec4f,
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
    @location(2) vert_tangent: vec4f,
    @location(3) vert_texcoord0: vec2f,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) world_pos: vec3f,
    @location(1) tex_coords: vec2f,
    @location(2) world_normal: vec3f,
    @location(3) @interpolate(flat) color: vec4f,
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

    out.tex_coords = in.vert_texcoord0;
    out.world_normal = normalize((obj.model * vec4f(in.normal, 0.0)).xyz);
    out.color = obj.color;

    return out;
}

const LIGHT_DIRECTION = vec3f(-0.3, -1.0, -0.2);
const LIGHT_COLOR = vec3f(1.0, 0.95, 0.9);
const AMBIENT_COLOR = vec3f(0.15, 0.15, 0.15);

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let albedo = textureSample(albedo_texture, albedo_sampler, in.tex_coords);

    let normal = normalize(in.world_normal);
    let light_dir = normalize(-LIGHT_DIRECTION);

    let n_dot_l = max(dot(normal, light_dir), 0.0);
    let ambient = AMBIENT_COLOR * albedo.rgb;
    let diffuse = LIGHT_COLOR * albedo.rgb * n_dot_l;

    let final_color = ambient + diffuse;

    return vec4f(final_color * in.color.rgb, in.color.a * albedo.a);
}
