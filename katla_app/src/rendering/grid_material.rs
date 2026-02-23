//! Grid material for editor grid rendering.
//!
//! Creates a fullscreen triangle pipeline that renders an infinite grid
//! on the XZ plane at Y=0. Uses Ben Golus's "Best Darn Grid Shader" algorithm
//! for anti-aliased, perspective-correct grid lines.

use katla_vulkan::{
    material::MaterialPipeline,
    DescriptorSetLayoutBuilder, DescriptorType,
    MaterialPipelineCache, ShaderStages, VertexBinding,
};
use std::{cell::RefCell, path::PathBuf, rc::Rc};

/// Grid material that renders an infinite editor grid.
#[derive(katla_derive::Material)]
#[material(shader = "resources/shaders/grid.wgsl")]
#[material(domain = "PostProcess")]
#[material(depth_test = true, depth_write = false, cull_backfaces = false, alpha_blending = true)]
pub struct GridMaterial {
    pub vertex_binding: VertexBinding,
    pub shader_path: PathBuf,
    pub descriptor_layouts: Vec<DescriptorSetLayoutBuilder>,
    #[material(skip)]
    pub pipeline: Option<Rc<RefCell<MaterialPipeline>>>,
}

impl GridMaterial {
    /// Create a grid material using the pipeline cache.
    pub fn new(cache: &mut MaterialPipelineCache) -> Self {
        let mut material = Self {
            vertex_binding: VertexBinding { formats: vec![] },
            shader_path: PathBuf::from("resources/shaders/grid.wgsl"),
            descriptor_layouts: vec![
                DescriptorSetLayoutBuilder::new()
                    .add_binding(0, DescriptorType::StorageBuffer, ShaderStages::VERTEX_FRAGMENT)
                    .add_binding(1, DescriptorType::StorageBuffer, ShaderStages::VERTEX_FRAGMENT),
            ],
            pipeline: None,
        };

        let pipeline = cache.get_or_create(&material).expect("Failed to create grid pipeline");
        material.pipeline = Some(pipeline);
        material
    }

    /// Get the pipeline.
    pub fn pipeline(&self) -> Rc<RefCell<MaterialPipeline>> {
        self.pipeline.as_ref().expect("Pipeline not initialized").clone()
    }
}
