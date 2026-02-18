//! Sky material for procedural sky rendering.
//!
//! Creates a fullscreen triangle pipeline that renders a procedural sky
//! with atmospheric scattering and sun disk. The sky always renders behind
//! all geometry (depth write disabled, depth compare = always).

use katla_vulkan::{
    context::VulkanContext, material::MaterialPipeline, ImageFormat, MaterialBuilder,
    VertexBinding,
};
use std::{cell::RefCell, path::Path, rc::Rc};

/// Sky material that renders a procedural sky background.
///
/// Uses a fullscreen triangle (no vertex input) with camera-relative
/// sky gradient using inverse view-projection matrix.
pub struct SkyMaterial {
    pub pipeline: Rc<RefCell<MaterialPipeline>>,
    /// Empty vertex binding - sky shader uses @builtin(vertex_index)
    pub vertex_binding: VertexBinding,
}

impl SkyMaterial {
    /// Create a new sky material with the given Vulkan context.
    ///
    /// The sky pipeline is configured with:
    /// - Depth test enabled but depth write disabled
    /// - Depth compare = ALWAYS (sky always behind geometry)
    /// - No backface culling (fullscreen quad)
    /// - Storage buffer mode for camera-relative sky
    pub fn new(context: Rc<VulkanContext>) -> Self {
        // Empty vertex binding - shader generates vertices from vertex_index
        let vertex_binding = VertexBinding { formats: vec![] };

        // Use storage buffer mode for camera-relative sky
        let pipeline = MaterialBuilder::new(context)
            .with_vertex_binding(vertex_binding.clone())
            .with_wgsl_shader(Path::new("resources/shaders/sky.wgsl"))
            .with_sky_rendering()
            .with_color_format(ImageFormat::R16G16B16A16Sfloat)
            .with_depth_format(ImageFormat::D32SfloatS8Uint)
            .build_with_storage()
            .expect("Failed to create sky pipeline");

        Self {
            pipeline: Rc::new(RefCell::new(pipeline)),
            vertex_binding,
        }
    }

    /// Get the pipeline as a cloned Rc<RefCell<>> for registration.
    pub fn pipeline(&self) -> Rc<RefCell<MaterialPipeline>> {
        Rc::clone(&self.pipeline)
    }
}
