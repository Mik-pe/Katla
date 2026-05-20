//! Metal backend for the render graph.
//!
//! Implements `RenderGraphBackend` for `MetalRenderer`, providing
//! concrete transient texture creation, bindless management, and
//! frame indexing using Metal GPU resources.

use crate::metal::metal_renderer::MetalRenderer;
use crate::metal::metal_transient_texture::MetalTransientTexture;
use crate::render_graph::backend::RenderGraphBackend;
use crate::render_graph::error::RenderGraphError;
use crate::render_graph::resource::GraphResourceDesc;

impl RenderGraphBackend for MetalRenderer {
    type TransientTexture = MetalTransientTexture;

    fn create_transient_texture(
        _desc: &GraphResourceDesc,
    ) -> Result<Self::TransientTexture, RenderGraphError> {
        Err(RenderGraphError::InvalidConfiguration(
            "Metal transient textures must be created through FrameGraph::initialize_transient_textures()".to_string(),
        ))
    }

    fn destroy_transient_texture(texture: Self::TransientTexture) {
        drop(texture);
    }

    fn current_frame(&self) -> usize {
        // Access frame_index directly since it's a private field in the same crate.
        // The GpuRenderer impl uses `(self.frame_index % 3) as usize`.
        self.frame_index()
    }

    fn register_bindless_texture(
        &mut self,
        texture: &Self::TransientTexture,
    ) -> Result<u32, RenderGraphError> {
        self.register_metal_bindless_texture(&texture.view.inner)
            .map_err(|e| RenderGraphError::VulkanError(e.to_string()))
    }

    fn update_bindless_texture(
        &mut self,
        _slot: u32,
        texture: &Self::TransientTexture,
    ) -> Result<(), RenderGraphError> {
        // Metal's argument buffer is updated on registration;
        // updating an existing slot is a re-registration.
        self.register_metal_bindless_texture(&texture.view.inner)
            .map_err(|e| RenderGraphError::VulkanError(e.to_string()))?;
        Ok(())
    }
}
