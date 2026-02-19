// Skinned PBR shader with GPU skeletal animation.
//
// Uses storage buffers for uniform data with instance_index for per-object selection.
// Three descriptor sets: uniforms (set 0), textures (set 1), skeleton (set 2).
//
// Implements:
// - GPU skeletal animation with up to 4 joint influences per vertex
// - Metallic/Roughness workflow
// - Fresnel-Schlick approximation
// - GGX distribution for specular
// - Geometry/visibility function (Smith)
// - Directional lighting with camera position

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

// Per-object uniforms
struct ObjectUniforms {
    model: mat4x4f,
    base_color: vec4f,
    material_params: vec4f,      // x=metallic, y=roughness, z=ao, w=emission_idx
    texture_indices: vec4<u32>,  // bindless indices (unused in legacy mode)
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

// Set 2: Skeleton joint matrices
// Each mesh with skeletal animation gets its own joint matrix buffer
@group(2) @binding(0)
var<storage, read> joint_matrices: array<mat4x4f>;

// Maximum joints per skeleton (match with CPU-side constant)
const MAX_JOINTS: u32 = 256u;

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
    @location(2) vert_tangent: vec4f,
    @location(3) vert_texcoord0: vec2f,
    // Skinning attributes
    // Note: Vertex format is RGBA16u (u16x4), GPU zero-extends each u16 to u32
    @location(4) joint_indices: vec4u,   // 4 joint indices (0-65535, zero-extended from u16)
    @location(5) joint_weights: vec4f,   // 4 joint weights (must sum to 1.0)
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) world_pos: vec3f,
    @location(1) tex_coords: vec2f,
    @location(2) world_normal: vec3f,
    @location(3) @interpolate(flat) instance_idx: u32,
}

/// Compute the skin matrix by blending joint matrices based on weights.
/// Each vertex can be influenced by up to 4 joints.
fn compute_skin_matrix(
    joint_indices: vec4u,
    joint_weights: vec4f,
) -> mat4x4f {
    // Blend joint matrices weighted by joint_weights
    // Note: joint_matrices already includes inverse bind matrix * joint transform
    let m0 = joint_matrices[joint_indices[0]] * joint_weights[0];
    let m1 = joint_matrices[joint_indices[1]] * joint_weights[1];
    let m2 = joint_matrices[joint_indices[2]] * joint_weights[2];
    let m3 = joint_matrices[joint_indices[3]] * joint_weights[3];

    return m0 + m1 + m2 + m3;
}

@vertex
fn vs_main(
    in: VertexInput,
    @builtin(instance_index) instance_idx: u32,
) -> VertexOutput {
    var out: VertexOutput;

    let obj = objects[instance_idx];

    // Compute skin matrix for this vertex
    let skin_matrix = compute_skin_matrix(in.joint_indices, in.joint_weights);

    // Apply skinning to position (in object space)
    let skinned_pos = skin_matrix * vec4f(in.position, 1.0);

    // Apply model matrix and view-projection
    let world_pos = obj.model * skinned_pos;
    out.world_pos = world_pos.xyz;
    out.clip_position = frame_data.proj * frame_data.view * world_pos;

    out.tex_coords = in.vert_texcoord0;

    // Apply skinning to normal (only rotation/scale, no translation)
    // Extract 3x3 from skin matrix for normal transformation
    let skin_matrix_3x3 = mat3x3f(
        skin_matrix[0].xyz,
        skin_matrix[1].xyz,
        skin_matrix[2].xyz,
    );

    // Also apply model matrix to get world-space normal
    let normal_matrix = mat3x3f(
        obj.model[0].xyz,
        obj.model[1].xyz,
        obj.model[2].xyz,
    );

    // Apply skin matrix then model matrix to normal
    let skinned_normal = skin_matrix_3x3 * in.normal;
    out.world_normal = normalize(normal_matrix * skinned_normal);

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
fn distribution_ggx(N: vec3f, H: vec3f, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let NdotH = max(dot(N, H), 0.0);
    let NdotH2 = NdotH * NdotH;

    let num = a2;
    var denom = (NdotH2 * (a2 - 1.0) + 1.0);
    denom = PI * denom * denom;

    return num / denom;
}

// Schlick-GGX geometry function
fn geometry_schlick_ggx(NdotV: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;

    let num = NdotV;
    let denom = NdotV * (1.0 - k) + k;

    return num / denom;
}

// Smith's geometry function
fn geometry_smith(N: vec3f, V: vec3f, L: vec3f, roughness: f32) -> f32 {
    let NdotV = max(dot(N, V), 0.0);
    let NdotL = max(dot(N, L), 0.0);
    let ggx1 = geometry_schlick_ggx(NdotV, roughness);
    let ggx2 = geometry_schlick_ggx(NdotL, roughness);

    return ggx1 * ggx2;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let obj = objects[in.instance_idx];

    // Sample albedo texture
    let albedo_sample = textureSample(albedo_texture, albedo_sampler, in.tex_coords);
    let albedo = albedo_sample.rgb * obj.base_color.rgb;
    let alpha = albedo_sample.a * obj.base_color.a;

    // Material properties
    let metallic = obj.material_params.x;
    let roughness = obj.material_params.y;
    let ao = obj.material_params.z;

    // Normal (from vertex interpolation)
    let N = normalize(in.world_normal);

    // View direction (from camera position)
    let V = normalize(frame_data.camera_position.xyz - in.world_pos);

    // Light direction (points TO the light)
    let L = normalize(frame_data.light_direction.xyz);
    let H = normalize(V + L);

    // Calculate reflectance at normal incidence (F0)
    // Dielectrics have F0 around 0.04, metals use albedo color
    let F0 = mix(vec3f(0.04), albedo, metallic);

    // Cook-Torrance BRDF
    let D = distribution_ggx(N, H, roughness);
    let G = geometry_smith(N, V, L, roughness);
    let F = fresnel_schlick(max(dot(H, V), 0.0), F0);

    let numerator = D * G * F;
    let NdotL = max(dot(N, L), 0.0);
    let denominator = 4.0 * max(dot(N, V), 0.0) * NdotL + 0.0001;
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

    // Ambient (simple constant ambient term)
    let ambient = vec3f(0.03) * albedo * ao;

    // Final color
    var color = ambient + Lo;

    // HDR tone mapping (Reinhard)
    color = color / (color + vec3f(1.0));

    // Gamma correction
    color = pow(color, vec3f(1.0 / 2.2));

    return vec4f(color, alpha);
}
