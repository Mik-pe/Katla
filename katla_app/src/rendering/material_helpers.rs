use std::{path::PathBuf, rc::Rc};

use katla_math::Color;
use katla_vulkan::{
    DescriptorSetLayoutBuilder, DescriptorType, ImageFormat, ShaderStages, Texture, VertexBinding,
    VulkanContext, VulkanRenderer,
};
use log::warn;

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

/// Material configuration for bindless PBR materials.
#[derive(katla_derive::Material)]
#[material(shader = "resources/shaders/model_pbr.wgsl", domain = "Surface")]
#[material(depth_test = true, depth_write = true, cull_backfaces = true)]
#[material(uses_bindless = true)]
struct BindlessPbrMaterialConfig {
    vertex_binding: VertexBinding,
    shader_path: PathBuf,
    descriptor_layouts: Vec<DescriptorSetLayoutBuilder>,
}

/// Create a checkerboard material.
///
/// This function creates a material with a procedurally generated checkerboard texture.
/// The texture is registered with the bindless manager.
pub fn create_checkerboard_material(
    context: Rc<VulkanContext>,
    renderer: &mut VulkanRenderer,
) -> Material {
    create_checkerboard_material_with_color(context, renderer, None)
}

/// Create a colored checkerboard material.
///
/// This function creates a material with a procedurally generated checkerboard texture
/// that is blended with a material color.
pub fn create_colored_checkerboard_material(
    context: Rc<VulkanContext>,
    renderer: &mut VulkanRenderer,
    color: Color,
) -> Material {
    create_checkerboard_material_with_color(context, renderer, Some(color))
}

/// Internal helper to create checkerboard material with optional color.
fn create_checkerboard_material_with_color(
    context: Rc<VulkanContext>,
    renderer: &mut VulkanRenderer,
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
    let bindless_manager = renderer
        .bindless_manager
        .as_mut()
        .expect("Bindless manager not initialized");
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

    // Create material config and get pipeline from cache
    let vertex_binding = VertexPBR::get_vertex_binding();
    let bindless_layout = bindless_manager.vk_descriptor_layout();

    let config = BindlessPbrMaterialConfig {
        vertex_binding: vertex_binding.clone(),
        shader_path: PathBuf::from(PBR_SHADER_PATH),
        descriptor_layouts: vec![DescriptorSetLayoutBuilder::new()
            .add_binding(
                0,
                DescriptorType::StorageBuffer,
                ShaderStages::VERTEX_FRAGMENT,
            )
            .add_binding(
                1,
                DescriptorType::StorageBuffer,
                ShaderStages::VERTEX_FRAGMENT,
            )],
    };

    let material_pipeline = renderer
        .material_cache
        .borrow_mut()
        .get_or_create_bindless(&config, bindless_layout)
        .expect("Failed to create bindless pipeline");

    Material::from_pipeline_handle(
        material_pipeline,
        vertex_binding,
        true, // is_bindless
    )
    .with_bindless_indices(texture_indices, emission_idx)
}
