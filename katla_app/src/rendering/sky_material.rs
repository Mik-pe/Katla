//! Sky material for procedural sky rendering.
//!
//! Pure configuration for a fullscreen triangle pipeline that renders a procedural sky
//! with atmospheric scattering and sun disk. The sky always renders behind
//! all geometry (depth write disabled, depth compare = always).

use katla_vulkan::{DescriptorSetLayoutBuilder, DescriptorType, ShaderStages, VertexBinding};
use std::path::PathBuf;

/// Sky material that renders a procedural sky background.
///
/// Uses a fullscreen triangle (no vertex input) with camera-relative
/// sky gradient using inverse view-projection matrix.
///
/// This is a pure configuration struct. Pipelines are created by the renderer
/// using `MaterialPipelineCache::get_or_create()`.
#[derive(katla_derive::Material)]
#[material(shader = "resources/shaders/sky.wgsl")]
#[material(domain = "PostProcess")]
#[material(depth_test = true, depth_write = false, cull_backfaces = false)]
pub struct SkyMaterial {
    pub vertex_binding: VertexBinding,
    pub shader_path: PathBuf,
    pub descriptor_layouts: Vec<DescriptorSetLayoutBuilder>,
}

impl Default for SkyMaterial {
    fn default() -> Self {
        Self {
            vertex_binding: VertexBinding { formats: vec![] },
            shader_path: PathBuf::from("resources/shaders/sky.wgsl"),
            descriptor_layouts: vec![
                DescriptorSetLayoutBuilder::new()
                    .add_binding(0, DescriptorType::StorageBuffer, ShaderStages::VERTEX_FRAGMENT)
                    .add_binding(1, DescriptorType::StorageBuffer, ShaderStages::VERTEX_FRAGMENT),
            ],
        }
    }
}
