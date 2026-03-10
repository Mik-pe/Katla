//! UI rendering subsystem.
//!
//! The `UIRenderer` owns all UI-specific rendering state and provides a clean API
//! for UI operations without polluting the core renderer interface.

use crate::TextureHandle;
use crate::renderer::UiFrameResources;

/// UI rendering subsystem.
///
/// Owns all UI-specific GPU resources and provides font atlas management.
/// Wraps a reference to VulkanRenderer for low-level graphics operations.
pub struct UIRenderer {
    /// Per-frame UI rendering resources (vertex/index buffers, descriptor sets, uniform buffer).
    ui_resources: UiFrameResources,
    /// Font atlas texture handle for text rendering.
    font_atlas: Option<TextureHandle>,
}

impl UIRenderer {
    /// Create a new UI rendering subsystem.
    pub(crate) fn new() -> Self {
        Self {
            ui_resources: UiFrameResources::default(),
            font_atlas: None,
        }
    }

    /// Set the font atlas texture handle.
    ///
    /// Called by VulkanRenderer after creating the font atlas texture.
    /// This is an internal method - use VulkanRenderer::create_ui_font_atlas() instead.
    pub(crate) fn set_font_atlas(&mut self, handle: TextureHandle) {
        log::debug!("Font atlas handle set to: {:?}", handle);
        self.font_atlas = Some(handle);
    }

    /// Get the font atlas texture handle.
    ///
    /// Returns `None` if no font atlas has been created yet.
    pub fn font_atlas(&self) -> Option<TextureHandle> {
        self.font_atlas
    }

    /// Get mutable access to UI resources (for frame graph execution).
    ///
    /// This is used internally by the frame graph to manage per-frame UI buffers.
    pub(crate) fn ui_resources_mut(&mut self) -> &mut UiFrameResources {
        &mut self.ui_resources
    }

    /// Get the font atlas texture handle (for frame graph execution).
    ///
    /// This is used internally by the frame graph when binding UI resources.
    pub(crate) fn font_atlas_handle(&self) -> Option<TextureHandle> {
        self.font_atlas
    }
}

impl Default for UIRenderer {
    fn default() -> Self {
        Self::new()
    }
}
