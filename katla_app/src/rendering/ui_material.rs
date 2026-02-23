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

/// UI material configuration (lightweight, implements Material for cache lookup).
#[derive(katla_derive::Material)]
#[material(domain = "Ui")]
#[material(depth_test = false, depth_write = false, cull_backfaces = false, alpha_blending = true)]
#[material(color_format = "B8G8R8A8Srgb")]
struct UiMaterialConfig {
    vertex_binding: VertexBinding,
    shader_path: PathBuf,
    descriptor_layouts: Vec<DescriptorSetLayoutBuilder>,
}

/// UI material that renders immediate mode UI overlays.
pub struct UiMaterial {
    pub pipeline: Rc<RefCell<MaterialPipeline>>,
    vertex_binding: VertexBinding,
    shader_path: PathBuf,
    descriptor_layouts: Vec<DescriptorSetLayoutBuilder>,
}

impl UiMaterial {
    /// Create a new UI material using the unified Material API.
    ///
    /// Uses MaterialPipelineCache for pipeline creation and deduplication.
    pub fn new(cache: &mut MaterialPipelineCache) -> Self {
        let vertex_binding = VertexBinding {
            formats: vec![VertexFormat::RG32f, VertexFormat::RG32f, VertexFormat::RGBA32f],
        };
        let shader_path = PathBuf::from("resources/shaders/ui/ui.wgsl");

        // Set 0: Static UI resources (font atlas, sampler, uniforms)
        // Set 1: Dynamic texture via push descriptors (for viewport/thumbnails)
        let descriptor_layouts = vec![
            DescriptorSetLayoutBuilder::new()
                .add_binding(0, DescriptorType::SampledImage, ShaderStages::FRAGMENT)
                .add_binding(1, DescriptorType::Sampler, ShaderStages::FRAGMENT)
                .add_binding(3, DescriptorType::UniformBuffer, ShaderStages::VERTEX),
            DescriptorSetLayoutBuilder::new()
                .add_binding(0, DescriptorType::SampledImage, ShaderStages::FRAGMENT)
                .with_push_descriptor(true),
        ];

        // Use unified Material API with cache
        let config = UiMaterialConfig {
            vertex_binding: vertex_binding.clone(),
            shader_path: shader_path.clone(),
            descriptor_layouts: descriptor_layouts.clone(),
        };

        let pipeline = cache
            .get_or_create(&config)
            .expect("Failed to create UI pipeline");

        Self { pipeline, vertex_binding, shader_path, descriptor_layouts }
    }

    pub fn pipeline(&self) -> Rc<RefCell<MaterialPipeline>> {
        Rc::clone(&self.pipeline)
    }
}

impl katla_vulkan::Material for UiMaterial {
    fn vertex_shader(&self) -> ShaderSource { ShaderSource::WgslFile(self.shader_path.clone()) }
    fn fragment_shader(&self) -> ShaderSource { ShaderSource::WgslFile(self.shader_path.clone()) }
    fn vertex_binding(&self) -> VertexBinding { self.vertex_binding.clone() }
    fn render_state(&self) -> RenderState {
        RenderState { depth_test: false, depth_write: false, cull_backfaces: false, alpha_blending: true }
    }
    fn descriptor_layouts(&self) -> Vec<DescriptorSetLayoutBuilder> { self.descriptor_layouts.clone() }
    fn color_format(&self) -> ImageFormat { ImageFormat::B8G8R8A8Srgb }
    fn depth_format(&self) -> ImageFormat { ImageFormat::D32SfloatS8Uint }
    fn domain(&self) -> MaterialDomain { MaterialDomain::Ui }
}
