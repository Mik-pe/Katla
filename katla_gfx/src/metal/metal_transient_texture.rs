use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::MTLHeap;

/// Native storage retained for the owning frame slot.
pub struct MetalTransientAllocation {
    pub frame_slot: usize,
    pub slot: u32,
    pub offset: u64,
    pub bytes: u64,
    pub logical_bytes: u64,
    pub memoryless: bool,
    pub aliased: bool,
    pub heap: Option<Retained<ProtocolObject<dyn MTLHeap>>>,
}

use crate::texture::ImageFormat;

use super::texture::{MetalTexture, MetalTextureView};

pub struct MetalTransientTexture {
    pub texture: MetalTexture,
    pub view: MetalTextureView,
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
    pub bindless_slot: Option<u32>,
    pub allocation: MetalTransientAllocation,
}

impl MetalTransientTexture {
    pub fn new(
        texture: MetalTexture,
        view: MetalTextureView,
        format: ImageFormat,
        width: u32,
        height: u32,
        allocation: MetalTransientAllocation,
    ) -> Self {
        Self {
            texture,
            view,
            format,
            width,
            height,
            bindless_slot: None,
            allocation,
        }
    }
}
