struct Camera {
    view: mat4x4f,
    projection: mat4x4f,
    inverse_view_projection: mat4x4f,
    position: vec4f,
}

struct Object {
    model: mat4x4f,
    color: vec4f,
    material: vec4f,
    textures: vec4<u32>,
}

@group(0) @binding(0) var<storage, read> camera: Camera;
@group(0) @binding(1) var<storage, read> objects: array<Object>;

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
    @location(2) tangent: vec4f,
    @location(3) uv: vec2f,
}

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) color: vec4f,
}

@vertex
fn vs_main(input: VertexInput, @builtin(instance_index) instance: u32) -> VertexOutput {
    let object = objects[instance];
    var output: VertexOutput;
    output.position = camera.projection * camera.view * object.model * vec4f(input.position, 1.0);
    output.color = object.color;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4f {
    return input.color;
}
