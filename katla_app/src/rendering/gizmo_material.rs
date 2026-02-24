//! Gizmo material for 3D transform manipulation.
//!
//! Pure configuration for a pipeline that renders gizmos (translate/rotate/scale handles)
//! with unlit rendering and always-on-top depth behavior.

use katla_vulkan::{
    DescriptorSetLayoutBuilder, DescriptorType, ShaderStages, VertexBinding, VertexFormat,
};
use std::path::PathBuf;

/// Gizmo material that renders 3D manipulation handles.
///
/// This is a pure configuration struct. Pipelines are created by the renderer
/// using `MaterialPipelineCache::get_or_create()`.
#[derive(katla_derive::Material)]
#[material(shader = "resources/shaders/gizmo.wgsl")]
#[material(domain = "Surface")]
#[material(depth_test = true, depth_write = false, cull_backfaces = false)]
pub struct GizmoMaterial {
    pub vertex_binding: VertexBinding,
    pub shader_path: PathBuf,
    pub descriptor_layouts: Vec<DescriptorSetLayoutBuilder>,
}

impl Default for GizmoMaterial {
    fn default() -> Self {
        Self {
            vertex_binding: VertexBinding {
                formats: vec![VertexFormat::RGB32f, VertexFormat::RGB32f],
            },
            shader_path: PathBuf::from("resources/shaders/gizmo.wgsl"),
            descriptor_layouts: vec![DescriptorSetLayoutBuilder::new()
                .add_binding(
                    0,
                    DescriptorType::StorageBuffer,
                    ShaderStages::VERTEX_FRAGMENT,
                )
                .add_binding(
                    1,
                    DescriptorType::StorageBuffer,
                    ShaderStages::VERTEX_FRAGMENT,
                )],
        }
    }
}
