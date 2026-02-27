//! Render target handle for render graph building.
//!
//! This module provides an opaque wrapper around ResourceId,
//! allowing the application layer to reference render targets
//! without knowing about Vulkan types.

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
    pub fn new(id: ResourceId) -> Self {
        Self(id)
    }

    /// Get the underlying resource ID.
    pub fn resource_id(&self) -> ResourceId {
        self.0
    }
}
