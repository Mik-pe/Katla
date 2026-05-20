//! Render graph backend trait.
//!
//! Defines the interface that GPU backends must implement to execute
//! a render graph. The render graph core (Layer 1) uses this trait
//! to delegate all GPU-specific work to the backend.

use super::error::RenderGraphError;
use super::resource::{GraphResourceDesc, TransientTextureOps};
use crate::texture::ImageFormat;

/// Backend interface for render graph execution.
///
/// Each GPU backend (Vulkan, Metal) implements this trait to provide
/// concrete resource creation, barrier insertion, and pass execution.
///
/// The trait is designed so that the render graph core can drive execution
/// without knowing about GPU-specific types. Backend implementations handle
/// all GPU-specific details internally.
pub trait RenderGraphBackend: Sized + 'static {
    /// Backend-specific transient texture type.
    type TransientTexture: TransientTextureOps;

    /// Backend-specific image view type for render pass attachments.
    type ImageView: Clone + Send + Sync;

    /// Create a transient texture for a resource descriptor.
    fn create_transient_texture(
        &self,
        desc: &GraphResourceDesc,
    ) -> Result<Self::TransientTexture, RenderGraphError>;

    /// Destroy a transient texture.
    fn destroy_transient_texture(texture: Self::TransientTexture);

    /// Current frame index (for double-buffered resources).
    fn current_frame(&self) -> usize;

    /// Register a texture with the bindless system, return slot.
    fn register_bindless_texture(
        &mut self,
        texture: &Self::TransientTexture,
    ) -> Result<u32, RenderGraphError>;

    /// Update an existing bindless texture slot with a new texture.
    fn update_bindless_texture(
        &mut self,
        slot: u32,
        texture: &Self::TransientTexture,
    ) -> Result<(), RenderGraphError>;

    /// Get the image format of a transient texture.
    fn transient_texture_format(texture: &Self::TransientTexture) -> ImageFormat;

    /// Get the width and height of a transient texture.
    fn transient_texture_extent(texture: &Self::TransientTexture) -> (u32, u32);

    /// Whether the transient texture is a depth format.
    fn transient_texture_is_depth(texture: &Self::TransientTexture) -> bool;

    /// Get or set the bindless slot stored on a transient texture.
    fn transient_texture_bindless_slot(texture: &Self::TransientTexture) -> Option<u32>;
    fn set_transient_texture_bindless_slot(texture: &mut Self::TransientTexture, slot: u32);

    /// Extract the image view from a transient texture for render pass attachment.
    fn transient_texture_view(texture: &Self::TransientTexture) -> Self::ImageView;

    /// Get the swapchain image view for the current frame's image index.
    fn swapchain_image_view(&self, image_index: u32) -> Self::ImageView;

    /// Get the depth buffer image view for a specific frame index.
    fn depth_image_view(&self, frame_index: usize) -> Option<Self::ImageView>;

    /// Transition a transient texture's resource state before a pass.
    fn transition_texture(
        _texture: &mut Self::TransientTexture,
        _from: super::resource::ResourceState,
        _to: super::resource::ResourceState,
    ) {
    }

    /// Transition the backbuffer image before a pass.
    fn transition_backbuffer(&self, _image_index: u32, _to: super::resource::ResourceState) {}

    /// Insert a depth render pass sync barrier between consecutive depth-using passes.
    fn depth_render_pass_sync(&self, _frame_index: usize) {}
}
