//! UI material for immediate mode overlay rendering.
//!
//! Creates a pipeline for rendering UI elements with alpha blending.
//! Vertices use screen coordinates (pixels) and the shader transforms to NDC
//! using a uniform buffer containing the screen size.

use katla_vulkan::{
    material::MaterialPipeline,
    DescriptorSetLayoutBuilder, DescriptorType,
    MaterialPipelineCache, ShaderStages, VertexBinding, VertexFormat,
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
#[derive(katla_derive::Material)]
#[material(shader = "resources/shaders/ui/ui.wgsl")]
#[material(domain = "Ui")]
#[material(depth_test = false, depth_write = false, cull_backfaces = false, alpha_blending = true)]
#[material(color_format = "B8G8R8A8Srgb")]
pub struct UiMaterial {
    pub vertex_binding: VertexBinding,
    pub shader_path: PathBuf,
    pub descriptor_layouts: Vec<DescriptorSetLayoutBuilder>,
    #[material(skip)]
    pub pipeline: Option<Rc<RefCell<MaterialPipeline>>>,
}

impl UiMaterial {
    /// Create a new UI material and its pipeline.
    pub fn new(cache: &mut MaterialPipelineCache) -> Self {
        let mut material = Self {
            vertex_binding: VertexBinding {
                formats: vec![VertexFormat::RG32f, VertexFormat::RG32f, VertexFormat::RGBA32f],
            },
            shader_path: PathBuf::from("resources/shaders/ui/ui.wgsl"),
            descriptor_layouts: vec![
                // Set 0: Static UI resources (font atlas, sampler, uniforms)
                DescriptorSetLayoutBuilder::new()
                    .add_binding(0, DescriptorType::SampledImage, ShaderStages::FRAGMENT)
                    .add_binding(1, DescriptorType::Sampler, ShaderStages::FRAGMENT)
                    .add_binding(3, DescriptorType::UniformBuffer, ShaderStages::VERTEX),
                // Set 1: Dynamic texture via push descriptors
                DescriptorSetLayoutBuilder::new()
                    .add_binding(0, DescriptorType::SampledImage, ShaderStages::FRAGMENT)
                    .with_push_descriptor(true),
            ],
            pipeline: None,
        };

        let pipeline = cache
            .get_or_create(&material)
            .expect("Failed to create UI pipeline");

        material.pipeline = Some(pipeline);
        material
    }

    /// Get the pipeline.
    pub fn pipeline(&self) -> Rc<RefCell<MaterialPipeline>> {
        self.pipeline.as_ref().expect("Pipeline not initialized").clone()
    }
}
