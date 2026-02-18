use std::{path::Path, rc::Rc};

use katla_math::Color;
use katla_vulkan::{ImageFormat, MaterialBuilder, Texture, VulkanContext};

use crate::rendering::{Material, VertexPBR};

/// Create a checkerboard texture for use with materials.
///
/// This function creates a procedurally generated checkerboard texture
/// that can be used with template-based materials.
pub fn create_checkerboard_texture(context: Rc<VulkanContext>) -> Texture {
    // Create a checkerboard texture (64x64)
    let texture_size = 64;
    let checker_size = 8; // 8x8 pixel squares
    let mut pixels = Vec::with_capacity((texture_size * texture_size) as usize);

    for y in 0..texture_size {
        for x in 0..texture_size {
            // Determine which checker square we're in
            let checker_x = x / checker_size;
            let checker_y = y / checker_size;

            // Checkerboard pattern: alternate between two colors
            let is_white = (checker_x + checker_y) % 2 == 0;

            let pixel = if is_white {
                Color::WHITE.to_bytes()
            } else {
                Color::BLACK.to_bytes()
            };
            pixels.extend_from_slice(&pixel);
        }
    }

    Texture::create_image(
        context.clone(),
        texture_size,
        texture_size,
        ImageFormat::R8G8B8A8Srgb,
        &pixels,
    )
}

/// Create a checkerboard material for use with primitive shapes.
///
/// This function creates a material with a procedurally generated checkerboard texture.
/// The material can then be registered with a MaterialManager and shared across multiple models.
pub fn create_checkerboard_material(
    context: Rc<VulkanContext>,
) -> Material {
    // Create a checkerboard texture (64x64)
    let texture_size = 64;
    let checker_size = 8; // 8x8 pixel squares
    let mut pixels = Vec::with_capacity((texture_size * texture_size) as usize);

    for y in 0..texture_size {
        for x in 0..texture_size {
            // Determine which checker square we're in
            let checker_x = x / checker_size;
            let checker_y = y / checker_size;

            // Checkerboard pattern: alternate between two colors
            let is_white = (checker_x + checker_y) % 2 == 0;

            let pixel = if is_white {
                Color::WHITE.to_bytes()
            } else {
                Color::BLACK.to_bytes()
            };
            pixels.extend_from_slice(&pixel);
        }
    }

    let texture = Rc::new(Texture::create_image(
        context.clone(),
        texture_size,
        texture_size,
        ImageFormat::R8G8B8A8Srgb,
        &pixels,
    ));

    let vertex_binding = VertexPBR::get_vertex_binding();
    let wgsl_path = Path::new("resources/shaders/colored_mesh_storage.wgsl");
    let material_pipeline = MaterialBuilder::new(context.clone())
        .with_vertex_binding(vertex_binding.clone())
        .with_wgsl_shader(wgsl_path)
        .with_texture(texture.clone())
        .with_depth_test(true)
        .with_depth_write(true)
        .with_backface_culling(true)
        // Dynamic rendering: specify attachment formats
        .with_color_format(ImageFormat::R16G16B16A16Sfloat)
        .with_depth_format(ImageFormat::D32SfloatS8Uint)
        .build_with_storage()
        .expect("Failed to create material pipeline");

    Material::from_pipeline(material_pipeline, Some(texture), vertex_binding, None)
}

/// Create a colored checkerboard material for use with primitive shapes.
///
/// This function creates a material with a procedurally generated checkerboard texture
/// that is blended with a material color using the colored_mesh WGSL shader.
pub fn create_colored_checkerboard_material(
    context: Rc<VulkanContext>,
    color: Color,
) -> Material {
    // Create a checkerboard texture (64x64)
    let texture_size = 64;
    let checker_size = 8; // 8x8 pixel squares
    let mut pixels = Vec::with_capacity((texture_size * texture_size) as usize);

    for y in 0..texture_size {
        for x in 0..texture_size {
            // Determine which checker square we're in
            let checker_x = x / checker_size;
            let checker_y = y / checker_size;

            // Checkerboard pattern: alternate between two colors
            let is_white = (checker_x + checker_y) % 2 == 0;

            let pixel = if is_white {
                Color::WHITE.to_bytes()
            } else {
                Color::BLACK.to_bytes()
            };
            pixels.extend_from_slice(&pixel);
        }
    }

    let texture = Rc::new(Texture::create_image(
        context.clone(),
        texture_size,
        texture_size,
        ImageFormat::R8G8B8A8Srgb,
        &pixels,
    ));

    let vertex_binding = VertexPBR::get_vertex_binding();

    // Use WGSL shader that supports color blending
    let wgsl_path = std::path::Path::new("resources/shaders/colored_mesh_storage.wgsl");
    let material_pipeline = MaterialBuilder::new(context.clone())
        .with_vertex_binding(vertex_binding.clone())
        .with_wgsl_shader(wgsl_path)
        .with_texture(texture.clone())
        .with_depth_test(true)
        .with_depth_write(true)
        .with_backface_culling(true)
        // Dynamic rendering: specify attachment formats
        .with_color_format(ImageFormat::R16G16B16A16Sfloat)
        .with_depth_format(ImageFormat::D32SfloatS8Uint)
        .build_with_storage()
        .expect("Failed to create colored material pipeline");

    Material::from_pipeline(
        material_pipeline,
        Some(texture),
        vertex_binding,
        Some(color),
    )
}
