// #include lighting_common
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
    let basis = transformed_tangent_frame(mat3x3f(surface.model[0].xyz,surface.model[1].xyz,surface.model[2].xyz),data.normal.xyz,data.tangent);
    var output: Output;
    output.clip = frame.view_projection * world; output.clip.y *= frame.ambient.w;
    output.position = world.xyz; output.normal = basis.normal;
    output.tangent = vec4f(basis.tangent,data.tangent.w*select(-1.0,1.0,determinant(mat3x3f(surface.model[0].xyz,surface.model[1].xyz,surface.model[2].xyz))>=0.0));
    output.color = data.color; output.uv_base = data.uvs[0].xy;
    output.uv_normal = data.uvs[1].xy; output.uv_material = data.uvs[2].xy;
    output.uv_ao = data.uvs[3].xyz; output.uv_emissive = data.uvs[4].xy;
    output.object = object; return output;
}
@fragment fn fs_model(input: Output, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    let surface = objects[input.object];
    let base = textureSample(base_texture, base_sampler, input.uv_base) * surface.base_color * input.color;
    if surface.flags.y == 1u && base.a < surface.emissive.w { discard; }
    if surface.flags.y == 2u && base.a <= 0.0 { discard; }
    let alpha = select(1.0, clamp(base.a,0.0,1.0), surface.flags.y == 2u);
    var radiance = base.rgb;
    if surface.flags.z == 0u {
        let basis = material_tangent_basis(input.position,input.uv_normal,input.normal,input.tangent,(surface.flags.w & 2u)!=0u);
        var normal = basis[2];
        if (surface.flags.w & 1u)!=0u {
            normal=normalized_or(basis*surface_tangent_normal(textureSample(normal_texture,normal_sampler,input.uv_normal).xyz,surface.factors.w),basis[2]);
        }
        normal *= select(-1.0,1.0,front);
        let material = textureSample(material_texture, material_sampler, input.uv_material);
        var roughness = clamp(material.g * surface.factors.y, 0.04, 1.0);
        let metallic = clamp(material.b * surface.factors.x, 0.0, 1.0);
        var f0 = mix(vec3<f32>(0.04), base.rgb, metallic);
        var diffuse_color = base.rgb * (1.0 - metallic);
        if surface.flags.x == 1u {
            f0 = clamp(material.rgb * surface.specular_glossiness.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
            roughness = clamp((1.0 - material.a * surface.specular_glossiness.w) * surface.factors.y, 0.04, 1.0);
            diffuse_color = base.rgb * (1.0 - max(f0.r, max(f0.g, f0.b)));
        }
        let view = normalize(frame.camera_position.xyz - input.position);
        let light = normalize(-frame.light_direction.xyz);
        let direct = pbr_light(normal,view,light,diffuse_color,f0,roughness);
        let ao = mix(1.0, textureSample(occlusion_texture, occlusion_sampler, input.uv_ao.xy).r, input.uv_ao.z) * surface.factors.z;
        radiance = frame.ambient.rgb * base.rgb * ao + direct * frame.light_color.rgb * frame.light_color.w * shadow_visibility(input.position,normal);
        radiance += point_illumination(input.clip.xy,input.position,normal,view,diffuse_color,f0,roughness);
        radiance += textureSample(emissive_texture, emissive_sampler, input.uv_emissive).rgb * surface.emissive.rgb;
    }
    return vec4<f32>(radiance, alpha);
}
