struct Uniforms {
    world: mat4x4f,
    view: mat4x4f,
    proj: mat4x4f,
    color: vec4f,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(0) @binding(1)
var albedo_texture: texture_2d<f32>;

@group(0) @binding(2)
var albedo_sampler: sampler;

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
    @location(2) vert_tangent: vec4f,
    @location(3) vert_texcoord0: vec2f,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) vs_pos: vec3f,
    @location(1) tex_coords: vec2f,
    @location(2) vs_norm: vec3f,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    // Transform position to clip space
    let world_pos = uniforms.world * vec4f(in.position, 1.0);
    out.clip_position = uniforms.proj * uniforms.view * world_pos;

    // Pass world position to fragment shader for lighting calculations
    out.vs_pos = world_pos.xyz;

    // Transform normal to world space (assuming uniform scaling)
    out.vs_norm = normalize((uniforms.world * vec4f(in.normal, 0.0)).xyz);

    // Pass through texture coordinates
    out.tex_coords = in.vert_texcoord0;

    return out;
}

// TODO: Integrate with LightingSystem from katla_app
// Currently using hardcoded values. Need to:
// 1. Add LightingUniforms struct with array of DirectionalLight, PointLight, SpotLight
// 2. Pass light data from application to shader via uniform buffer
// 3. Remove these hardcoded constants and use actual light data
// Simple hardcoded directional light (temporary until uniform buffer integration)
const LIGHT_DIRECTION = vec3f(-0.3, -1.0, -0.2);
const LIGHT_COLOR = vec3f(1.0, 0.95, 0.9);
const AMBIENT_COLOR = vec3f(0.15, 0.15, 0.15);

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // Sample the albedo texture
    let albedo = textureSample(albedo_texture, albedo_sampler, in.tex_coords);

    // Normalize inputs
    let normal = normalize(in.vs_norm);
    let light_dir = normalize(-LIGHT_DIRECTION);

    // Ambient lighting
    let ambient = AMBIENT_COLOR * albedo.rgb;

    // Diffuse lighting (Lambertian)
    let n_dot_l = max(dot(normal, light_dir), 0.0);
    let diffuse = LIGHT_COLOR * albedo.rgb * n_dot_l;

    // Combine lighting components
    let final_color = ambient + diffuse;

    return vec4f(final_color, 1.0);
}
