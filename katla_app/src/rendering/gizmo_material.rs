//! Gizmo material for 3D transform manipulation.
//!
//! Creates a pipeline for rendering gizmos (translate/rotate/scale handles)
//! with unlit rendering and always-on-top depth behavior.

use katla_vulkan::{
    material::MaterialPipeline,
    DescriptorSetLayoutBuilder, DescriptorType,
    MaterialPipelineCache, ShaderStages, VertexBinding, VertexFormat,
};
use std::{cell::RefCell, path::PathBuf, rc::Rc};

/// Gizmo material that renders 3D manipulation handles.
#[derive(katla_derive::Material)]
#[material(shader = "resources/shaders/gizmo.wgsl")]
#[material(domain = "Surface")]
#[material(depth_test = true, depth_write = false, cull_backfaces = false)]
pub struct GizmoMaterial {
    pub vertex_binding: VertexBinding,
    pub shader_path: PathBuf,
    pub descriptor_layouts: Vec<DescriptorSetLayoutBuilder>,
    #[material(skip)]
    pub pipeline: Option<Rc<RefCell<MaterialPipeline>>>,
}

impl GizmoMaterial {
    /// Create a gizmo material using the pipeline cache.
    pub fn new(cache: &mut MaterialPipelineCache) -> Self {
        let mut material = Self {
            vertex_binding: VertexBinding {
                formats: vec![VertexFormat::RGB32f, VertexFormat::RGB32f],
            },
            shader_path: PathBuf::from("resources/shaders/gizmo.wgsl"),
            descriptor_layouts: vec![
                DescriptorSetLayoutBuilder::new()
                    .add_binding(0, DescriptorType::StorageBuffer, ShaderStages::VERTEX_FRAGMENT)
                    .add_binding(1, DescriptorType::StorageBuffer, ShaderStages::VERTEX_FRAGMENT),
            ],
            pipeline: None,
        };

        let pipeline = cache.get_or_create(&material).expect("Failed to create gizmo pipeline");
        material.pipeline = Some(pipeline);
        material
    }

    /// Get the pipeline.
    pub fn pipeline(&self) -> Rc<RefCell<MaterialPipeline>> {
        self.pipeline.as_ref().expect("Pipeline not initialized").clone()
    }
}
