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
use crate::texture::{ImageFormat, TextureDescriptor, TextureUsage};

impl RenderGraphBackend for MetalRenderer {
    type TransientTexture = MetalTransientTexture;
    type ImageView = crate::metal::texture::MetalTextureView;

    fn create_transient_texture(
        &self,
        desc: &GraphResourceDesc,
    ) -> Result<Self::TransientTexture, RenderGraphError> {
        let usage = match desc.resource_type {
            crate::render_graph::resource::GraphResourceType::ColorAttachment { .. } => {
                TextureUsage::COLOR_ATTACHMENT | TextureUsage::SAMPLED
            }
            crate::render_graph::resource::GraphResourceType::DepthAttachment {
                sampled, ..
            } => {
                let mut u = TextureUsage::DEPTH_STENCIL_ATTACHMENT;
                if sampled {
                    u |= TextureUsage::SAMPLED;
                }
                u
            }
            crate::render_graph::resource::GraphResourceType::SampledImage => TextureUsage::SAMPLED,
        };

        let tex_desc =
            TextureDescriptor::new(desc.width, desc.height, desc.format).with_usage(usage);

        let (texture, view) = self
            .context
            .create_texture(&tex_desc)
            .map_err(|e| RenderGraphError::BackendError(e.to_string()))?;

        Ok(MetalTransientTexture::new(
            texture,
            view,
            desc.format,
            desc.width,
            desc.height,
        ))
    }

    fn destroy_transient_texture(texture: Self::TransientTexture) {
        drop(texture);
    }

    fn current_frame(&self) -> usize {
        self.frame_index()
    }

    fn register_bindless_texture(
        &mut self,
        texture: &Self::TransientTexture,
    ) -> Result<u32, RenderGraphError> {
        self.register_metal_bindless_texture(&texture.view.inner)
            .map_err(|e| RenderGraphError::BackendError(e.to_string()))
    }

    fn update_bindless_texture(
        &mut self,
        slot: u32,
        texture: &Self::TransientTexture,
    ) -> Result<(), RenderGraphError> {
        self.update_metal_bindless_texture(slot, &texture.view.inner)
            .map_err(|e| RenderGraphError::BackendError(e.to_string()))?;
        Ok(())
    }

    fn transient_texture_format(texture: &Self::TransientTexture) -> ImageFormat {
        texture.format
    }

    fn transient_texture_extent(texture: &Self::TransientTexture) -> (u32, u32) {
        (texture.width, texture.height)
    }

    fn transient_texture_is_depth(texture: &Self::TransientTexture) -> bool {
        matches!(
            texture.format,
            ImageFormat::D32Sfloat | ImageFormat::D32SfloatS8Uint | ImageFormat::D24UnormS8Uint
        )
    }

    fn transient_texture_bindless_slot(texture: &Self::TransientTexture) -> Option<u32> {
        texture.bindless_slot
    }

    fn set_transient_texture_bindless_slot(texture: &mut Self::TransientTexture, slot: u32) {
        texture.bindless_slot = Some(slot);
    }

    fn transient_texture_view(texture: &Self::TransientTexture) -> Self::ImageView {
        texture.view.clone()
    }

    fn swapchain_image_view(&self, _image_index: u32) -> Self::ImageView {
        self.drawable_texture_view
            .clone()
            .expect("No drawable texture view — call begin_frame first")
    }

    fn depth_image_view(&self, _frame_index: usize) -> Option<Self::ImageView> {
        self.depth_texture_view.clone()
    }
}
