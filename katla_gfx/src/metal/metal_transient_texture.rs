use std::cell::Cell;

use crate::render_pass::ResourceState;
use crate::texture::ImageFormat;

use super::texture::{MetalTexture, MetalTextureView};

pub(crate) struct MetalTransientTexture {
    pub(crate) texture: MetalTexture,
    pub(crate) view: MetalTextureView,
    pub(crate) format: ImageFormat,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) state: Cell<ResourceState>,
}

impl MetalTransientTexture {
    pub(crate) fn new(
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
            state: Cell::new(ResourceState::Undefined),
        }
    }
}
