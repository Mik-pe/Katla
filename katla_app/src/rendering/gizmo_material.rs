//! Gizmo material for 3D transform manipulation.
//!
//! Creates a pipeline for rendering gizmos (translate/rotate/scale handles)
//! with unlit rendering and always-on-top depth behavior.

use katla_vulkan::{
    context::VulkanContext, material::MaterialPipeline, ImageFormat, MaterialBuilder,
    VertexBinding, VertexFormat,
};
use std::{cell::RefCell, path::Path, rc::Rc};

/// Gizmo material that renders 3D manipulation handles.
///
/// Uses unlit rendering with depth test set to ALWAYS so gizmos
/// are always visible on top of scene geometry.
pub struct GizmoMaterial {
    pub pipeline: Rc<RefCell<MaterialPipeline>>,
}

impl GizmoMaterial {
    /// Create a new gizmo material with the given Vulkan context.
    ///
    /// The gizmo pipeline is configured with:
    /// - Depth test enabled but depth write disabled
    /// - Depth compare = ALWAYS (gizmos always visible on top)
    /// - No backface culling (gizmos visible from any angle)
    /// - Storage buffer mode for transform uniforms
    pub fn new(context: Rc<VulkanContext>) -> Self {
        // Gizmo vertex format: position (vec3), color (vec3)
        let vertex_binding = VertexBinding {
            formats: vec![
                VertexFormat::RGB32f, // position
                VertexFormat::RGB32f, // color
            ],
        };

        let pipeline = MaterialBuilder::new(context)
            .with_vertex_binding(vertex_binding)
            .with_wgsl_shader(Path::new("resources/shaders/gizmo.wgsl"))
            .with_sky_rendering() // Always visible on top
            .with_color_format(ImageFormat::B8G8R8A8Srgb)
            .with_depth_format(ImageFormat::D32SfloatS8Uint)
            .build_with_storage()
            .expect("Failed to create gizmo pipeline");

        Self {
            pipeline: Rc::new(RefCell::new(pipeline)),
        }
    }

    /// Get the pipeline as a cloned Rc<RefCell<>> for registration.
    pub fn pipeline(&self) -> Rc<RefCell<MaterialPipeline>> {
        Rc::clone(&self.pipeline)
    }
}
