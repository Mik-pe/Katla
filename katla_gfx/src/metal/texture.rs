use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::MTLTexture;

use crate::backend::resource::{GpuImage, GpuImageView};
use crate::backend::traits::GpuBackend;
use crate::texture::ImageFormat;

use super::MetalBackend;

pub struct MetalTexture {
    pub inner: Retained<ProtocolObject<dyn MTLTexture>>,
    format: ImageFormat,
}

impl Clone for MetalTexture {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            format: self.format,
        }
    }
}

impl MetalTexture {
    pub fn new(inner: Retained<ProtocolObject<dyn MTLTexture>>, format: ImageFormat) -> Self {
        Self { inner, format }
    }
}

impl GpuImage for MetalTexture {
    fn width(&self) -> u32 {
        self.inner.width() as u32
    }

    fn height(&self) -> u32 {
        self.inner.height() as u32
    }

    fn format(&self) -> ImageFormat {
        self.format
    }

    fn mip_levels(&self) -> u32 {
        self.inner.mipmapLevelCount() as u32
    }
}

unsafe impl Send for MetalTexture {}
unsafe impl Sync for MetalTexture {}

pub struct MetalTextureView {
    pub inner: Retained<ProtocolObject<dyn MTLTexture>>,
    parent: MetalTexture,
}

impl Clone for MetalTextureView {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            parent: self.parent.clone(),
        }
    }
}

impl MetalTextureView {
    pub fn new(inner: Retained<ProtocolObject<dyn MTLTexture>>, parent: MetalTexture) -> Self {
        Self { inner, parent }
    }
}

impl GpuImageView<MetalBackend> for MetalTextureView {
    fn image(&self) -> &<MetalBackend as GpuBackend>::Image {
        &self.parent
    }
}

unsafe impl Send for MetalTextureView {}
unsafe impl Sync for MetalTextureView {}
