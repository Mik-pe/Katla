//! Sky material for procedural sky rendering.
//!
//! Creates a fullscreen triangle pipeline that renders a procedural sky
//! with atmospheric scattering and sun disk. The sky always renders behind
//! all geometry (depth write disabled, depth compare = always).

use katla_vulkan::{
    material::{MaterialPipeline, RenderState, ShaderSource},
    DescriptorSetLayoutBuilder, DescriptorType, ImageFormat,
    MaterialDomain, MaterialPipelineCache, ShaderStages, VertexBinding,
};
use std::{cell::RefCell, path::PathBuf, rc::Rc};

/// Sky material that renders a procedural sky background.
///
/// Uses a fullscreen triangle (no vertex input) with camera-relative
/// sky gradient using inverse view-projection matrix.
pub struct SkyMaterial {
    pub pipeline: Rc<RefCell<MaterialPipeline>>,
}

impl SkyMaterial {
    /// Create a sky material using the pipeline cache.
    pub fn new(cache: &mut MaterialPipelineCache) -> Self {
        let config = SkyMaterialConfig;
        let pipeline = cache.get_or_create(&config).expect("Failed to create sky pipeline");
        Self { pipeline }
    }
}

/// Config struct for pipeline cache lookup (internal).
struct SkyMaterialConfig;

impl katla_vulkan::Material for SkyMaterialConfig {
    fn vertex_shader(&self) -> ShaderSource {
        ShaderSource::WgslFile(PathBuf::from("resources/shaders/sky.wgsl"))
    }

    fn fragment_shader(&self) -> ShaderSource {
        ShaderSource::WgslFile(PathBuf::from("resources/shaders/sky.wgsl"))
    }

    fn vertex_binding(&self) -> VertexBinding {
        VertexBinding { formats: vec![] }
    }

    fn render_state(&self) -> RenderState {
        RenderState {
            depth_test: true,
            depth_write: false,
            cull_backfaces: false,
            alpha_blending: false,
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
