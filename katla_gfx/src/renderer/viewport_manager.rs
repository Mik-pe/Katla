//! Viewport manager for viewport and render target management.
//!
//! ViewportManager provides a clean internal API for managing viewports
//! and render targets. This module organizes viewport-related functionality
//! away from VulkanRenderer.

use crate::viewport::{Viewport, ViewportBuilder, ViewportHandle};
use crate::{RendererError, Size2D, VulkanContext};
use ash::vk;
use log::error;
use std::rc::Rc;

/// Viewport manager for managing viewports and render targets.
///
/// Handles viewport creation, lookup, and lifecycle management.
pub(crate) struct ViewportManager {
    /// Vulkan context for resource creation.
    context: Rc<VulkanContext>,
    /// Viewport storage.
    viewports: Vec<Viewport>,
}

impl ViewportManager {
    /// Create a new viewport manager.
    pub(crate) fn new(context: Rc<VulkanContext>) -> Self {
        Self {
            context,
            viewports: Vec::new(),
        }
    }

    /// Create a new viewport builder.
    ///
    /// Returns a builder for configuring and creating a viewport.
    pub(crate) fn create(&mut self) -> ViewportBuilder {
        ViewportBuilder::new()
    }

    /// Register a created viewport.
    ///
    /// # Arguments
    /// * `viewport` - The viewport to register
    ///
    /// # Returns
    /// A handle to the registered viewport.
    pub(crate) fn register_viewport(&mut self, viewport: Viewport) -> ViewportHandle {
        let handle = ViewportHandle(self.viewports.len());
        self.viewports.push(viewport);
        handle
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

    /// Check if a viewport is ready for rendering.
    pub(crate) fn is_ready(&self, handle: ViewportHandle) -> bool {
        self.get(handle)
            .is_some_and(|v| v.storage_manager.is_some() && v.storage_descriptor.is_some())
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

        // Mark as destroyed by setting to None (we'll use Option<Viewport> in the future)
        // For now, just remove it
        self.viewports.remove(handle.0);

        true
    }

    /// Clear all viewports.
    pub(crate) fn clear(&mut self) {
        self.viewports.clear();
    }

    /// Get an iterator over all viewports.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &Viewport> {
        self.viewports.iter()
    }

    /// Get a mutable iterator over all viewports.
    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = &mut Viewport> {
        self.viewports.iter_mut()
    }
}
