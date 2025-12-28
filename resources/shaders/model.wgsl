struct Uniforms {
    world: mat4x4f,
    view: mat4x4f,
    proj: mat4x4f,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) uv: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec3<f32>,
    @location(3) bitangent: vec3<f32>,
    @location(4) uv: vec2<f32>,
}


@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    // Transform position to world space
    let world_pos = uniforms.world * vec4<f32>(in.position, 1.0);
    out.world_position = world_pos.xyz;

    // Transform to clip space
    out.clip_position = uniforms.proj * uniforms.view * world_pos;
    // out.clip_position.y = -out.clip_position.y;

    // Transform normal and tangent to world space
    let normal_matrix = mat3x3<f32>(
        uniforms.world[0].xyz,
        uniforms.world[1].xyz,
        uniforms.world[2].xyz
    );

    out.normal = normalize(normal_matrix * in.normal);
    out.tangent = normalize(normal_matrix * in.tangent.xyz);

    // Calculate bitangent (w component of tangent controls handedness)
    out.bitangent = cross(out.normal, out.tangent) * in.tangent.w;

    out.uv = in.uv;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Simple shading using normal
    let light_dir = normalize(vec3<f32>(1.0, 1.0, 1.0));
    let diffuse = max(dot(in.normal, light_dir), 0.0);

    return vec4<f32>(vec3<f32>(diffuse), 1.0);
}
