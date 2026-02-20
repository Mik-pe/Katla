use std::{path::Path, rc::Rc};

use katla_math::Color;
use katla_vulkan::{BindlessTextureManager, ImageFormat, MaterialBuilder, Texture, VulkanContext};
use log::warn;

use crate::rendering::{Material, VertexPBR};

/// Shader path for PBR materials
const PBR_SHADER_PATH: &str = "resources/shaders/model_pbr_bindless.wgsl";

/// Generate checkerboard pixel data.
fn generate_checkerboard_pixels(texture_size: u32, checker_size: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((texture_size * texture_size * 4) as usize);

    for y in 0..texture_size {
        for x in 0..texture_size {
            let checker_x = x / checker_size;
            let checker_y = y / checker_size;
            let is_white = (checker_x + checker_y) % 2 == 0;

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
pub fn create_checkerboard_material(
    context: Rc<VulkanContext>,
    bindless_manager: &mut BindlessTextureManager,
) -> Material {
    create_checkerboard_material_with_color(context, bindless_manager, None)
}

/// Create a colored checkerboard material.
///
/// This function creates a material with a procedurally generated checkerboard texture
/// that is blended with a material color.
pub fn create_colored_checkerboard_material(
    context: Rc<VulkanContext>,
    bindless_manager: &mut BindlessTextureManager,
    color: Color,
) -> Material {
    create_checkerboard_material_with_color(context, bindless_manager, Some(color))
}

/// Internal helper to create checkerboard material with optional color.
fn create_checkerboard_material_with_color(
    context: Rc<VulkanContext>,
    bindless_manager: &mut BindlessTextureManager,
    color: Option<Color>,
) -> Material {
    // Generate checkerboard pixels
    let pixels = generate_checkerboard_pixels(64, 8);

    let texture = Rc::new(Texture::create_image(
        context.clone(),
        64,
        64,
        ImageFormat::R8G8B8A8Srgb,
        &pixels,
    ));

    // Register texture with bindless manager
    let albedo_idx = match bindless_manager.register_texture(texture.image_view) {
        Some(idx) => idx,
        None => {
            warn!("Failed to register checkerboard texture, using default");
            katla_vulkan::bindless_texture::DEFAULT_ALBEDO_SLOT
        }
    };

    // Use default textures for other PBR slots
    let texture_indices = [
        albedo_idx,
        katla_vulkan::bindless_texture::DEFAULT_NORMAL_SLOT,
        katla_vulkan::bindless_texture::DEFAULT_MR_SLOT,
        katla_vulkan::bindless_texture::DEFAULT_AO_SLOT,
    ];
    let emission_idx = katla_vulkan::bindless_texture::DEFAULT_EMISSION_SLOT;

    // Build material pipeline
    let vertex_binding = VertexPBR::get_vertex_binding();
    let bindless_layout = bindless_manager.vk_descriptor_layout();
    let material_pipeline = MaterialBuilder::new(context)
        .with_vertex_binding(vertex_binding.clone())
        .with_wgsl_shader(Path::new(PBR_SHADER_PATH))
        .with_depth_test(true)
        .with_depth_write(true)
        .with_backface_culling(true)
        .with_color_format(ImageFormat::R16G16B16A16Sfloat)
        .with_depth_format(ImageFormat::D32SfloatS8Uint)
        .build_bindless(bindless_layout)
        .expect("Failed to create material pipeline");

    Material::from_pipeline_with_textures(
        material_pipeline,
        Some(texture),
        vertex_binding,
        color,
        texture_indices,
        emission_idx,
    )
}
