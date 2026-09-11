use crate::texture::ImageFormat;

use super::texture::{MetalTexture, MetalTextureView};

pub struct MetalTransientTexture {
    pub texture: MetalTexture,
    pub view: MetalTextureView,
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
    pub bindless_slot: Option<u32>,
}

impl MetalTransientTexture {
    pub fn new(
        texture: MetalTexture,
        view: MetalTextureView,
        format: ImageFormat,
        width: u32,
        height: u32,
    ) -> Self {
        Self {
            texture,
            view,
            format,
            width,
            height,
            bindless_slot: None,
        }
    }
}
