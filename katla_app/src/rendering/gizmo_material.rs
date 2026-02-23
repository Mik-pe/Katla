//! Gizmo material for 3D transform manipulation.
//!
//! Creates a pipeline for rendering gizmos (translate/rotate/scale handles)
//! with unlit rendering and always-on-top depth behavior.

use katla_vulkan::{
    material::{MaterialPipeline, RenderState, ShaderSource},
    DescriptorSetLayoutBuilder, DescriptorType, ImageFormat,
    MaterialDomain, MaterialPipelineCache, ShaderStages, VertexBinding, VertexFormat,
};
use std::{cell::RefCell, path::PathBuf, rc::Rc};

/// Gizmo material that renders 3D manipulation handles.
pub struct GizmoMaterial {
    pub pipeline: Rc<RefCell<MaterialPipeline>>,
}

impl GizmoMaterial {
    /// Create a gizmo material using the pipeline cache.
    pub fn new(cache: &mut MaterialPipelineCache) -> Self {
        let config = GizmoMaterialConfig;
        let pipeline = cache.get_or_create(&config).expect("Failed to create gizmo pipeline");
        Self { pipeline }
    }
}

/// Config struct for pipeline cache lookup (internal).
struct GizmoMaterialConfig;

impl katla_vulkan::Material for GizmoMaterialConfig {
    fn vertex_shader(&self) -> ShaderSource {
        ShaderSource::WgslFile(PathBuf::from("resources/shaders/gizmo.wgsl"))
    }

    fn fragment_shader(&self) -> ShaderSource {
        ShaderSource::WgslFile(PathBuf::from("resources/shaders/gizmo.wgsl"))
    }

    fn vertex_binding(&self) -> VertexBinding {
        VertexBinding {
            formats: vec![VertexFormat::RGB32f, VertexFormat::RGB32f],
        }
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
        MaterialDomain::Surface
    }
}
