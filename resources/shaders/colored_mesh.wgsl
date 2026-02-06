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
    @location(1) normal: vec3f,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    // Transform position to clip space
    let world_pos = uniforms.world * vec4f(in.position, 1.0);
    out.clip_position = uniforms.proj * uniforms.view * world_pos;

    // Pass through UV and normal
    out.uv = in.uv;
    out.normal = (uniforms.world * vec4f(in.normal, 0.0)).xyz;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // Sample texture with proper WGSL syntax
    let texture_color = textureSample(albedo_texture, albedo_sampler, in.uv);

    // Blend with material color (multiply mode)
    let blended = texture_color.rgb * uniforms.color.rgb;

    return vec4f(blended, uniforms.color.a * texture_color.a);
}
