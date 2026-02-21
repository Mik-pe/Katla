//! Frame resources for render graph building.
//!
//! This module provides high-level abstractions for render resources,
//! allowing the application layer to reference resources without knowing
//! about ResourceId or Vulkan types.

use crate::ResourceId;

/// Opaque render target handle.
///
/// This wraps a ResourceId internally, providing a type-safe way for
/// the application layer to reference render targets without knowing
/// about resource IDs or Vulkan types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RenderTarget(pub(crate) ResourceId);

impl RenderTarget {
    /// Create a new render target from a resource ID.
    /// This is only available within katla_vulkan.
    pub(crate) fn new(id: ResourceId) -> Self {
        Self(id)
    }

    /// Get the underlying resource ID.
    pub fn resource_id(&self) -> ResourceId {
        self.0
    }
}

/// Frame resources for render graph building.
///
/// This struct provides semantic names for all render resources,
/// allowing the application to reference them without knowing
/// about resource IDs or Vulkan types.
///
/// # Example
///
/// ```ignore
/// let resources = renderer.frame_resources();
///
/// builder.add_pass("sky_pass", |pass| {
///     pass.write_color(&resources.viewport_color)
///         .clear_color([0.4, 0.6, 0.9, 1.0]);
/// });
/// ```
#[derive(Clone, Debug)]
pub struct FrameResources {
    /// The swapchain image (presentation target)
    pub swapchain: RenderTarget,
    /// The viewport color attachment (main scene render target)
    pub viewport_color: RenderTarget,
    /// The viewport depth attachment (main scene depth buffer)
    pub viewport_depth: RenderTarget,
    /// The output color attachment (UI composition target)
    pub output_color: RenderTarget,
}

impl FrameResources {
    /// Create a new FrameResources with the given render targets.
    /// This is only available within katla_vulkan.
    pub(crate) fn new(
        swapchain: ResourceId,
        viewport_color: ResourceId,
        viewport_depth: ResourceId,
        output_color: ResourceId,
    ) -> Self {
        Self {
            swapchain: RenderTarget::new(swapchain),
            viewport_color: RenderTarget::new(viewport_color),
            viewport_depth: RenderTarget::new(viewport_depth),
            output_color: RenderTarget::new(output_color),
        }
    }
}
