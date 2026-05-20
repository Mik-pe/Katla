//! Render graph backend trait.
//!
//! Defines the interface that GPU backends must implement to execute
//! a render graph. The render graph core (Layer 1) uses this trait
//! to delegate all GPU-specific work to the backend.

use super::error::RenderGraphError;
use super::resource::{GraphResourceDesc, TransientTextureOps};

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

    /// Create a transient texture for a resource descriptor.
    fn create_transient_texture(
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
}
