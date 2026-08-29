use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLBlitCommandEncoder, MTLCommandEncoder, MTLOrigin, MTLSize};

use crate::backend::command::*;
use crate::backend::resource::GpuImage;

use super::MetalBackend;
use super::buffer::MetalBuffer;
use super::texture::MetalTexture;

pub(crate) struct MetalBlitEncoder {
    pub(crate) inner: Retained<ProtocolObject<dyn MTLBlitCommandEncoder>>,
}

impl MetalBlitEncoder {
    pub(crate) fn new(inner: Retained<ProtocolObject<dyn MTLBlitCommandEncoder>>) -> Self {
        Self { inner }
    }
}

impl MetalBlitEncoder {
    /// Copies rows of `src` into the full base mip of `dst` with explicit pitch.
    pub(crate) fn copy_buffer_to_texture_staged(
        &mut self,
        src: &MetalBuffer,
        dst: &MetalTexture,
        bytes_per_row: usize,
        width: u32,
        height: u32,
    ) {
        unsafe {
            self.inner.copyFromBuffer_sourceOffset_sourceBytesPerRow_sourceBytesPerImage_sourceSize_toTexture_destinationSlice_destinationLevel_destinationOrigin(
                &src.inner,
                0,
                bytes_per_row,
                bytes_per_row * height as usize,
                MTLSize {
                    width: width as usize,
                    height: height as usize,
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

impl GpuBlitEncoder<MetalBackend> for MetalBlitEncoder {
    fn end_encoding(self) {
        self.inner.endEncoding();
    }

    fn copy_buffer_to_buffer(
        &mut self,
        src: &MetalBuffer,
        src_offset: u64,
        dst: &MetalBuffer,
        dst_offset: u64,
        size: u64,
    ) {
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

    fn copy_buffer_to_texture(
        &mut self,
        src: &MetalBuffer,
        dst: &MetalTexture,
        regions: &[BufferImageCopy],
    ) {
        let bytes_per_pixel = dst.format().bytes_per_pixel() as usize;
        for region in regions {
            let bytes_per_row = region.image_width as usize * bytes_per_pixel;
            unsafe {
                self.inner.copyFromBuffer_sourceOffset_sourceBytesPerRow_sourceBytesPerImage_sourceSize_toTexture_destinationSlice_destinationLevel_destinationOrigin(
                    &src.inner,
                    region.buffer_offset as usize,
                    bytes_per_row,
                    bytes_per_row * region.image_height as usize,
                    MTLSize {
                        width: region.image_width as usize,
                        height: region.image_height as usize,
                        depth: region.image_depth as usize,
                    },
                    &dst.inner,
                    region.base_array_layer as usize,
                    region.mip_level as usize,
                    MTLOrigin { x: 0, y: 0, z: 0 },
                );
            }
        }
    }

    fn copy_texture_to_texture(&mut self, src: &MetalTexture, dst: &MetalTexture) {
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
