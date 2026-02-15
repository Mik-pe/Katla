//! UI material for immediate mode overlay rendering.
//!
//! Creates a pipeline for rendering UI elements with alpha blending.
//! Vertices should be pre-transformed to NDC space.

use katla_vulkan::{
    context::VulkanContext, material::MaterialPipeline, ImageFormat, MaterialBuilder,
    VertexBinding, VertexFormat,
};
use std::{cell::RefCell, path::Path, rc::Rc};

/// Shader-compatible UI vertex with tight packing.
///
/// This struct matches the shader's expected layout exactly:
/// - position: vec2f (8 bytes)
/// - uv: vec2f (8 bytes)
/// - color: vec4f (16 bytes)
///
/// Note: katla_math::Vec2 is 16 bytes (aligned), so we use [f32; 2] directly.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct UiShaderVertex {
    /// Position in NDC coordinates (-1 to 1).
    pub position: [f32; 2],
    /// Texture coordinates (0-1).
    pub uv: [f32; 2],
    /// Vertex color (RGBA, 0-1).
    pub color: [f32; 4],
}

impl UiShaderVertex {
    /// Create a new shader vertex from components.
    pub fn new(position: [f32; 2], uv: [f32; 2], color: [f32; 4]) -> Self {
        Self { position, uv, color }
    }
}

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
    /// - Vertex format: position[2] (NDC), uv[2], color[4]
    pub fn new(context: Rc<VulkanContext>) -> Self {
        // UI vertex format: position (vec2 in NDC), uv (vec2), color (vec4)
        let vertex_binding = VertexBinding {
            formats: vec![
                VertexFormat::RG32f,   // position (NDC coordinates)
                VertexFormat::RG32f,   // uv
                VertexFormat::RGBA32f, // color
            ],
        };

        let pipeline = MaterialBuilder::new(context)
            .with_vertex_binding(vertex_binding)
            .with_wgsl_shader(Path::new("resources/shaders/ui/ui.wgsl"))
            .with_ui_texture_layout()  // Use UI-style descriptor layout
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
