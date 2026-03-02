//! Debug line material for immediate-mode debug drawing.
//!
//! Pure configuration for a pipeline that renders debug lines
//! with unlit rendering and depth test ON, depth write OFF.

use katla_gfx::{
    DescriptorSetLayoutBuilder, DescriptorType, ShaderStages, VertexBinding, VertexFormat,
};
use std::path::PathBuf;

/// Debug line material that renders 3D debug primitives.
///
/// This is a pure configuration struct. Pipelines are created by the renderer
/// using `MaterialPipelineCache::get_or_create()`.
#[derive(katla_derive::Material)]
#[material(shader = "resources/shaders/debug_line.wgsl")]
#[material(domain = "Surface")]
#[material(depth_test = true, depth_write = false, cull_backfaces = false)]
pub struct DebugLineMaterial {
    pub vertex_binding: VertexBinding,
    pub shader_path: PathBuf,
    pub descriptor_layouts: Vec<DescriptorSetLayoutBuilder>,
}

impl Default for DebugLineMaterial {
    fn default() -> Self {
        Self {
            vertex_binding: VertexBinding {
                formats: vec![VertexFormat::RGB32f, VertexFormat::RGB32f],
            },
            shader_path: PathBuf::from("resources/shaders/debug_line.wgsl"),
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
