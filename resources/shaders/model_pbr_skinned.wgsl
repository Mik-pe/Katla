// Skinned PBR shader with BINDLESS TEXTURES, GPU skeletal animation, and Forward+ dynamic lighting.
//
// Uses storage buffers for uniform data with instance_index for per-object selection.
// Four descriptor sets: uniforms (set 0), bindless textures (set 1), skeleton (set 2), lights (set 3).
//
// Bindless architecture:
// - Set 1, Binding 0: texture_2d array (4096 textures)
// - Set 1, Binding 1: shared sampler
// - Texture indices come from per-object ObjectUniforms
//
// Forward+ lighting:
// - Set 3, Binding 0: point_lights (storage buffer, read)
// - Set 3, Binding 1: tile_light_indices (storage buffer, read)
// - Set 3, Binding 2: tile_light_counts (storage buffer, read)
//
// Implements:
// - GPU skeletal animation with up to 4 joint influences per vertex
// - Metallic/Roughness workflow with texture support
// - Tangent-space normal mapping
// - Fresnel-Schlick approximation
// - GGX distribution for specular
// - Geometry/visibility function (Smith)
// - Directional lighting (sun) from frame_data
// - Dynamic point lights via Forward+ tile culling
// - HDR linear output (NO tonemapping - handled by post-process pass)

const TILE_SIZE: u32 = 16u;
const MAX_LIGHTS_PER_TILE: u32 = 128u;
const MAX_POINT_LIGHTS: u32 = 256u;

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

// Point light data (must match PointLightGPU in Rust)
struct PointLightGPU {
    position: vec3f,
    range: f32,
    color: vec3f,
    intensity: f32,
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

// Set 2: Skeleton joint matrices
// Each mesh with skeletal animation gets its own joint matrix buffer
@group(2) @binding(0)
var<storage, read> joint_matrices: array<mat4x4f>;

// Set 3: Forward+ light culling data
@group(3) @binding(0)
var<storage, read> point_lights: array<PointLightGPU, MAX_POINT_LIGHTS>;

@group(3) @binding(1)
var<storage, read> tile_light_indices: array<u32>;

@group(3) @binding(2)
var<storage, read> tile_light_counts: array<u32>;

// Maximum joints per skeleton (match with CPU-side constant)
const MAX_JOINTS: u32 = 256u;

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
    @location(2) vert_tangent: vec4f,  // w component = handedness
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
    @location(3) world_tangent: vec3f,
    @location(4) world_bitangent: vec3f,
    @location(5) @interpolate(flat) instance_idx: u32,
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

    // Apply skinning to normal and tangent (only rotation/scale, no translation)
    // Extract 3x3 from skin matrix for normal/tangent transformation
    let skin_matrix_3x3 = mat3x3f(
        skin_matrix[0].xyz,
        skin_matrix[1].xyz,
        skin_matrix[2].xyz,
    );

    // Also apply model matrix to get world-space vectors
    let normal_matrix = mat3x3f(
        obj.model[0].xyz,
        obj.model[1].xyz,
        obj.model[2].xyz,
    );

    // Apply skin matrix then model matrix to normal and tangent
    let skinned_normal = skin_matrix_3x3 * in.normal;
    let skinned_tangent = skin_matrix_3x3 * in.vert_tangent.xyz;

    let N = normalize(normal_matrix * skinned_normal);
    out.world_normal = N;

    // Gram-Schmidt reorthogonalization for proper tangent frame
    let T = normalize(normal_matrix * skinned_tangent);
    out.world_tangent = normalize(T - dot(T, N) * N);

    // Calculate bitangent using tangent and normal with handedness
    let handedness = in.vert_tangent.w;
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

    let roughness_sq = roughness * roughness;

    // === Directional light (sun) ===
    let L_sun = normalize(frame_data.light_direction.xyz);
    let H_sun = normalize(V + L_sun);
    let D_sun = distribution_ggx(final_normal, H_sun, roughness_sq);
    let G_sun = geometry_smith(final_normal, V, L_sun, roughness_sq);
    let F_sun = fresnel_schlick(max(dot(H_sun, V), 0.0), F0);
    let numerator_sun = D_sun * G_sun * F_sun;
    let NdotL_sun = max(dot(final_normal, L_sun), 0.0);
    let denominator_sun = 4.0 * max(dot(final_normal, V), 0.0) * NdotL_sun + 0.0001;
    let specular_sun = numerator_sun / denominator_sun;
    let radiance_sun = frame_data.light_color.rgb * frame_data.light_intensity.x;

    // === PBR lighting accumulation ===
    let kS = F_sun;
    let kD = (1.0 - kS) * (1.0 - metallic);
    let diffuse = kD * albedo / PI;

    let Lo_sun = (diffuse + specular_sun) * radiance_sun * NdotL_sun;

    // === Point lights (Forward+ tile culling) ===
    var Lo_point = vec3f(0.0);

    let tiles_x = u32(frame_data.light_intensity.y);
    let tiles_y = u32(frame_data.light_intensity.z);
    // Clamp to avoid negative values at screen edges (clip_position can be < 0.5 at first pixel)
    let pixel_x = max(u32(in.clip_position.x), 0u);
    let pixel_y = max(u32(in.clip_position.y), 0u);
    let tile = vec2<u32>(
        pixel_x / TILE_SIZE,
        pixel_y / TILE_SIZE
    );
    let tile_idx = tile.y * tiles_x + tile.x;

    if (tile.x < tiles_x && tile.y < tiles_y && tile_idx < arrayLength(&tile_light_counts)) {
        let light_count = tile_light_counts[tile_idx];

        let base_offset = tile_idx * MAX_LIGHTS_PER_TILE;
        for (var i = 0u; i < light_count; i++) {
            let light_idx = tile_light_indices[base_offset + i];
            if (light_idx >= MAX_POINT_LIGHTS) {
                break;
            }

            let light = point_lights[light_idx];
            let to_light = light.position - in.world_pos;
            let dist = length(to_light);
            let L_pt = to_light / max(dist, 0.001);

            if (dist > light.range) {
                continue;
            }
            let attenuation = 1.0 - (dist / light.range);
            let atten = attenuation * attenuation;

            let H_pt = normalize(V + L_pt);
            let D_pt = distribution_ggx(final_normal, H_pt, roughness_sq);
            let G_pt = geometry_smith(final_normal, V, L_pt, roughness_sq);
            let F_pt = fresnel_schlick(max(dot(H_pt, V), 0.0), F0);
            let numerator_pt = D_pt * G_pt * F_pt;
            let NdotL_pt = max(dot(final_normal, L_pt), 0.0);
            let denominator_pt = 4.0 * max(dot(final_normal, V), 0.0) * NdotL_pt + 0.0001;
            let specular_pt = numerator_pt / denominator_pt;

            let radiance_pt = light.color * light.intensity * atten;
            Lo_point += (diffuse + specular_pt) * radiance_pt * NdotL_pt;
        }
    }

    let Lo = Lo_sun + Lo_point;

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
