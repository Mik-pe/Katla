//! Grid material for editor grid rendering.
//!
//! Creates a fullscreen triangle pipeline that renders an infinite grid
//! on the XZ plane at Y=0. Uses Ben Golus's "Best Darn Grid Shader" algorithm
//! for anti-aliased, perspective-correct grid lines.

use katla_vulkan::{
    material::{MaterialPipeline, RenderState, ShaderSource},
    DescriptorSetLayoutBuilder, DescriptorType, ImageFormat,
    MaterialDomain, MaterialPipelineCache, ShaderStages, VertexBinding,
};
use std::{cell::RefCell, path::PathBuf, rc::Rc};

/// Grid material that renders an infinite editor grid.
pub struct GridMaterial {
    pub pipeline: Rc<RefCell<MaterialPipeline>>,
}

impl GridMaterial {
    /// Create a grid material using the pipeline cache.
    pub fn new(cache: &mut MaterialPipelineCache) -> Self {
        let config = GridMaterialConfig;
        let pipeline = cache.get_or_create(&config).expect("Failed to create grid pipeline");
        Self { pipeline }
    }
}

/// Config struct for pipeline cache lookup (internal).
struct GridMaterialConfig;

impl katla_vulkan::Material for GridMaterialConfig {
    fn vertex_shader(&self) -> ShaderSource {
        ShaderSource::WgslFile(PathBuf::from("resources/shaders/grid.wgsl"))
    }

    fn fragment_shader(&self) -> ShaderSource {
        ShaderSource::WgslFile(PathBuf::from("resources/shaders/grid.wgsl"))
    }

    fn vertex_binding(&self) -> VertexBinding {
        VertexBinding { formats: vec![] }
    }

    fn render_state(&self) -> RenderState {
        RenderState {
            depth_test: true,
            depth_write: false,
            cull_backfaces: false,
            alpha_blending: true,
        }
    }

    fn descriptor_layouts(&self) -> Vec<DescriptorSetLayoutBuilder> {
        vec![
            DescriptorSetLayoutBuilder::new()
                .add_binding(0, DescriptorType::StorageBuffer, ShaderStages::VERTEX_FRAGMENT)
                .add_binding(1, DescriptorType::StorageBuffer, ShaderStages::VERTEX_FRAGMENT),
        ]
    }

    fn color_format(&self) -> ImageFormat {
        ImageFormat::R16G16B16A16Sfloat
    }

    fn depth_format(&self) -> ImageFormat {
        ImageFormat::D32SfloatS8Uint
    }

    fn domain(&self) -> MaterialDomain {
        MaterialDomain::PostProcess
    }
}
