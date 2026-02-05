use std::rc::Rc;

use katla_vulkan::{MaterialBuilder, RenderPass, Texture, VulkanContext, ImageFormat};

use crate::rendering::{Material, ShaderRegistry, VertexPBR};

/// Create a checkerboard material for use with primitive shapes.
///
/// This function creates a material with a procedurally generated checkerboard texture.
/// The material can then be registered with a MaterialManager and shared across multiple models.
pub fn create_checkerboard_material(
    context: Rc<VulkanContext>,
    render_pass: &RenderPass,
    shader_registry: &ShaderRegistry,
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
                [255, 255, 255, 255] // White
            } else {
                [0, 0, 0, 255] // Black
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
    let material_pipeline = MaterialBuilder::new(context.clone())
        .with_vertex_binding(vertex_binding.clone())
        .with_vertex_shader(shader_registry.get_vertex_shader("model_pbr.vert"))
        .with_fragment_shader(shader_registry.get_fragment_shader("model.frag"))
        .with_texture(texture.clone())
        .with_depth_test(true)
        .with_depth_write(true)
        .with_backface_culling(true)
        .build(render_pass)
        .expect("Failed to create material pipeline");

    Material {
        material_pipeline: std::rc::Rc::new(std::cell::RefCell::new(material_pipeline)),
        texture: Some(texture),
        vertex_binding,
        handle: None,
    }
}
