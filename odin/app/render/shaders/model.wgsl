struct Frame {
    view_projection: mat4x4<f32>, camera_position: vec4<f32>,
    light_direction: vec4<f32>, light_color: vec4<f32>, ambient: vec4<f32>,
}
struct Model_Object {
    model: mat4x4<f32>, normal_model: mat4x4<f32>,
    base_color: vec4<f32>, factors: vec4<f32>, emissive: vec4<f32>,
    specular_glossiness: vec4<f32>, flags: vec4<u32>,
}
struct Model_Vertex {
    position: vec4<f32>, normal: vec4<f32>, tangent: vec4<f32>, color: vec4<f32>,
    uvs: array<vec4<f32>, 5>,
}
@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var<storage, read> objects: array<Model_Object>;
@group(0) @binding(2) var<storage, read> geometry: array<Model_Vertex>;
@group(1) @binding(0) var base_texture: texture_2d<f32>;
@group(1) @binding(1) var normal_texture: texture_2d<f32>;
@group(1) @binding(2) var material_texture: texture_2d<f32>;
@group(1) @binding(3) var occlusion_texture: texture_2d<f32>;
@group(1) @binding(4) var emissive_texture: texture_2d<f32>;
@group(1) @binding(5) var base_sampler: sampler;
@group(1) @binding(6) var normal_sampler: sampler;
@group(1) @binding(7) var material_sampler: sampler;
@group(1) @binding(8) var occlusion_sampler: sampler;
@group(1) @binding(9) var emissive_sampler: sampler;
struct Output {
    @builtin(position) clip: vec4<f32>,
    @location(0) position: vec3<f32>, @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>, @location(3) color: vec4<f32>,
    @location(4) uv_base: vec2<f32>, @location(5) uv_normal: vec2<f32>,
    @location(6) uv_material: vec2<f32>, @location(7) uv_ao: vec3<f32>,
    @location(8) uv_emissive: vec2<f32>,
    @location(9) @interpolate(flat) object: u32,
}
@vertex fn vs_model(@builtin(vertex_index) vertex: u32, @builtin(instance_index) object: u32) -> Output {
    let data = geometry[vertex]; let surface = objects[object];
    let world = surface.model * data.position;
    let normal = normalize((surface.normal_model * vec4<f32>(data.normal.xyz, 0.0)).xyz);
    let tangent = (surface.model * vec4<f32>(data.tangent.xyz, 0.0)).xyz;
    var output: Output;
    output.clip = frame.view_projection * world; output.clip.y *= frame.ambient.w;
    output.position = world.xyz; output.normal = normal;
    output.tangent = vec4<f32>(normalize(tangent - normal * dot(normal, tangent)), data.tangent.w * select(-1.0, 1.0, determinant(mat3x3<f32>(surface.model[0].xyz, surface.model[1].xyz, surface.model[2].xyz)) >= 0.0));
    output.color = data.color; output.uv_base = data.uvs[0].xy;
    output.uv_normal = data.uvs[1].xy; output.uv_material = data.uvs[2].xy;
    output.uv_ao = data.uvs[3].xyz; output.uv_emissive = data.uvs[4].xy;
    output.object = object; return output;
}
@fragment fn fs_model(input: Output, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    let surface = objects[input.object];
    let base = textureSample(base_texture, base_sampler, input.uv_base) * surface.base_color * input.color;
    if surface.flags.y == 1u && base.a < surface.emissive.w { discard; }
    let alpha = select(1.0, base.a, surface.flags.y == 2u);
    var radiance = base.rgb;
    if surface.flags.z == 0u {
        let sampled = textureSample(normal_texture, normal_sampler, input.uv_normal).xyz * 2.0 - 1.0;
        let n = normalize(input.normal) * select(-1.0, 1.0, front);
        let t = normalize(input.tangent.xyz);
        let b = normalize(cross(n, t)) * input.tangent.w;
        var normal = n;
        if surface.flags.w != 0u { normal = normalize(mat3x3<f32>(t, b, n) * vec3<f32>(sampled.xy * surface.factors.w, sampled.z)); }
        let material = textureSample(material_texture, material_sampler, input.uv_material);
        var roughness = clamp(material.g * surface.factors.y, 0.045, 1.0);
        let metallic = clamp(material.b * surface.factors.x, 0.0, 1.0);
        var f0 = mix(vec3<f32>(0.04), base.rgb, metallic);
        var diffuse_color = base.rgb * (1.0 - metallic);
        if surface.flags.x == 1u {
            f0 = clamp(material.rgb * surface.specular_glossiness.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
            roughness = clamp((1.0 - material.a * surface.specular_glossiness.w) * surface.factors.y, 0.045, 1.0);
            diffuse_color = base.rgb * (1.0 - max(f0.r, max(f0.g, f0.b)));
        }
        let view = normalize(frame.camera_position.xyz - input.position);
        let light = normalize(-frame.light_direction.xyz); let half_vector = normalize(view + light);
        let nv = max(dot(normal, view), 0.0); let nl = max(dot(normal, light), 0.0);
        let nh = max(dot(normal, half_vector), 0.0); let vh = max(dot(view, half_vector), 0.0);
        let a = roughness * roughness; let a2 = a * a;
        let d = nh * nh * (a2 - 1.0) + 1.0;
        let distribution = a2 / max(3.14159265359 * d * d, 0.00001);
        let k = (roughness + 1.0) * (roughness + 1.0) / 8.0;
        let geometry_term = nv / max(nv * (1.0 - k) + k, 0.00001) * nl / max(nl * (1.0 - k) + k, 0.00001);
        let fresnel = f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - vh, 5.0);
        let specular = distribution * geometry_term * fresnel / max(4.0 * nv * nl, 0.00001);
        let diffuse = (vec3<f32>(1.0) - fresnel) * diffuse_color / 3.14159265359;
        let ao = mix(1.0, textureSample(occlusion_texture, occlusion_sampler, input.uv_ao.xy).r, input.uv_ao.z) * surface.factors.z;
        radiance = frame.ambient.rgb * base.rgb * ao + (diffuse + specular) * frame.light_color.rgb * frame.light_color.w * nl;
        radiance += textureSample(emissive_texture, emissive_sampler, input.uv_emissive).rgb * surface.emissive.rgb;
    }
    let mapped = select(radiance / (radiance + vec3<f32>(1.0)), radiance, surface.flags.z != 0u);
    return vec4<f32>(mapped, alpha);
}
