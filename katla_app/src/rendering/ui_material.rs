//! UI material for immediate mode overlay rendering.
//!
//! Pure configuration for a pipeline that renders UI elements with alpha blending.
//! Vertices use screen coordinates (pixels) and the shader transforms to NDC
//! using a uniform buffer containing the screen size.

use katla_gfx::{
    DescriptorSetLayoutBuilder, DescriptorType, ShaderStages, VertexBinding, VertexFormat,
};
use std::path::PathBuf;

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
        Self {
            position,
            uv,
            color,
        }
    }
}

/// UI material that renders immediate mode UI overlays.
///
/// Uses two descriptor sets:
/// - Set 0: Static resources (font atlas, sampler, uniforms)
/// - Set 1: Dynamic texture via push descriptors
///
/// This is a pure configuration struct. Pipelines are created by the renderer
/// using `MaterialPipelineCache::get_or_create()`.
#[derive(katla_derive::Material)]
#[material(shader = "resources/shaders/ui/ui.wgsl")]
#[material(domain = "Ui")]
#[material(
    depth_test = false,
    depth_write = false,
    cull_backfaces = false,
    alpha_blending = true
)]
#[material(color_format = "B8G8R8A8Srgb")]
pub struct UiMaterial {
    pub vertex_binding: VertexBinding,
    pub shader_path: PathBuf,
    pub descriptor_layouts: Vec<DescriptorSetLayoutBuilder>,
}

impl Default for UiMaterial {
    fn default() -> Self {
        Self {
            vertex_binding: VertexBinding {
                formats: vec![
                    VertexFormat::RG32f,
                    VertexFormat::RG32f,
                    VertexFormat::RGBA32f,
                ],
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
        }
    }
}
