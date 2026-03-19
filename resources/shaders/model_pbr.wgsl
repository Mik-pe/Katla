// Full PBR shader with BINDLESS TEXTURES and HDR output.
//
// Uses storage buffers for uniform data with instance_index for per-object selection.
// Two descriptor sets: uniforms (set 0) and bindless textures (set 1).
//
// Bindless architecture:
// - Set 1, Binding 0: texture_2d array (4096 textures)
// - Set 1, Binding 1: shared sampler
// - Texture indices come from per-object ObjectUniforms
//
// Implements:
// - Metallic/Roughness workflow with texture support
// - Tangent-space normal mapping
// - Fresnel-Schlick approximation
// - GGX distribution for specular
// - Geometry/visibility function (Smith)
// - Directional lighting with camera position
// - HDR linear output (NO tonemapping - handled by post-process pass)

// Frame-level uniforms (shared across all objects)
struct FrameUniforms {
    view: mat4x4f,
    proj: mat4x4f,
    inv_view_proj: mat4x4f,  // Inverse VP for sky/world ray calculation
    camera_position: vec4f,
    light_direction: vec4f,
    light_color: vec4f,
    light_intensity: vec4f,
}

// Per-object uniforms (updated for bindless)
struct ObjectUniforms {
    model: mat4x4f,
    base_color: vec4f,
    material_params: vec4f,     // x=metallic, y=roughness, z=ao, w=emission_idx
    texture_indices: vec4<u32>, // x=albedo, y=normal, z=mr, w=ao (bindless indices)
}

// Set 0: Uniforms (storage buffers)
@group(0) @binding(0)
var<storage, read> frame_data: FrameUniforms;

@group(0) @binding(1)
var<storage, read> objects: array<ObjectUniforms>;

// Set 1: Bindless textures
// Binding 0: Texture array (up to 4096 textures)
@group(1) @binding(0)
var bindless_textures: binding_array<texture_2d<f32>, 4096>;

// Binding 1: Shared sampler
@group(1) @binding(1)
var shared_sampler: sampler;

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
    @location(2) vert_tangent: vec4f,  // w component = handedness
    @location(3) vert_texcoord0: vec2f,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) world_pos: vec3f,
    @location(1) tex_coords: vec2f,
    @location(2) world_normal: vec3f,
    @location(3) world_tangent: vec3f,
    @location(4) world_bitangent: vec3f,
    @location(5) @interpolate(flat) instance_idx: u32,
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

    // Calculate world-space normal/tangent using the upper-left 3x3 of model matrix
    let normal_matrix = mat3x3f(
        obj.model[0].xyz,
        obj.model[1].xyz,
        obj.model[2].xyz,
    );

    // Gram-Schmidt reorthogonalization for proper tangent frame
    let T = normalize(normal_matrix * in.vert_tangent.xyz);
    let N = normalize(normal_matrix * in.normal);
    out.world_normal = N;

    // Calculate bitangent using tangent and normal with handedness
    let handedness = in.vert_tangent.w;
    out.world_tangent = normalize(T - dot(T, N) * N);
    out.world_bitangent = normalize(cross(N, out.world_tangent) * handedness);

    out.instance_idx = instance_idx;

    return out;
}

// === PBR Helper Functions ===

const PI: f32 = 3.14159265359;

// Fresnel-Schlick approximation
fn fresnel_schlick(cos_theta: f32, F0: vec3f) -> vec3f {
    return F0 + (1.0 - F0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

// GGX/Trowbridge-Reitz normal distribution function
fn distribution_ggx(N: vec3f, H: vec3f, roughness_sq: f32) -> f32 {
    let a2 = roughness_sq;
    let NdotH = max(dot(N, H), 0.0);
    let NdotH2 = NdotH * NdotH;

    let num = a2;
    var denom = (NdotH2 * (a2 - 1.0) + 1.0);
    denom = PI * denom * denom;

    return num / denom;
}

// Schlick-GGX geometry function
fn geometry_schlick_ggx(NdotV: f32, roughness_sq: f32) -> f32 {
    let r = roughness_sq + 1.0;
    let k = (r * r) / 8.0;

    let num = NdotV;
    let denom = NdotV * (1.0 - k) + k;

    return num / denom;
}

// Smith's geometry function
fn geometry_smith(N: vec3f, V: vec3f, L: vec3f, roughness_sq: f32) -> f32 {
    let NdotV = max(dot(N, V), 0.0);
    let NdotL = max(dot(N, L), 0.0);
    let ggx1 = geometry_schlick_ggx(NdotV, roughness_sq);
    let ggx2 = geometry_schlick_ggx(NdotL, roughness_sq);

    return ggx1 * ggx2;
}

// Sample texture by bindless index
fn sample_texture(idx: u32, coords: vec2f) -> vec4f {
    return textureSample(bindless_textures[idx], shared_sampler, coords);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let obj = objects[in.instance_idx];

    // Get texture indices
    let albedo_idx = obj.texture_indices.x;
    let normal_idx = obj.texture_indices.y;
    let mr_idx = obj.texture_indices.z;
    let ao_idx = obj.texture_indices.w;
    let emission_idx = u32(obj.material_params.w); // Stored as f32, cast to u32

    // Sample albedo texture
    let albedo_sample = sample_texture(albedo_idx, in.tex_coords);
    let albedo = albedo_sample.rgb * obj.base_color.rgb;
    let alpha = albedo_sample.a * obj.base_color.a;

    // Sample normal map and transform to world space
    let normal_sample = sample_texture(normal_idx, in.tex_coords);

    // Unpack normal from [0,1] to [-1,1] (use default scale of 1.0)
    let unpacked = normal_sample.xyz * 2.0 - 1.0;
    let tangent_normal = vec3f(unpacked.x, unpacked.y, unpacked.z);

    // Build TBN matrix and transform to world space
    let T = normalize(in.world_tangent);
    let B = normalize(in.world_bitangent);
    let N = normalize(in.world_normal);
    let TBN = mat3x3f(T, B, N);

    let final_normal = normalize(TBN * tangent_normal);

    // Sample metallic/roughness (GLTF packed: G=roughness, B=metallic)
    let mr_sample = sample_texture(mr_idx, in.tex_coords);
    // Clamp roughness to avoid division by zero in PBR calculations
    let roughness = max(mr_sample.g * obj.material_params.y, 0.04);  // G = roughness, minimum 0.04
    let metallic = mr_sample.b * obj.material_params.x;   // B = metallic

    // Sample ambient occlusion
    let ao_sample = sample_texture(ao_idx, in.tex_coords);
    let ao = ao_sample.r * obj.material_params.z;

    // View direction (from camera position)
    let V = normalize(frame_data.camera_position.xyz - in.world_pos);

    // Light direction (points TO the light)
    let L = normalize(frame_data.light_direction.xyz);
    let H = normalize(V + L);

    // Calculate reflectance at normal incidence (F0)
    // Dielectrics have F0 around 0.04, metals use albedo color
    let F0 = mix(vec3f(0.04), albedo, metallic);

    // Cook-Torrance BRDF
    let roughness_sq = roughness * roughness;
    let D = distribution_ggx(final_normal, H, roughness_sq);
    let G = geometry_smith(final_normal, V, L, roughness_sq);
    let F = fresnel_schlick(max(dot(H, V), 0.0), F0);

    let numerator = D * G * F;
    let NdotL = max(dot(final_normal, L), 0.0);
    let denominator = 4.0 * max(dot(final_normal, V), 0.0) * NdotL + 0.0001;
    let specular = numerator / denominator;

    // Energy conservation: kS + kD = 1
    // For metals, there is no diffuse reflection
    let kS = F;
    let kD = (1.0 - kS) * (1.0 - metallic);

    // Diffuse (Lambertian)
    let diffuse = kD * albedo / PI;

    // Combine diffuse and specular
    let radiance = frame_data.light_color.rgb * frame_data.light_intensity.x;
    let Lo = (diffuse + specular) * radiance * NdotL;

    // Ambient (simple constant ambient term with AO)
    let ambient = vec3f(0.03) * albedo * ao;

    // Emission - self-illuminated areas (only if emission_idx > 0)
    var emission = vec3f(0.0);
    if (emission_idx > 0u) {
        let emission_sample = sample_texture(emission_idx, in.tex_coords);
        emission = emission_sample.rgb;
    }

    // Final color - HDR LINEAR OUTPUT (no tonemapping)
    let color = ambient + Lo + emission;

    return vec4f(color, alpha);
}
