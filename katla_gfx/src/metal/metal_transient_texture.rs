use std::cell::Cell;

use crate::render_graph::TransientTextureOps;
use crate::render_pass::ResourceState;
use crate::texture::ImageFormat;

use super::texture::{MetalTexture, MetalTextureView};

pub struct MetalTransientTexture {
    pub texture: MetalTexture,
    pub view: MetalTextureView,
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
    pub state: Cell<ResourceState>,
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
            state: Cell::new(ResourceState::Undefined),
            bindless_slot: None,
        }
    }
}

impl TransientTextureOps for MetalTransientTexture {
    fn state(&self) -> ResourceState {
        self.state.get()
    }

    fn set_state(&self, state: ResourceState) {
        self.state.set(state);
    }
}
