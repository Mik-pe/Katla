//! UI rendering subsystem.
//!
//! The `UIRenderer` owns all UI-specific rendering state and provides a clean API
//! for UI operations without polluting the core renderer interface.

use std::rc::Rc;

use crate::TextureHandle;
use crate::renderer::UiFrameResources;
use crate::vulkan::context::VulkanContext;

/// UI rendering subsystem.
///
/// Owns all UI-specific GPU resources and provides font atlas management.
pub struct UIRenderer {
    /// Per-frame UI rendering resources (vertex/index buffers, descriptor sets, uniform buffer).
    ui_resources: UiFrameResources,
    /// Font atlas texture handle for text rendering.
    font_atlas: Option<TextureHandle>,
    /// Bindless texture slot index for the font atlas.
    /// This is the slot allocated by BindlessTextureManager when the font atlas is registered.
    font_atlas_bindless_slot: Option<u32>,
}

impl UIRenderer {
    /// Create a new UI rendering subsystem.
    pub(crate) fn new(context: &Rc<VulkanContext>) -> Self {
        Self {
            ui_resources: UiFrameResources::new(context),
            font_atlas: None,
            font_atlas_bindless_slot: None,
        }
    }

    /// Set the font atlas texture handle.
    ///
    /// Called by VulkanRenderer after creating the font atlas texture.
    pub(crate) fn set_font_atlas(&mut self, handle: TextureHandle) {
        log::debug!("Font atlas handle set to: {:?}", handle);
        self.font_atlas = Some(handle);
    }

    /// Set the font atlas bindless texture slot.
    ///
    /// Called by VulkanRenderer after registering the font atlas with the bindless system.
    pub(crate) fn set_font_atlas_bindless_slot(&mut self, slot: u32) {
        log::debug!("Font atlas bindless slot set to: {}", slot);
        self.font_atlas_bindless_slot = Some(slot);
    }

    /// Get the font atlas bindless texture slot.
    ///
    /// Returns None if the font atlas has not been registered with the bindless system yet.
    pub fn font_atlas_bindless_slot(&self) -> Option<u32> {
        self.font_atlas_bindless_slot
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
