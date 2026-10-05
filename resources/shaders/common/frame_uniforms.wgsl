// Shared frame-level uniforms (must match FrameUniforms in Rust).

struct FrameUniforms {
    view: mat4x4f,
    proj: mat4x4f,
    inv_view_proj: mat4x4f,
    camera_position: vec4f,
    light_direction: vec4f,      // xyz = direction, w = unused
    light_color: vec4f,
    light_intensity: vec4f,      // x = intensity, y = depth_texture_bindless_idx
    tiles: vec4<u32>,
    // Post-processing params (frame-level, not per-object)
    tonemap: vec4f,              // x = exposure, y = gamma, z = mode, w = hdr_texture_index
    overlay: vec4f,              // x = ldr_texture_index, y = stencil_indicator_index
    compositing: vec4f,          // x = screen_width, y = screen_height, z = viewport_count, w = viewport_bindless_index
}

struct ObjectUniforms {
    model: mat4x4f,
    base_color: vec4f,
    material_params: vec4f,     // x=metallic, y=roughness, z=ao, w=emission_idx
    texture_indices: vec4<u32>, // x=albedo, y=normal, z=mr, w=ao (bindless indices)
}
