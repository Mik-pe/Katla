//! Gizmo material for 3D transform manipulation.
//!
//! Creates a pipeline for rendering gizmos (translate/rotate/scale handles)
//! with unlit rendering and always-on-top depth behavior.

use katla_vulkan::{
    context::VulkanContext,
    material::{MaterialPipeline, RenderState, ShaderSource},
    DescriptorSetLayoutBuilder, DescriptorType, ImageFormat,
    MaterialDomain, MaterialPipelineCache, ShaderStages, VertexBinding, VertexFormat,
};
use std::{cell::RefCell, path::PathBuf, rc::Rc};

/// Gizmo material configuration (lightweight, implements Material for cache lookup).
#[derive(katla_derive::Material)]
#[material(domain = "Surface")]
#[material(depth_test = true, depth_write = false, cull_backfaces = false)]
struct GizmoMaterialConfig {
    vertex_binding: VertexBinding,
    shader_path: PathBuf,
    descriptor_layouts: Vec<DescriptorSetLayoutBuilder>,
}

/// Gizmo material that renders 3D manipulation handles.
pub struct GizmoMaterial {
    pub pipeline: Rc<RefCell<MaterialPipeline>>,
    vertex_binding: VertexBinding,
    shader_path: PathBuf,
    descriptor_layouts: Vec<DescriptorSetLayoutBuilder>,
}

impl GizmoMaterial {
    /// Create a gizmo material using the pipeline cache.
    pub fn new_cached(_context: Rc<VulkanContext>, cache: &mut MaterialPipelineCache) -> Self {
        let vertex_binding = VertexBinding {
            formats: vec![VertexFormat::RGB32f, VertexFormat::RGB32f],
        };
        let shader_path = PathBuf::from("resources/shaders/gizmo.wgsl");

        let descriptor_layouts = vec![
            DescriptorSetLayoutBuilder::new()
                .add_binding(0, DescriptorType::StorageBuffer, ShaderStages::VERTEX_FRAGMENT)
                .add_binding(1, DescriptorType::StorageBuffer, ShaderStages::VERTEX_FRAGMENT),
        ];

        let config = GizmoMaterialConfig {
            vertex_binding: vertex_binding.clone(),
            shader_path: shader_path.clone(),
            descriptor_layouts: descriptor_layouts.clone(),
        };

        let pipeline = cache.get_or_create(&config).expect("Failed to create gizmo pipeline");

        Self { pipeline, vertex_binding, shader_path, descriptor_layouts }
    }

    pub fn pipeline(&self) -> Rc<RefCell<MaterialPipeline>> {
        Rc::clone(&self.pipeline)
    }
}

impl katla_vulkan::Material for GizmoMaterial {
    fn vertex_shader(&self) -> ShaderSource { ShaderSource::WgslFile(self.shader_path.clone()) }
    fn fragment_shader(&self) -> ShaderSource { ShaderSource::WgslFile(self.shader_path.clone()) }
    fn vertex_binding(&self) -> VertexBinding { self.vertex_binding.clone() }
    fn render_state(&self) -> RenderState {
        RenderState { depth_test: true, depth_write: false, cull_backfaces: false, alpha_blending: false }
    }
    fn descriptor_layouts(&self) -> Vec<DescriptorSetLayoutBuilder> { self.descriptor_layouts.clone() }
    fn color_format(&self) -> ImageFormat { ImageFormat::R16G16B16A16Sfloat }
    fn depth_format(&self) -> ImageFormat { ImageFormat::D32SfloatS8Uint }
    fn domain(&self) -> MaterialDomain { MaterialDomain::Surface }
}
