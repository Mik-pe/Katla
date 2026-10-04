// Application-owned linear PBR factors and vertex-pulled scene geometry.
struct Frame {
    view_projection: mat4x4<f32>,
    camera_position: vec4<f32>,
    light_direction: vec4<f32>,
    light_color: vec4<f32>,
    ambient: vec4<f32>,
}
struct Object {
    model: mat4x4<f32>,
    normal_model: mat4x4<f32>,
    linear_color: vec4<f32>,
    factors: vec4<f32>,
}
struct Vertex { position: vec4<f32>, normal: vec4<f32>, uv: vec4<f32> }
@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var<storage, read> objects: array<Object>;
@group(0) @binding(2) var<storage, read> geometry: array<Vertex>;
struct Vertex_Output {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) @interpolate(flat) object_index: u32,
}
@vertex fn vs_main(@builtin(vertex_index) vertex: u32,
                  @builtin(instance_index) object: u32) -> Vertex_Output {
    let object_data = objects[object];
    let data = geometry[vertex];
    let world = object_data.model * data.position;
    var output: Vertex_Output;
    output.clip_position = frame.view_projection * world;
    output.clip_position.y *= frame.ambient.w;
    output.world_position = world.xyz;
    output.normal = normalize((object_data.normal_model * vec4<f32>(data.normal.xyz, 0.0)).xyz);
    output.object_index = object;
    return output;
}
fn srgb_channel(value: f32) -> f32 {
    return select(1.055 * pow(value, 1.0 / 2.4) - 0.055,
                  value * 12.92, value <= 0.0031308);
}
@fragment fn fs_main(input: Vertex_Output) -> @location(0) vec4<f32> {
    let surface = objects[input.object_index];
    let color = surface.linear_color.rgb;
    let metallic = clamp(surface.factors.x, 0.0, 1.0);
    let roughness = clamp(surface.factors.y, 0.045, 1.0);
    let ao = clamp(surface.factors.z, 0.0, 1.0);
    let normal = normalize(input.normal);
    let view = normalize(frame.camera_position.xyz - input.world_position);
    let light = normalize(-frame.light_direction.xyz);
    let halfway = normalize(view + light);
    let nv = max(dot(normal, view), 0.0);
    let nl = max(dot(normal, light), 0.0);
    let nh = max(dot(normal, halfway), 0.0);
    let vh = max(dot(view, halfway), 0.0);
    let alpha = roughness * roughness;
    let alpha_squared = alpha * alpha;
    let denominator = nh * nh * (alpha_squared - 1.0) + 1.0;
    let distribution = alpha_squared / max(3.14159265359 * denominator * denominator, 0.00001);
    let k = (roughness + 1.0) * (roughness + 1.0) / 8.0;
    let geometry_term = nv / max(nv * (1.0 - k) + k, 0.00001)
                      * nl / max(nl * (1.0 - k) + k, 0.00001);
    let f0 = mix(vec3<f32>(0.04), color, metallic);
    let fresnel = f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - vh, 5.0);
    let specular = distribution * geometry_term * fresnel / max(4.0 * nv * nl, 0.00001);
    let diffuse = (vec3<f32>(1.0) - fresnel) * (1.0 - metallic) * color / 3.14159265359;
    let direct = (diffuse + specular) * frame.light_color.rgb * frame.light_color.w * nl;
    let radiance = frame.ambient.rgb * color * ao + direct;
    let mapped = radiance / (radiance + vec3<f32>(1.0));
    return vec4<f32>(srgb_channel(mapped.r), srgb_channel(mapped.g), srgb_channel(mapped.b), surface.linear_color.a);
}
