//! Grid material for editor grid rendering.
//!
//! Pure configuration for a fullscreen triangle pipeline that renders an infinite grid
//! on the XZ plane at Y=0. Uses Ben Golus's "Best Darn Grid Shader" algorithm
//! for anti-aliased, perspective-correct grid lines.

use katla_vulkan::{DescriptorSetLayoutBuilder, DescriptorType, ShaderStages, VertexBinding};
use std::path::PathBuf;

/// Grid material that renders an infinite editor grid.
///
/// This is a pure configuration struct. Pipelines are created by the renderer
/// using `MaterialPipelineCache::get_or_create()`.
#[derive(katla_derive::Material)]
#[material(shader = "resources/shaders/grid.wgsl")]
#[material(domain = "PostProcess")]
#[material(depth_test = true, depth_write = false, cull_backfaces = false, alpha_blending = true)]
pub struct GridMaterial {
    pub vertex_binding: VertexBinding,
    pub shader_path: PathBuf,
    pub descriptor_layouts: Vec<DescriptorSetLayoutBuilder>,
}

impl Default for GridMaterial {
    fn default() -> Self {
        Self {
            vertex_binding: VertexBinding { formats: vec![] },
            shader_path: PathBuf::from("resources/shaders/grid.wgsl"),
            descriptor_layouts: vec![
                DescriptorSetLayoutBuilder::new()
                    .add_binding(0, DescriptorType::StorageBuffer, ShaderStages::VERTEX_FRAGMENT)
                    .add_binding(1, DescriptorType::StorageBuffer, ShaderStages::VERTEX_FRAGMENT),
            ],
        }
    }
}
