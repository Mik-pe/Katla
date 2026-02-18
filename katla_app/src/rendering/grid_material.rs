//! Grid material for editor grid rendering.
//!
//! Creates a fullscreen triangle pipeline that renders an infinite grid
//! on the XZ plane at Y=0. Uses Ben Golus's "Best Darn Grid Shader" algorithm
//! for anti-aliased, perspective-correct grid lines.

use katla_vulkan::{
    context::VulkanContext, material::MaterialPipeline, ImageFormat, MaterialBuilder,
    VertexBinding,
};
use std::{cell::RefCell, path::Path, rc::Rc};

/// Grid material that renders an infinite editor grid.
///
/// Uses a fullscreen triangle (no vertex input) with ray-plane intersection
/// to determine world position on the XZ plane at Y=0.
///
/// The grid:
/// - Renders after sky but before geometry
/// - Is depth-tested but doesn't write depth (geometry can occlude grid)
/// - Fades out at distance and when camera is high above the plane
pub struct GridMaterial {
    pub pipeline: Rc<RefCell<MaterialPipeline>>,
    /// Empty vertex binding - grid shader uses @builtin(vertex_index)
    pub vertex_binding: VertexBinding,
}

impl GridMaterial {
    /// Create a new grid material with the given Vulkan context.
    ///
    /// The grid pipeline is configured with:
    /// - Depth test enabled, depth write disabled
    /// - Depth compare = LESS (grid occluded by closer geometry)
    /// - No backface culling (fullscreen quad)
    /// - Storage buffer mode for camera-relative grid
    pub fn new(context: Rc<VulkanContext>) -> Self {
        // Empty vertex binding - shader generates vertices from vertex_index
        let vertex_binding = VertexBinding { formats: vec![] };

        // Use storage buffer mode for camera-relative grid
        let pipeline = MaterialBuilder::new(context)
            .with_vertex_binding(vertex_binding.clone())
            .with_wgsl_shader(Path::new("resources/shaders/grid.wgsl"))
            .with_grid_rendering()
            .with_color_format(ImageFormat::B8G8R8A8Srgb)
            .with_depth_format(ImageFormat::D32SfloatS8Uint)
            .build_with_storage()
            .expect("Failed to create grid pipeline");

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
