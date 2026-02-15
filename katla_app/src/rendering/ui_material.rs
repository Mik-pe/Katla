//! UI material for immediate mode overlay rendering.
//!
//! Creates a pipeline for rendering UI elements with alpha blending.

use katla_vulkan::{
    context::VulkanContext, material::MaterialPipeline, ImageFormat, MaterialBuilder,
    VertexBinding, VertexFormat,
};
use std::{cell::RefCell, path::Path, rc::Rc};

/// UI material that renders immediate mode UI overlays.
///
/// Uses alpha blending and no depth testing for proper overlay rendering.
pub struct UiMaterial {
    pub pipeline: Rc<RefCell<MaterialPipeline>>,
}

impl UiMaterial {
    /// Create a new UI material with the given Vulkan context.
    ///
    /// The UI pipeline is configured with:
    /// - Alpha blending enabled
    /// - No depth test or write
    /// - No backface culling
    /// - Vertex format: position[2], uv[2], color[4]
    pub fn new(context: Rc<VulkanContext>) -> Self {
        // UI vertex format: position (vec2), uv (vec2), color (vec4)
        let vertex_binding = VertexBinding {
            formats: vec![
                VertexFormat::RG32f,   // position
                VertexFormat::RG32f,   // uv
                VertexFormat::RGBA32f, // color
            ],
        };

        let pipeline = MaterialBuilder::new(context)
            .with_vertex_binding(vertex_binding)
            .with_wgsl_shader(Path::new("resources/shaders/ui/ui.wgsl"))
            .with_alpha_blending(true)
            .with_depth_test(false)
            .with_depth_write(false)
            .with_backface_culling(false)
            .with_color_format(ImageFormat::B8G8R8A8Srgb)
            .with_depth_format(ImageFormat::D32SfloatS8Uint)
            .build()
            .expect("Failed to create UI pipeline");

        Self {
            pipeline: Rc::new(RefCell::new(pipeline)),
        }
    }

    /// Get the pipeline as a cloned Rc<RefCell<>>.
    pub fn pipeline(&self) -> Rc<RefCell<MaterialPipeline>> {
        Rc::clone(&self.pipeline)
    }
}
