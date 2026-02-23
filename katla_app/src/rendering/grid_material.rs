//! Grid material for editor grid rendering.
//!
//! Creates a fullscreen triangle pipeline that renders an infinite grid
//! on the XZ plane at Y=0. Uses Ben Golus's "Best Darn Grid Shader" algorithm
//! for anti-aliased, perspective-correct grid lines.

use katla_vulkan::{
    context::VulkanContext,
    material::{MaterialPipeline, RenderState, ShaderSource},
    DescriptorSetLayoutBuilder, DescriptorType, ImageFormat,
    MaterialDomain, MaterialPipelineCache, ShaderStages, VertexBinding,
};
use std::{cell::RefCell, path::PathBuf, rc::Rc};

/// Grid material configuration (lightweight, implements Material for cache lookup).
#[derive(katla_derive::Material)]
#[material(domain = "PostProcess")]
#[material(depth_test = true, depth_write = false, cull_backfaces = false, alpha_blending = true)]
struct GridMaterialConfig {
    vertex_binding: VertexBinding,
    shader_path: PathBuf,
    descriptor_layouts: Vec<DescriptorSetLayoutBuilder>,
}

/// Grid material that renders an infinite editor grid.
pub struct GridMaterial {
    pub pipeline: Rc<RefCell<MaterialPipeline>>,
    pub vertex_binding: VertexBinding,
    shader_path: PathBuf,
    descriptor_layouts: Vec<DescriptorSetLayoutBuilder>,
}

impl GridMaterial {
    /// Create a grid material using the pipeline cache.
    pub fn new_cached(_context: Rc<VulkanContext>, cache: &mut MaterialPipelineCache) -> Self {
        let vertex_binding = VertexBinding { formats: vec![] };
        let shader_path = PathBuf::from("resources/shaders/grid.wgsl");

        let descriptor_layouts = vec![
            DescriptorSetLayoutBuilder::new()
                .add_binding(0, DescriptorType::StorageBuffer, ShaderStages::VERTEX_FRAGMENT)
                .add_binding(1, DescriptorType::StorageBuffer, ShaderStages::VERTEX_FRAGMENT),
        ];

        let config = GridMaterialConfig {
            vertex_binding: vertex_binding.clone(),
            shader_path: shader_path.clone(),
            descriptor_layouts: descriptor_layouts.clone(),
        };

        let pipeline = cache.get_or_create(&config).expect("Failed to create grid pipeline");

        Self { pipeline, vertex_binding, shader_path, descriptor_layouts }
    }

    pub fn pipeline(&self) -> Rc<RefCell<MaterialPipeline>> {
        Rc::clone(&self.pipeline)
    }
}

impl katla_vulkan::Material for GridMaterial {
    fn vertex_shader(&self) -> ShaderSource { ShaderSource::WgslFile(self.shader_path.clone()) }
    fn fragment_shader(&self) -> ShaderSource { ShaderSource::WgslFile(self.shader_path.clone()) }
    fn vertex_binding(&self) -> VertexBinding { self.vertex_binding.clone() }
    fn render_state(&self) -> RenderState {
        RenderState { depth_test: true, depth_write: false, cull_backfaces: false, alpha_blending: true }
    }
    fn descriptor_layouts(&self) -> Vec<DescriptorSetLayoutBuilder> { self.descriptor_layouts.clone() }
    fn color_format(&self) -> ImageFormat { ImageFormat::R16G16B16A16Sfloat }
    fn depth_format(&self) -> ImageFormat { ImageFormat::D32SfloatS8Uint }
    fn domain(&self) -> MaterialDomain { MaterialDomain::PostProcess }
}
