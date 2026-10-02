use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLResource, MTLTexture};

use crate::backend::resource::{GpuImage, GpuImageView};
use crate::backend::traits::GpuBackend;
use crate::texture::ImageFormat;

use super::MetalBackend;

pub struct MetalTexture {
    pub inner: Retained<ProtocolObject<dyn MTLTexture>>,
    format: ImageFormat,
    generate_mips: bool,
    label: Option<&'static str>,
}

impl Clone for MetalTexture {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            format: self.format,
            generate_mips: self.generate_mips,
            label: self.label,
        }
    }
}

impl MetalTexture {
    pub fn new(inner: Retained<ProtocolObject<dyn MTLTexture>>, format: ImageFormat) -> Self {
        Self {
            inner,
            format,
            generate_mips: false,
            label: None,
        }
    }
}

impl MetalTexture {
    pub(crate) fn with_upload_policy(
        mut self,
        generate: bool,
        label: Option<&'static str>,
    ) -> Self {
        self.generate_mips = generate;
        self.label = label;
        if let Some(label) = label {
            self.inner
                .setLabel(Some(&objc2_foundation::NSString::from_str(label)));
        }
        self
    }
    pub(crate) fn descriptor(&self) -> crate::texture::TextureDescriptor {
        let mut desc =
            crate::texture::TextureDescriptor::new(self.width(), self.height(), self.format);
        desc.depth = self.inner.depth() as u32;
        desc.array_layers = self.inner.arrayLength() as u32;
        desc.mip_levels = self.mip_levels();
        desc.generate_mips = self.generate_mips;
        desc.label = self.label;
        desc
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
