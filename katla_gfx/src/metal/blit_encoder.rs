use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTL4CommandEncoder, MTL4ComputeCommandEncoder, MTLOrigin, MTLSize};

use crate::backend::command::*;
#[cfg(test)]
use crate::backend::resource::GpuImage;

use super::MetalBackend;
use super::buffer::MetalBuffer;
use super::texture::MetalTexture;

pub(crate) struct MetalBlitEncoder {
    pub(crate) inner: Retained<ProtocolObject<dyn MTL4ComputeCommandEncoder>>,
    resources: std::rc::Rc<super::encoding_resources::EncodingResources>,
    ended: std::cell::Cell<bool>,
}

impl MetalBlitEncoder {
    pub(crate) fn new(
        inner: Retained<ProtocolObject<dyn MTL4ComputeCommandEncoder>>,
        resources: std::rc::Rc<super::encoding_resources::EncodingResources>,
    ) -> Self {
        Self {
            inner,
            resources,
            ended: std::cell::Cell::new(false),
        }
    }
}

impl GpuBlitEncoder<MetalBackend> for MetalBlitEncoder {
    fn end_encoding(self) {
        if !self.ended.replace(true) {
            self.inner.endEncoding();
        }
    }

    fn copy_buffer_to_buffer(
        &mut self,
        src: &MetalBuffer,
        src_offset: u64,
        dst: &MetalBuffer,
        dst_offset: u64,
        size: u64,
    ) {
        self.retain_buffer(src);
        self.retain_buffer(dst);
        unsafe {
            self.inner
                .copyFromBuffer_sourceOffset_toBuffer_destinationOffset_size(
                    &src.inner,
                    src_offset as usize,
                    &dst.inner,
                    dst_offset as usize,
                    size as usize,
                );
        }
    }

    #[cfg(test)]
    fn copy_texture_to_texture(&mut self, src: &MetalTexture, dst: &MetalTexture) {
        self.retain_texture(src);
        self.retain_texture(dst);
        unsafe {
            self.inner
                .copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toTexture_destinationSlice_destinationLevel_destinationOrigin(
                    &src.inner,
                    0,
                    0,
                    MTLOrigin { x: 0, y: 0, z: 0 },
                    MTLSize {
                        width: src.width() as usize,
                        height: src.height() as usize,
                        depth: 1,
                    },
                    &dst.inner,
                    0,
                    0,
                    MTLOrigin { x: 0, y: 0, z: 0 },
                );
        }
    }
}

impl MetalBlitEncoder {
    fn retain_buffer(&self, buffer: &MetalBuffer) {
        self.resources
            .residency
            .add_buffer(&buffer.inner)
            .expect("Metal transfer buffer residency");
    }
    fn retain_texture(&self, texture: &MetalTexture) {
        self.resources
            .residency
            .add_texture(&texture.inner)
            .expect("Metal transfer texture residency");
    }

    pub(crate) fn copy_buffer_to_texture_region(
        &mut self,
        src: &MetalBuffer,
        dst: &MetalTexture,
        region: crate::texture::TextureUploadRegion,
    ) {
        self.retain_buffer(src);
        self.retain_texture(dst);
        unsafe {
            self.inner.copyFromBuffer_sourceOffset_sourceBytesPerRow_sourceBytesPerImage_sourceSize_toTexture_destinationSlice_destinationLevel_destinationOrigin(
            &src.inner, 0, region.bytes_per_row, region.bytes_per_image,
            MTLSize { width: region.extent[0] as usize, height: region.extent[1] as usize, depth: region.extent[2] as usize },
            &dst.inner, region.array_layer as usize, region.mip_level as usize,
            MTLOrigin { x: region.origin[0] as usize, y: region.origin[1] as usize, z: region.origin[2] as usize });
        }
    }
    pub(crate) fn barrier_transfers(&self) {
        self.inner
            .barrierAfterEncoderStages_beforeEncoderStages_visibilityOptions(
                objc2_metal::MTLStages::Blit,
                objc2_metal::MTLStages::Blit,
                objc2_metal::MTL4VisibilityOptions::Device,
            );
        super::sync::capture_native_boundary(
            &self.resources,
            "transfer_visibility",
            super::sync::NativeBoundary::Transfers,
            objc2_metal::MTLStages::Blit,
            objc2_metal::MTLStages::Blit,
            objc2_metal::MTL4VisibilityOptions::Device,
        );
    }
    pub(crate) fn generate_mipmaps(&mut self, texture: &MetalTexture) {
        self.retain_texture(texture);
        self.inner
            .barrierAfterEncoderStages_beforeEncoderStages_visibilityOptions(
                objc2_metal::MTLStages::Blit,
                objc2_metal::MTLStages::Blit,
                objc2_metal::MTL4VisibilityOptions::Device,
            );
        super::sync::capture_native_boundary(
            &self.resources,
            "transfer_visibility",
            super::sync::NativeBoundary::Transfers,
            objc2_metal::MTLStages::Blit,
            objc2_metal::MTLStages::Blit,
            objc2_metal::MTL4VisibilityOptions::Device,
        );
        unsafe {
            self.inner.generateMipmapsForTexture(&texture.inner);
        }
    }
    pub(crate) fn fill_buffer(&mut self, buffer: &MetalBuffer, offset: u64, size: u64, value: u8) {
        self.retain_buffer(buffer);
        unsafe {
            self.inner.fillBuffer_range_value(
                &buffer.inner,
                objc2_foundation::NSRange {
                    location: offset as usize,
                    length: size as usize,
                },
                value,
            );
        }
    }
    pub(crate) fn copy_texture_region_to_buffer(
        &mut self,
        texture: &MetalTexture,
        region: crate::renderer::texture_readback::TextureReadbackRegion,
        buffer: &MetalBuffer,
        row_pitch: usize,
    ) {
        self.retain_texture(texture);
        self.retain_buffer(buffer);
        self.inner
            .barrierAfterQueueStages_beforeStages_visibilityOptions(
                objc2_metal::MTLStages::All,
                objc2_metal::MTLStages::Blit,
                objc2_metal::MTL4VisibilityOptions::Device,
            );
        super::sync::capture_native_boundary(
            &self.resources,
            "readback.acquire",
            super::sync::NativeBoundary::UploadAcquire,
            objc2_metal::MTLStages::All,
            objc2_metal::MTLStages::Blit,
            objc2_metal::MTL4VisibilityOptions::Device,
        );
        unsafe {
            self.inner.copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toBuffer_destinationOffset_destinationBytesPerRow_destinationBytesPerImage(
                &texture.inner, region.array_layer as usize, region.mip_level as usize,
                MTLOrigin { x: region.origin[0] as usize, y: region.origin[1] as usize, z: 0 },
                MTLSize { width: region.size.width as usize, height: region.size.height as usize, depth: 1 },
                &buffer.inner, 0, row_pitch, row_pitch * region.size.height as usize,
            );
        }
    }

    #[cfg(test)]
    pub(crate) fn copy_texture_pixel_to_buffer(
        &mut self,
        texture: &MetalTexture,
        x: u32,
        y: u32,
        buffer: &MetalBuffer,
    ) {
        self.retain_texture(texture);
        self.retain_buffer(buffer);
        self.inner
            .barrierAfterQueueStages_beforeStages_visibilityOptions(
                objc2_metal::MTLStages::All,
                objc2_metal::MTLStages::Blit,
                objc2_metal::MTL4VisibilityOptions::Device,
            );
        super::sync::capture_native_boundary(
            &self.resources,
            "readback.acquire",
            super::sync::NativeBoundary::UploadAcquire,
            objc2_metal::MTLStages::All,
            objc2_metal::MTLStages::Blit,
            objc2_metal::MTL4VisibilityOptions::Device,
        );
        let pitch = texture.format().bytes_per_pixel() as usize;
        unsafe {
            self.inner.copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toBuffer_destinationOffset_destinationBytesPerRow_destinationBytesPerImage(&texture.inner, 0, 0, MTLOrigin { x: x as usize, y: y as usize, z: 0 }, MTLSize {width:1,height:1,depth:1}, &buffer.inner, 0, pitch, pitch);
        }
    }
}

impl Drop for MetalBlitEncoder {
    fn drop(&mut self) {
        if !self.ended.replace(true) {
            self.inner.endEncoding();
        }
    }
}
