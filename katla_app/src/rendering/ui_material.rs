//! UI material for immediate mode overlay rendering.
//!
//! Creates a pipeline for rendering UI elements with alpha blending.
//! Vertices use screen coordinates (pixels) and the shader transforms to NDC
//! using a uniform buffer containing the screen size.

use katla_vulkan::{
    material::{MaterialPipeline, RenderState, ShaderSource},
    DescriptorSetLayoutBuilder, DescriptorType, ImageFormat,
    MaterialDomain, MaterialPipelineCache, ShaderStages, VertexBinding, VertexFormat,
};
use std::{cell::RefCell, path::PathBuf, rc::Rc};

/// Shader-compatible UI vertex with tight packing.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct UiShaderVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

impl UiShaderVertex {
    pub fn new(position: [f32; 2], uv: [f32; 2], color: [f32; 4]) -> Self {
        Self { position, uv, color }
    }
}

/// UI material that renders immediate mode UI overlays.
///
/// Uses two descriptor sets:
/// - Set 0: Static resources (font atlas, sampler, uniforms)
/// - Set 1: Dynamic texture via push descriptors
pub struct UiMaterial {
    pub pipeline: Rc<RefCell<MaterialPipeline>>,
}

impl UiMaterial {
    /// Create a new UI material and its pipeline.
    pub fn new(cache: &mut MaterialPipelineCache) -> Self {
        let config = UiMaterialConfig;
        let pipeline = cache
            .get_or_create(&config)
            .expect("Failed to create UI pipeline");

        Self { pipeline }
    }
}

/// Config struct for pipeline cache lookup (internal).
struct UiMaterialConfig;

impl katla_vulkan::Material for UiMaterialConfig {
    fn vertex_shader(&self) -> ShaderSource {
        ShaderSource::WgslFile(PathBuf::from("resources/shaders/ui/ui.wgsl"))
    }

    fn fragment_shader(&self) -> ShaderSource {
        ShaderSource::WgslFile(PathBuf::from("resources/shaders/ui/ui.wgsl"))
    }

    fn vertex_binding(&self) -> VertexBinding {
        VertexBinding {
            formats: vec![VertexFormat::RG32f, VertexFormat::RG32f, VertexFormat::RGBA32f],
        }
    }

    fn render_state(&self) -> RenderState {
        RenderState {
            depth_test: false,
            depth_write: false,
            cull_backfaces: false,
            alpha_blending: true,
        }
    }

    fn descriptor_layouts(&self) -> Vec<DescriptorSetLayoutBuilder> {
        vec![
            // Set 0: Static UI resources (font atlas, sampler, uniforms)
            DescriptorSetLayoutBuilder::new()
                .add_binding(0, DescriptorType::SampledImage, ShaderStages::FRAGMENT)
                .add_binding(1, DescriptorType::Sampler, ShaderStages::FRAGMENT)
                .add_binding(3, DescriptorType::UniformBuffer, ShaderStages::VERTEX),
            // Set 1: Dynamic texture via push descriptors (for viewport/thumbnails)
            DescriptorSetLayoutBuilder::new()
                .add_binding(0, DescriptorType::SampledImage, ShaderStages::FRAGMENT)
                .with_push_descriptor(true),
        ]
    }

    fn domain(&self) -> MaterialDomain {
        MaterialDomain::Ui
    }

    fn color_format(&self) -> ImageFormat {
        ImageFormat::B8G8R8A8Srgb
    }

    fn depth_format(&self) -> ImageFormat {
        ImageFormat::D32SfloatS8Uint
    }
}
