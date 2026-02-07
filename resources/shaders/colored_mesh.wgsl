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
    @location(2) tangent: vec4f,
    @location(3) uv: vec2f,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) uv: vec2f,
    @location(1) vs_pos: vec3f,
    @location(2) normal: vec3f,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    // Transform position to clip space
    let world_pos = uniforms.world * vec4f(in.position, 1.0);
    out.clip_position = uniforms.proj * uniforms.view * world_pos;

    // Pass world position to fragment shader for lighting calculations
    out.vs_pos = world_pos.xyz;

    // Pass through UV
    out.uv = in.uv;

    // Transform normal to world space (assuming uniform scaling)
    out.normal = normalize((uniforms.world * vec4f(in.normal, 0.0)).xyz);

    return out;
}

// Simple hardcoded directional light (temporary until uniform buffer integration)
const LIGHT_DIRECTION = vec3f(-0.3, -1.0, -0.2);
const LIGHT_COLOR = vec3f(1.0, 0.95, 0.9);
const AMBIENT_COLOR = vec3f(0.15, 0.15, 0.15);

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // Sample texture with proper WGSL syntax
    let texture_color = textureSample(albedo_texture, albedo_sampler, in.uv);

    // Blend with material color (multiply mode)
    let albedo = texture_color.rgb * uniforms.color.rgb;

    // Normalize inputs
    let normal = normalize(in.normal);
    let light_dir = normalize(-LIGHT_DIRECTION);

    // Ambient lighting
    let ambient = AMBIENT_COLOR * albedo;

    // Diffuse lighting (Lambertian)
    let n_dot_l = max(dot(normal, light_dir), 0.0);
    let diffuse = LIGHT_COLOR * albedo * n_dot_l;

    // Combine lighting components (no specular - requires proper camera position)
    let final_color = ambient + diffuse;

    return vec4f(final_color, uniforms.color.a * texture_color.a);
}
