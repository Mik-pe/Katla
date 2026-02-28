use std::path::PathBuf;

use katla_math::Color;
use katla_vulkan::{BindlessPbrMaterialConfig, TextureDescriptor, VulkanRenderer};

use crate::rendering::{Material, VertexPBR};

/// Shader path for PBR materials
const PBR_SHADER_PATH: &str = "resources/shaders/model_pbr.wgsl";

/// Generate checkerboard pixel data.
fn generate_checkerboard_pixels(texture_size: u32, checker_size: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((texture_size * texture_size * 4) as usize);

    for y in 0..texture_size {
        for x in 0..texture_size {
            let checker_x = x / checker_size;
            let checker_y = y / checker_size;
            let is_white = (checker_x + checker_y).is_multiple_of(2);

            let pixel = if is_white {
                Color::WHITE.to_bytes()
            } else {
                Color::BLACK.to_bytes()
            };
            pixels.extend_from_slice(&pixel);
        }
    }
    pixels
}

/// Create a checkerboard material.
///
/// This function creates a material with a procedurally generated checkerboard texture.
/// The texture is registered with the bindless manager.
pub fn create_checkerboard_material(renderer: &mut VulkanRenderer) -> Material {
    create_checkerboard_material_with_color(renderer, None)
}

/// Create a colored checkerboard material.
///
/// This function creates a material with a procedurally generated checkerboard texture
/// that is blended with a material color.
pub fn create_colored_checkerboard_material(
    renderer: &mut VulkanRenderer,
    color: Color,
) -> Material {
    create_checkerboard_material_with_color(renderer, Some(color))
}

/// Internal helper to create checkerboard material with optional color.
fn create_checkerboard_material_with_color(
    renderer: &mut VulkanRenderer,
    color: Option<Color>,
) -> Material {
    use katla_vulkan::{
        DEFAULT_ALBEDO_SLOT, DEFAULT_AO_SLOT, DEFAULT_EMISSION_SLOT, DEFAULT_MR_SLOT,
        DEFAULT_NORMAL_SLOT,
    };

    // Generate checkerboard pixels
    let pixels = generate_checkerboard_pixels(64, 8);

    // Create texture using TextureManager
    let tm = renderer.texture_manager_mut();
    let desc = TextureDescriptor::rgba8_srgb(64, 64);
    let albedo_handle = tm.create(&desc, &pixels);

    // Get view and register with bindless manager
    let albedo_idx = if let Some(view) = renderer.texture_manager().get_view(albedo_handle) {
        renderer
            .bindless_manager_mut()
            .register_texture(view)
            .unwrap_or(DEFAULT_ALBEDO_SLOT)
    } else {
        DEFAULT_ALBEDO_SLOT
    };

    // Track bindless slot in TextureManager
    renderer
        .texture_manager_mut()
        .register_bindless_slot(albedo_handle, albedo_idx);
    // Use default textures for other PBR slots
    let texture_indices = [
        albedo_idx,
        DEFAULT_NORMAL_SLOT,
        DEFAULT_MR_SLOT,
        DEFAULT_AO_SLOT,
    ];
    let emission_idx = DEFAULT_EMISSION_SLOT;

    // Create material config and get pipeline from cache
    let vertex_binding = VertexPBR::get_vertex_binding();

    let config =
        BindlessPbrMaterialConfig::new(vertex_binding.clone(), PathBuf::from(PBR_SHADER_PATH));

    let material_pipeline = renderer
        .create_bindless_material(&config)
        .expect("Failed to create bindless pipeline");

    let mut material = Material::from_pipeline_handle(material_pipeline, vertex_binding, true);

    if let Some(c) = color {
        material = material.with_base_color([c.r, c.g, c.b, c.a]);
    }

    material.with_bindless_indices(texture_indices, emission_idx)
}
