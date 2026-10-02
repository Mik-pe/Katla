//! Backend-neutral readback of an exact committed graph-image generation.

use crate::Size2D;
use crate::render_graph::ResourceId;
use crate::texture::ImageFormat;

use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) fn fresh_readback_id() -> u64 {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_ID
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .expect("readback identity space exhausted")
}

/// Identity of a retained exported image from one committed submission.
///
/// The backend allocates `id`; the other fields preserve its graph and frame provenance.
/// A forged, retired or cross-renderer source is rejected before recording GPU work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GraphTextureSource {
    pub id: u64,
    pub resource: ResourceId,
    pub frame_slot: usize,
    pub generation: u64,
    pub submission: u64,
}

/// Rectangular region of one image subresource.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureReadbackRegion {
    pub origin: [u32; 2],
    pub size: Size2D,
    pub mip_level: u32,
    pub array_layer: u32,
}

impl TextureReadbackRegion {
    /// Read one pixel from the first mip and array layer.
    pub fn pixel(x: u32, y: u32) -> Self {
        Self {
            origin: [x, y],
            size: Size2D::new(1, 1),
            mip_level: 0,
            array_layer: 0,
        }
    }
}

/// A queued copy; polling retains the exact source until completion or failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureReadbackTicket {
    pub id: u64,
    pub source: GraphTextureSource,
}

/// Tightly packed pixel rows in the declared image format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureReadbackData {
    pub format: ImageFormat,
    pub size: Size2D,
    pub bytes: Vec<u8>,
}

impl TextureReadbackData {
    /// Decode a single unsigned integer pixel without imposing application entity policy.
    pub fn single_u32(&self) -> Option<u32> {
        if self.format != ImageFormat::R32Uint || self.size != Size2D::new(1, 1) {
            return None;
        }
        Some(u32::from_ne_bytes(self.bytes.as_slice().try_into().ok()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integer_pixel_requires_exact_format_and_extent() {
        let mut data = TextureReadbackData {
            format: ImageFormat::R32Uint,
            size: Size2D::new(1, 1),
            bytes: 28u32.to_ne_bytes().to_vec(),
        };
        assert_eq!(data.single_u32(), Some(28));
        data.bytes.push(0);
        assert_eq!(data.single_u32(), None);
        data.bytes.pop();
        data.format = ImageFormat::R8G8B8A8Unorm;
        assert_eq!(data.single_u32(), None);
    }
}
