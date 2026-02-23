//! Sky material for procedural sky rendering.
//!
//! Creates a fullscreen triangle pipeline that renders a procedural sky
//! with atmospheric scattering and sun disk. The sky always renders behind
//! all geometry (depth write disabled, depth compare = always).

use katla_vulkan::{
    material::MaterialPipeline,
    DescriptorSetLayoutBuilder, DescriptorType,
    MaterialPipelineCache, ShaderStages, VertexBinding,
};
use std::{cell::RefCell, path::PathBuf, rc::Rc};

/// Sky material that renders a procedural sky background.
///
/// Uses a fullscreen triangle (no vertex input) with camera-relative
/// sky gradient using inverse view-projection matrix.
#[derive(katla_derive::Material)]
#[material(shader = "resources/shaders/sky.wgsl")]
#[material(domain = "PostProcess")]
#[material(depth_test = true, depth_write = false, cull_backfaces = false)]
pub struct SkyMaterial {
    pub vertex_binding: VertexBinding,
    pub shader_path: PathBuf,
    pub descriptor_layouts: Vec<DescriptorSetLayoutBuilder>,
    #[material(skip)]
    pub pipeline: Option<Rc<RefCell<MaterialPipeline>>>,
}

impl SkyMaterial {
    /// Create a sky material using the pipeline cache.
    pub fn new(cache: &mut MaterialPipelineCache) -> Self {
        let mut material = Self {
            vertex_binding: VertexBinding { formats: vec![] },
            shader_path: PathBuf::from("resources/shaders/sky.wgsl"),
            descriptor_layouts: vec![
                DescriptorSetLayoutBuilder::new()
                    .add_binding(0, DescriptorType::StorageBuffer, ShaderStages::VERTEX_FRAGMENT)
                    .add_binding(1, DescriptorType::StorageBuffer, ShaderStages::VERTEX_FRAGMENT),
            ],
            pipeline: None,
        };

        let pipeline = cache.get_or_create(&material).expect("Failed to create sky pipeline");
        material.pipeline = Some(pipeline);
        material
    }

    /// Get the pipeline.
    pub fn pipeline(&self) -> Rc<RefCell<MaterialPipeline>> {
        self.pipeline.as_ref().expect("Pipeline not initialized").clone()
    }
}
