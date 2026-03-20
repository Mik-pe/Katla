// Shared frame-level uniforms (must match FrameUniforms in Rust).

struct FrameUniforms {
    view: mat4x4f,
    proj: mat4x4f,
    inv_view_proj: mat4x4f,
    camera_position: vec4f,
    light_direction: vec4f,
    light_color: vec4f,
    light_intensity: vec4f,
    tiles: vec4<u32>,
}

struct ObjectUniforms {
    model: mat4x4f,
    base_color: vec4f,
    material_params: vec4f,     // x=metallic, y=roughness, z=ao, w=emission_idx
    texture_indices: vec4<u32>, // x=albedo, y=normal, z=mr, w=ao (bindless indices)
}
