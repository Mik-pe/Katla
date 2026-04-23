//! Viewport manager for viewport configuration and lifecycle.
//!
//! ViewportManager provides a clean internal API for managing viewport
//! configuration. All rendering state is managed by the frame graph system.

use crate::Size2D;
use crate::viewport::{Viewport, ViewportBuilder, ViewportHandle};

/// Viewport manager for managing viewport configuration and lifecycle.
///
/// Handles viewport creation, lookup, and destruction. Rendering is managed
/// by the frame graph system.
pub(crate) struct ViewportManager {
    /// Viewport storage.
    viewports: Vec<Viewport>,
}

impl ViewportManager {
    /// Create a new viewport manager.
    pub(crate) fn new() -> Self {
        Self {
            viewports: Vec::new(),
        }
    }

    /// Create a new viewport builder.
    ///
    /// Returns a builder for configuring and creating a viewport.
    pub(crate) fn create(&mut self) -> ViewportBuilder {
        ViewportBuilder::new()
    }

    /// Get the number of viewports.
    pub(crate) fn count(&self) -> usize {
        self.viewports.len()
    }

    /// Get a viewport by handle.
    pub(crate) fn get(&self, handle: ViewportHandle) -> Option<&Viewport> {
        self.viewports.get(handle.0)
    }

    /// Get a mutable viewport by handle.
    pub(crate) fn get_mut(&mut self, handle: ViewportHandle) -> Option<&mut Viewport> {
        self.viewports.get_mut(handle.0)
    }

    /// Get the texture ID for a viewport.
    ///
    /// This is used for UI sampling of viewport textures. The texture is
    /// managed by the frame graph, not the viewport itself.
    pub(crate) fn texture_id(&self, handle: ViewportHandle) -> Option<u64> {
        self.get(handle).map(|_| {
            // Generate a unique texture ID based on viewport index
            // Using range 200+ to avoid conflicts with existing texture IDs
            200 + handle.0 as u64
        })
    }

    /// Get the extent (size) of a viewport.
    pub(crate) fn extent(&self, handle: ViewportHandle) -> Option<Size2D> {
        self.get(handle).map(|v| v.get_extent())
    }

    /// Destroy a viewport and free its resources.
    ///
    /// # Arguments
    /// * `handle` - The handle of the viewport to destroy
    ///
    /// # Returns
    /// `true` if the viewport was found and destroyed, `false` otherwise.
    pub(crate) fn destroy(&mut self, handle: ViewportHandle) -> bool {
        if handle.0 >= self.viewports.len() {
            return false;
        }

        // Swap-remove to avoid shifting indices of subsequent viewports
        self.viewports.swap_remove(handle.0);

        true
    }

    /// Clear all viewports.
    pub(crate) fn clear(&mut self) {
        self.viewports.clear();
    }
}
