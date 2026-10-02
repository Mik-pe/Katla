//! Validated texture subresources and upload service limits.

use super::{ImageFormat, TextureDescriptor};
use crate::error::RendererError;

/// Destination subresource plus the source's explicit byte pitches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureUploadRegion {
    pub mip_level: u32,
    pub array_layer: u32,
    pub origin: [u32; 3],
    pub extent: [u32; 3],
    /// Zero selects tightly packed block rows.
    pub bytes_per_row: usize,
    /// Zero selects tightly packed images using the selected row pitch.
    pub bytes_per_image: usize,
}

/// Checked source layout. Native backends may repack rows for alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureUploadLayout {
    pub row_bytes: usize,
    pub block_rows: usize,
    pub bytes_per_row: usize,
    pub bytes_per_image: usize,
    pub required_bytes: usize,
}

impl TextureUploadRegion {
    /// A complete base mip in the first array layer.
    pub fn base(desc: &TextureDescriptor) -> Self {
        Self {
            mip_level: 0,
            array_layer: 0,
            origin: [0; 3],
            extent: [desc.width, desc.height, desc.depth],
            bytes_per_row: 0,
            bytes_per_image: 0,
        }
    }

    /// Check mip, layer, origin, compressed edge rules, byte pitches and source length.
    pub fn validate(
        self,
        desc: &TextureDescriptor,
        data_len: usize,
    ) -> Result<TextureUploadLayout, RendererError> {
        desc.validate_data(0)?;
        let invalid = |reason: &str| RendererError::InvalidDescriptor {
            resource: desc.label.unwrap_or("texture upload").to_string(),
            reason: format!(
                "{:?} {}x{}x{}, mip {} layer {}, origin {:?} extent {:?}: {reason}",
                desc.format,
                desc.width,
                desc.height,
                desc.depth,
                self.mip_level,
                self.array_layer,
                self.origin,
                self.extent
            ),
        };
        if desc.format == ImageFormat::Auto || desc.format.is_depth_stencil() {
            return Err(invalid(
                "auto and depth/stencil formats cannot receive pixel uploads",
            ));
        }
        if self.mip_level >= desc.mip_levels || self.array_layer >= desc.array_layers {
            return Err(invalid("mip or array layer out of bounds"));
        }
        let dims = [desc.width, desc.height, desc.depth].map(|v| (v >> self.mip_level).max(1));
        for (axis, dim) in dims.iter().enumerate() {
            if self.extent[axis] == 0
                || self.origin[axis]
                    .checked_add(self.extent[axis])
                    .is_none_or(|end| end > *dim)
            {
                return Err(invalid("empty or out-of-bounds region"));
            }
        }
        let [bw, bh] = desc.format.block_extent();
        for (axis, block) in [bw, bh].iter().enumerate() {
            if !self.origin[axis].is_multiple_of(*block)
                || (!self.extent[axis].is_multiple_of(*block)
                    && self.origin[axis] + self.extent[axis] != dims[axis])
            {
                return Err(invalid(
                    "compressed region must align to blocks or end at the mip edge",
                ));
            }
        }
        let block_bytes = desc.format.bytes_per_block() as usize;
        let row_bytes = (self.extent[0].div_ceil(bw) as usize)
            .checked_mul(block_bytes)
            .ok_or_else(|| invalid("row size overflows"))?;
        let block_rows = self.extent[1].div_ceil(bh) as usize;
        let bytes_per_row = if self.bytes_per_row == 0 {
            row_bytes
        } else {
            self.bytes_per_row
        };
        if bytes_per_row < row_bytes || !bytes_per_row.is_multiple_of(block_bytes) {
            return Err(invalid(
                "row pitch is too small or not a multiple of format block bytes",
            ));
        }
        let image_bytes = bytes_per_row
            .checked_mul(block_rows)
            .ok_or_else(|| invalid("image pitch overflows"))?;
        let bytes_per_image = if self.bytes_per_image == 0 {
            image_bytes
        } else {
            self.bytes_per_image
        };
        if bytes_per_image < image_bytes || !bytes_per_image.is_multiple_of(block_bytes) {
            return Err(invalid(
                "image pitch is too small or not a multiple of format block bytes",
            ));
        }
        let required_bytes = bytes_per_image
            .checked_mul(self.extent[2] as usize - 1)
            .and_then(|v| v.checked_add(bytes_per_row.checked_mul(block_rows - 1)?))
            .and_then(|v| v.checked_add(row_bytes))
            .ok_or_else(|| invalid("source size overflows"))?;
        if data_len < required_bytes {
            return Err(RendererError::UploadFailed {
                resource: desc.label.unwrap_or("texture upload").to_string(),
                expected_bytes: required_bytes,
                actual_bytes: data_len,
                detail: format!(
                    "{:?}, mip {} layer {}, {:?}, row pitch {bytes_per_row}, image pitch {bytes_per_image}",
                    desc.format, self.mip_level, self.array_layer, self.extent
                ),
            });
        }
        Ok(TextureUploadLayout {
            row_bytes,
            block_rows,
            bytes_per_row,
            bytes_per_image,
            required_bytes,
        })
    }
}

/// Hard admission and per-submission limits. Uploads return an error before allocation when exceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureUploadBudget {
    pub max_queued_bytes: usize,
    pub max_staging_bytes: usize,
    pub max_bytes_per_submission: usize,
    pub max_uploads_per_submission: usize,
}
impl Default for TextureUploadBudget {
    fn default() -> Self {
        Self {
            max_queued_bytes: 256 * 1024 * 1024,
            max_staging_bytes: 512 * 1024 * 1024,
            max_bytes_per_submission: 256 * 1024 * 1024,
            max_uploads_per_submission: 4096,
        }
    }
}

/// Observable current gauges and cumulative upload work.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextureUploadMetrics {
    pub queued_bytes: usize,
    pub submitted_bytes: u64,
    pub staging_bytes: usize,
    pub staging_high_water_mark: usize,
    pub completed_batches: u64,
    pub completion_latency_ns: u64,
    pub failure_count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_upload_color_channel_and_float_layouts() {
        for (format, bytes) in [
            (ImageFormat::R8G8B8A8Unorm, 4),
            (ImageFormat::B8G8R8A8Srgb, 4),
            (ImageFormat::R8Unorm, 1),
            (ImageFormat::Rg8Unorm, 2),
            (ImageFormat::R32Sfloat, 4),
            (ImageFormat::R32Uint, 4),
            (ImageFormat::R16G16B16A16Sfloat, 8),
        ] {
            let desc = TextureDescriptor::new(7, 3, format);
            let layout = TextureUploadRegion::base(&desc)
                .validate(&desc, 21 * bytes)
                .unwrap();
            assert_eq!(layout.required_bytes, 21 * bytes);
            assert_eq!(layout.row_bytes, 7 * bytes);
        }
    }
    #[test]
    fn test_upload_depth_auto_rejected() {
        for format in [
            ImageFormat::Auto,
            ImageFormat::D32Sfloat,
            ImageFormat::D32SfloatS8Uint,
            ImageFormat::D24UnormS8Uint,
        ] {
            let desc = TextureDescriptor::new(4, 4, format);
            assert!(
                TextureUploadRegion::base(&desc)
                    .validate(&desc, 64)
                    .is_err()
            );
        }
    }
    #[test]
    fn test_upload_non_tight_pitches_and_3d() {
        let mut desc = TextureDescriptor::new(3, 2, ImageFormat::R8Unorm);
        desc.depth = 3;
        let mut region = TextureUploadRegion::base(&desc);
        region.bytes_per_row = 8;
        region.bytes_per_image = 32;
        assert_eq!(region.validate(&desc, 75).unwrap().required_bytes, 75);
        assert!(region.validate(&desc, 74).is_err());
        region.bytes_per_image = 8;
        assert!(region.validate(&desc, 75).is_err());
    }
    #[test]
    fn test_upload_mips_layers_and_partial_bounds() {
        let mut desc = TextureDescriptor::new(16, 8, ImageFormat::R8Unorm);
        desc.mip_levels = 5;
        desc.array_layers = 3;
        let region = TextureUploadRegion {
            mip_level: 2,
            array_layer: 2,
            origin: [1, 0, 0],
            extent: [3, 2, 1],
            bytes_per_row: 0,
            bytes_per_image: 0,
        };
        assert_eq!(region.validate(&desc, 6).unwrap().required_bytes, 6);
        assert!(
            TextureUploadRegion {
                array_layer: 3,
                ..region
            }
            .validate(&desc, 6)
            .is_err()
        );
        assert!(
            TextureUploadRegion {
                mip_level: 5,
                ..region
            }
            .validate(&desc, 6)
            .is_err()
        );
        assert!(
            TextureUploadRegion {
                origin: [2, 0, 0],
                ..region
            }
            .validate(&desc, 6)
            .is_err()
        );
    }
    #[test]
    fn test_upload_compressed_blocks_edges_and_alignment() {
        for (format, bytes) in [
            (ImageFormat::Bc1RgbaUnorm, 8),
            (ImageFormat::Bc3RgbaUnorm, 16),
        ] {
            let desc = TextureDescriptor::new(7, 7, format);
            let region = TextureUploadRegion::base(&desc);
            assert_eq!(
                region.validate(&desc, 4 * bytes).unwrap().required_bytes,
                4 * bytes
            );
            let edge = TextureUploadRegion {
                origin: [4, 4, 0],
                extent: [3, 3, 1],
                ..region
            };
            assert!(edge.validate(&desc, bytes).is_ok());
            assert!(
                TextureUploadRegion {
                    origin: [1, 0, 0],
                    extent: [4, 4, 1],
                    ..region
                }
                .validate(&desc, bytes)
                .is_err()
            );
            assert!(
                TextureUploadRegion {
                    extent: [3, 4, 1],
                    ..region
                }
                .validate(&desc, bytes)
                .is_err()
            );
        }
    }
    #[test]
    fn test_upload_pitch_overflow_rejected() {
        let desc = TextureDescriptor::new(1, 2, ImageFormat::R8Unorm);
        let region = TextureUploadRegion {
            bytes_per_row: usize::MAX,
            ..TextureUploadRegion::base(&desc)
        };
        assert!(region.validate(&desc, usize::MAX).is_err());
    }
}
