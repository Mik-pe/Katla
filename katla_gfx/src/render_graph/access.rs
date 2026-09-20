//! Backend-neutral resource access declarations.
//!
//! Accesses describe what a pass does to a resource, where in the pipeline the
//! access occurs, and which part of the resource participates. The compiler can
//! retain this vocabulary across Vulkan and Metal instead of reconstructing
//! intent from coarse read/write lists or native layouts.
//!
//! Images range over aspects, mip levels, and array layers
//! ([`ImageSubresourceRange`]); buffers range over bytes ([`BufferByteRange`]).
//! Both use the same access vocabulary ([`ResourceAccessMode`],
//! [`ResourceAccessStage`]) so the compiler runs one range-aware hazard analysis
//! for either resource kind.

use std::fmt;
use std::ops::{BitAnd, BitOr, BitOrAssign};

use super::handles::ResourceId;

/// Image aspect mask used by render-graph subresource ranges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ImageAspects(u8);

impl ImageAspects {
    pub const NONE: Self = Self(0);
    pub const COLOR: Self = Self(1 << 0);
    pub const DEPTH: Self = Self(1 << 1);
    pub const STENCIL: Self = Self(1 << 2);
    pub const DEPTH_STENCIL: Self = Self(Self::DEPTH.0 | Self::STENCIL.0);
    pub const ALL: Self = Self(Self::COLOR.0 | Self::DEPTH.0 | Self::STENCIL.0);

    #[inline]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[inline]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    #[inline]
    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    #[inline]
    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    pub fn names(self) -> impl Iterator<Item = &'static str> {
        [
            (Self::COLOR, "color"),
            (Self::DEPTH, "depth"),
            (Self::STENCIL, "stencil"),
        ]
        .into_iter()
        .filter_map(move |(aspect, name)| self.contains(aspect).then_some(name))
    }
}

impl BitOr for ImageAspects {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for ImageAspects {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl BitAnd for ImageAspects {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl fmt::Display for ImageAspects {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names = self.names().collect::<Vec<_>>();
        if names.is_empty() {
            f.write_str("none")
        } else {
            f.write_str(&names.join("|"))
        }
    }
}

/// Rectangular image subresource range over aspect, mip, and array-layer axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImageSubresourceRange {
    pub aspects: ImageAspects,
    pub base_mip_level: u32,
    pub mip_level_count: u32,
    pub base_array_layer: u32,
    pub array_layer_count: u32,
}

impl ImageSubresourceRange {
    pub const WHOLE_COLOR: Self = Self::whole(ImageAspects::COLOR);
    pub const WHOLE_DEPTH: Self = Self::whole(ImageAspects::DEPTH);
    pub const WHOLE_DEPTH_STENCIL: Self = Self::whole(ImageAspects::DEPTH_STENCIL);

    pub const fn new(
        aspects: ImageAspects,
        base_mip_level: u32,
        mip_level_count: u32,
        base_array_layer: u32,
        array_layer_count: u32,
    ) -> Self {
        Self {
            aspects,
            base_mip_level,
            mip_level_count,
            base_array_layer,
            array_layer_count,
        }
    }

    pub const fn whole(aspects: ImageAspects) -> Self {
        Self::new(aspects, 0, u32::MAX, 0, u32::MAX)
    }

    #[inline]
    pub const fn is_empty(self) -> bool {
        self.aspects.is_empty() || self.mip_level_count == 0 || self.array_layer_count == 0
    }

    /// Whether this range covers every subresource of its resource.
    ///
    /// A count of [`u32::MAX`] means "all remaining", matching the typed
    /// declaration convention, so it is a whole-resource range exactly when it
    /// also starts at zero.
    #[inline]
    pub const fn covers_whole_resource(self) -> bool {
        self.base_mip_level == 0
            && self.mip_level_count == u32::MAX
            && self.base_array_layer == 0
            && self.array_layer_count == u32::MAX
    }

    #[inline]
    fn mip_end(self) -> u64 {
        u64::from(self.base_mip_level) + u64::from(self.mip_level_count)
    }

    #[inline]
    fn layer_end(self) -> u64 {
        u64::from(self.base_array_layer) + u64::from(self.array_layer_count)
    }

    pub fn overlaps(self, other: Self) -> bool {
        !self.is_empty()
            && !other.is_empty()
            && self.aspects.intersects(other.aspects)
            && u64::from(self.base_mip_level) < other.mip_end()
            && u64::from(other.base_mip_level) < self.mip_end()
            && u64::from(self.base_array_layer) < other.layer_end()
            && u64::from(other.base_array_layer) < self.layer_end()
    }

    pub fn intersection(self, other: Self) -> Option<Self> {
        if !self.overlaps(other) {
            return None;
        }

        let mip_start = u64::from(self.base_mip_level.max(other.base_mip_level));
        let mip_end = self.mip_end().min(other.mip_end());
        let layer_start = u64::from(self.base_array_layer.max(other.base_array_layer));
        let layer_end = self.layer_end().min(other.layer_end());

        Some(Self::from_bounds(
            self.aspects & other.aspects,
            mip_start,
            mip_end,
            layer_start,
            layer_end,
        ))
    }

    /// Return the non-overlapping pieces of `self` after removing `other`.
    ///
    /// This is used by range-aware dependency analysis to stop walking older
    /// resource versions only for the subresources covered by a newer writer.
    pub fn subtract(self, other: Self) -> Vec<Self> {
        let Some(intersection) = self.intersection(other) else {
            return vec![self];
        };

        let mut result = Vec::with_capacity(5);
        let remaining_aspects = self.aspects.without(intersection.aspects);
        if !remaining_aspects.is_empty() {
            result.push(Self {
                aspects: remaining_aspects,
                ..self
            });
        }

        let self_mip_start = u64::from(self.base_mip_level);
        let self_mip_end = self.mip_end();
        let intersection_mip_start = u64::from(intersection.base_mip_level);
        let intersection_mip_end = intersection.mip_end();
        let self_layer_start = u64::from(self.base_array_layer);
        let self_layer_end = self.layer_end();
        let intersection_layer_start = u64::from(intersection.base_array_layer);
        let intersection_layer_end = intersection.layer_end();

        if self_mip_start < intersection_mip_start {
            result.push(Self::from_bounds(
                intersection.aspects,
                self_mip_start,
                intersection_mip_start,
                self_layer_start,
                self_layer_end,
            ));
        }
        if intersection_mip_end < self_mip_end {
            result.push(Self::from_bounds(
                intersection.aspects,
                intersection_mip_end,
                self_mip_end,
                self_layer_start,
                self_layer_end,
            ));
        }
        if self_layer_start < intersection_layer_start {
            result.push(Self::from_bounds(
                intersection.aspects,
                intersection_mip_start,
                intersection_mip_end,
                self_layer_start,
                intersection_layer_start,
            ));
        }
        if intersection_layer_end < self_layer_end {
            result.push(Self::from_bounds(
                intersection.aspects,
                intersection_mip_start,
                intersection_mip_end,
                intersection_layer_end,
                self_layer_end,
            ));
        }

        result
    }

    fn from_bounds(
        aspects: ImageAspects,
        mip_start: u64,
        mip_end: u64,
        layer_start: u64,
        layer_end: u64,
    ) -> Self {
        Self {
            aspects,
            base_mip_level: u32::try_from(mip_start).expect("mip start exceeds u32::MAX"),
            mip_level_count: u32::try_from(mip_end - mip_start)
                .expect("mip range exceeds u32::MAX"),
            base_array_layer: u32::try_from(layer_start)
                .expect("array-layer start exceeds u32::MAX"),
            array_layer_count: u32::try_from(layer_end - layer_start)
                .expect("array-layer range exceeds u32::MAX"),
        }
    }
}

/// Half-open byte range `[offset, offset + size)` within a buffer.
///
/// A size of [`u32::MAX`] means "all remaining bytes", matching the
/// subresource-range convention for images.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BufferByteRange {
    pub offset: u64,
    pub size: u64,
}

impl BufferByteRange {
    /// Every byte of the buffer.
    pub const WHOLE: Self = Self {
        offset: 0,
        size: u64::MAX,
    };

    pub const fn new(offset: u64, size: u64) -> Self {
        Self { offset, size }
    }

    /// A range covering all bytes from `offset` to the end of the buffer.
    pub const fn from(offset: u64) -> Self {
        Self {
            offset,
            size: u64::MAX,
        }
    }

    #[inline]
    pub const fn is_empty(self) -> bool {
        self.size == 0
    }

    /// Exclusive end of the range. A size of [`u64::MAX`] is represented as
    /// [`u64::MAX`] rather than wrapping to zero.
    #[inline]
    pub const fn end(self) -> u64 {
        match self.offset.checked_add(self.size) {
            Some(end) => end,
            None => u64::MAX,
        }
    }

    /// Whether this range covers every byte of a buffer of `len` bytes.
    #[inline]
    pub const fn covers_whole_buffer(self, len: u64) -> bool {
        self.offset == 0 && self.size >= len
    }

    #[inline]
    pub const fn overlaps(self, other: Self) -> bool {
        !self.is_empty()
            && !other.is_empty()
            && self.offset < other.end()
            && other.offset < self.end()
    }

    /// Overlapping portion of two ranges, if any.
    pub const fn intersection(self, other: Self) -> Option<Self> {
        if !self.overlaps(other) {
            return None;
        }

        let start = if self.offset > other.offset {
            self.offset
        } else {
            other.offset
        };
        let self_end = self.end();
        let other_end = other.end();
        let end = if self_end < other_end {
            self_end
        } else {
            other_end
        };

        Some(Self {
            offset: start,
            size: end - start,
        })
    }

    /// The non-overlapping pieces of `self` after removing `other`.
    ///
    /// Used by range-aware dependency analysis to stop walking older buffer
    /// versions only for the bytes a newer writer covered. Returns at most two
    /// pieces (below and above the intersection), preserving order.
    pub fn subtract(self, other: Self) -> Vec<Self> {
        let Some(intersection) = self.intersection(other) else {
            return vec![self];
        };

        let mut result = Vec::with_capacity(2);
        if self.offset < intersection.offset {
            result.push(Self {
                offset: self.offset,
                size: intersection.offset - self.offset,
            });
        }
        let self_end = self.end();
        if intersection.end() < self_end {
            result.push(Self {
                offset: intersection.end(),
                size: self_end - intersection.end(),
            });
        }
        result
    }

    /// Intersect with a buffer's length, resolving the unbounded `u64::MAX`
    /// size into a concrete range.
    pub fn clamp_to_len(self, len: u64) -> Self {
        let size = self.size.min(len.saturating_sub(self.offset));
        Self {
            offset: self.offset,
            size,
        }
    }
}

/// Whether an image access reads, writes, or updates an existing value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResourceAccessMode {
    Read,
    Write,
    ReadWrite,
}

impl ResourceAccessMode {
    #[inline]
    pub const fn reads(self) -> bool {
        matches!(self, Self::Read | Self::ReadWrite)
    }

    #[inline]
    pub const fn writes(self) -> bool {
        matches!(self, Self::Write | Self::ReadWrite)
    }
}

/// Backend-neutral image usage selected by a pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResourceAccessUsage {
    Sampled,
    ColorAttachment,
    DepthStencilAttachment,
    Storage,
    TransferSource,
    TransferDestination,
    Present,
}

/// Backend-neutral pipeline visibility for an image access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResourceAccessStage {
    VertexShader,
    FragmentShader,
    ComputeShader,
    ColorAttachmentOutput,
    DepthStencil,
    Transfer,
    Present,
    AllGraphics,
}

/// One typed image access declared by a render-graph pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImageAccess {
    pub resource: ResourceId,
    pub mode: ResourceAccessMode,
    pub usage: ResourceAccessUsage,
    pub stage: ResourceAccessStage,
    pub range: ImageSubresourceRange,
}

impl ImageAccess {
    /// Every aspect, mip, and layer of the image.
    pub const WHOLE_RESOURCE: ImageSubresourceRange =
        ImageSubresourceRange::whole(ImageAspects::ALL);

    pub const fn new(
        resource: ResourceId,
        mode: ResourceAccessMode,
        usage: ResourceAccessUsage,
        stage: ResourceAccessStage,
        range: ImageSubresourceRange,
    ) -> Self {
        Self {
            resource,
            mode,
            usage,
            stage,
            range,
        }
    }

    /// Read an image through a shader sampler.
    ///
    /// Sampling reads whichever aspect the target image carries; the graph
    /// level does not know the image format, so the default range covers
    /// every aspect. Use [`Self::with_range`] for aspect-precise reads.
    pub const fn sampled_read(resource: ResourceId) -> Self {
        Self::new(
            resource,
            ResourceAccessMode::Read,
            ResourceAccessUsage::Sampled,
            ResourceAccessStage::FragmentShader,
            Self::WHOLE_RESOURCE,
        )
    }

    pub const fn storage_write(resource: ResourceId) -> Self {
        Self::new(
            resource,
            ResourceAccessMode::Write,
            ResourceAccessUsage::Storage,
            ResourceAccessStage::AllGraphics,
            ImageSubresourceRange::WHOLE_COLOR,
        )
    }

    pub const fn storage_read_write(resource: ResourceId) -> Self {
        Self::new(
            resource,
            ResourceAccessMode::ReadWrite,
            ResourceAccessUsage::Storage,
            ResourceAccessStage::AllGraphics,
            ImageSubresourceRange::WHOLE_COLOR,
        )
    }

    /// Write a color attachment (whole color aspect).
    pub const fn color_attachment_write(resource: ResourceId) -> Self {
        Self::new(
            resource,
            ResourceAccessMode::Write,
            ResourceAccessUsage::ColorAttachment,
            ResourceAccessStage::ColorAttachmentOutput,
            ImageSubresourceRange::WHOLE_COLOR,
        )
    }

    /// Blend into or preserve a color attachment: reads the existing contents
    /// and writes the result (whole color aspect).
    pub const fn color_attachment_read_write(resource: ResourceId) -> Self {
        Self::new(
            resource,
            ResourceAccessMode::ReadWrite,
            ResourceAccessUsage::ColorAttachment,
            ResourceAccessStage::ColorAttachmentOutput,
            ImageSubresourceRange::WHOLE_COLOR,
        )
    }

    /// Read a depth-stencil attachment, e.g. sampling a shadow map.
    pub const fn depth_attachment_read(resource: ResourceId) -> Self {
        Self::new(
            resource,
            ResourceAccessMode::Read,
            ResourceAccessUsage::DepthStencilAttachment,
            ResourceAccessStage::DepthStencil,
            ImageSubresourceRange::WHOLE_DEPTH,
        )
    }

    /// Write a depth-stencil attachment (whole depth-stencil aspect).
    pub const fn depth_attachment_write(resource: ResourceId) -> Self {
        Self::new(
            resource,
            ResourceAccessMode::Write,
            ResourceAccessUsage::DepthStencilAttachment,
            ResourceAccessStage::DepthStencil,
            ImageSubresourceRange::WHOLE_DEPTH_STENCIL,
        )
    }

    /// Read as a transfer source.
    pub const fn transfer_read(resource: ResourceId) -> Self {
        Self::new(
            resource,
            ResourceAccessMode::Read,
            ResourceAccessUsage::TransferSource,
            ResourceAccessStage::Transfer,
            Self::WHOLE_RESOURCE,
        )
    }

    /// Write as a transfer destination.
    pub const fn transfer_write(resource: ResourceId) -> Self {
        Self::new(
            resource,
            ResourceAccessMode::Write,
            ResourceAccessUsage::TransferDestination,
            ResourceAccessStage::Transfer,
            Self::WHOLE_RESOURCE,
        )
    }

    /// Final present-engine read of the swapchain image.
    pub const fn present_write(resource: ResourceId) -> Self {
        Self::new(
            resource,
            ResourceAccessMode::Write,
            ResourceAccessUsage::Present,
            ResourceAccessStage::Present,
            Self::WHOLE_RESOURCE,
        )
    }

    pub const fn with_range(mut self, range: ImageSubresourceRange) -> Self {
        self.range = range;
        self
    }
}

/// String-addressed image access resolved by [`super::FrameGraphBuilder`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NamedImageAccess {
    pub resource: String,
    pub mode: ResourceAccessMode,
    pub usage: ResourceAccessUsage,
    pub stage: ResourceAccessStage,
    pub range: ImageSubresourceRange,
}

impl NamedImageAccess {
    pub(crate) fn resolve(&self, resource: ResourceId) -> ImageAccess {
        ImageAccess::new(resource, self.mode, self.usage, self.stage, self.range)
    }
}

/// Backend-neutral buffer usage selected by a pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BufferUsage {
    /// Uniform buffer read by a shader.
    Uniform,
    /// Storage buffer read or written by a shader.
    Storage,
    /// Vertex attribute or index data consumed by the input assembler.
    Vertex,
    /// Index data consumed by the input assembler.
    Index,
    /// Dispatch dimensions or draw counts read indirectly by the GPU.
    Indirect,
    /// Read as a transfer source (copy from).
    TransferSource,
    /// Written as a transfer destination (copy to, fill).
    TransferDestination,
    /// Read back to the host.
    Readback,
}

/// One typed buffer access declared by a render-graph pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BufferAccess {
    pub resource: ResourceId,
    pub mode: ResourceAccessMode,
    pub usage: BufferUsage,
    pub stage: ResourceAccessStage,
    /// Byte range the access touches.
    pub range: BufferByteRange,
}

impl BufferAccess {
    pub const fn new(
        resource: ResourceId,
        mode: ResourceAccessMode,
        usage: BufferUsage,
        stage: ResourceAccessStage,
        range: BufferByteRange,
    ) -> Self {
        Self {
            resource,
            mode,
            usage,
            stage,
            range,
        }
    }

    pub const fn uniform_read(resource: ResourceId) -> Self {
        Self::new(
            resource,
            ResourceAccessMode::Read,
            BufferUsage::Uniform,
            ResourceAccessStage::AllGraphics,
            BufferByteRange::WHOLE,
        )
    }

    pub const fn storage_read(resource: ResourceId) -> Self {
        Self::new(
            resource,
            ResourceAccessMode::Read,
            BufferUsage::Storage,
            ResourceAccessStage::ComputeShader,
            BufferByteRange::WHOLE,
        )
    }

    pub const fn storage_write(resource: ResourceId) -> Self {
        Self::new(
            resource,
            ResourceAccessMode::Write,
            BufferUsage::Storage,
            ResourceAccessStage::ComputeShader,
            BufferByteRange::WHOLE,
        )
    }

    pub const fn storage_read_write(resource: ResourceId) -> Self {
        Self::new(
            resource,
            ResourceAccessMode::ReadWrite,
            BufferUsage::Storage,
            ResourceAccessStage::ComputeShader,
            BufferByteRange::WHOLE,
        )
    }

    pub const fn vertex_read(resource: ResourceId) -> Self {
        Self::new(
            resource,
            ResourceAccessMode::Read,
            BufferUsage::Vertex,
            ResourceAccessStage::VertexShader,
            BufferByteRange::WHOLE,
        )
    }

    pub const fn index_read(resource: ResourceId) -> Self {
        Self::new(
            resource,
            ResourceAccessMode::Read,
            BufferUsage::Index,
            ResourceAccessStage::VertexShader,
            BufferByteRange::WHOLE,
        )
    }

    pub const fn indirect_read(resource: ResourceId) -> Self {
        Self::new(
            resource,
            ResourceAccessMode::Read,
            BufferUsage::Indirect,
            ResourceAccessStage::AllGraphics,
            BufferByteRange::WHOLE,
        )
    }

    pub const fn transfer_read(resource: ResourceId) -> Self {
        Self::new(
            resource,
            ResourceAccessMode::Read,
            BufferUsage::TransferSource,
            ResourceAccessStage::Transfer,
            BufferByteRange::WHOLE,
        )
    }

    pub const fn transfer_write(resource: ResourceId) -> Self {
        Self::new(
            resource,
            ResourceAccessMode::Write,
            BufferUsage::TransferDestination,
            ResourceAccessStage::Transfer,
            BufferByteRange::WHOLE,
        )
    }

    pub const fn readback_read(resource: ResourceId) -> Self {
        Self::new(
            resource,
            ResourceAccessMode::Read,
            BufferUsage::Readback,
            ResourceAccessStage::Transfer,
            BufferByteRange::WHOLE,
        )
    }

    pub const fn with_range(mut self, range: BufferByteRange) -> Self {
        self.range = range;
        self
    }
}

/// String-addressed buffer access resolved by [`super::FrameGraphBuilder`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NamedBufferAccess {
    pub resource: String,
    pub mode: ResourceAccessMode,
    pub usage: BufferUsage,
    pub stage: ResourceAccessStage,
    pub range: BufferByteRange,
}

impl NamedBufferAccess {
    pub(crate) fn resolve(&self, resource: ResourceId) -> BufferAccess {
        BufferAccess::new(resource, self.mode, self.usage, self.stage, self.range)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disjoint_mips_and_layers_do_not_overlap() {
        let mip_zero = ImageSubresourceRange::new(ImageAspects::COLOR, 0, 1, 0, 1);
        let mip_one = ImageSubresourceRange::new(ImageAspects::COLOR, 1, 1, 0, 1);
        let layer_one = ImageSubresourceRange::new(ImageAspects::COLOR, 0, 1, 1, 1);

        assert!(!mip_zero.overlaps(mip_one));
        assert!(!mip_zero.overlaps(layer_one));
    }

    #[test]
    fn aspects_are_independent() {
        let depth = ImageSubresourceRange::new(ImageAspects::DEPTH, 0, 1, 0, 1);
        let stencil = ImageSubresourceRange::new(ImageAspects::STENCIL, 0, 1, 0, 1);
        let both = ImageSubresourceRange::new(ImageAspects::DEPTH_STENCIL, 0, 1, 0, 1);

        assert!(!depth.overlaps(stencil));
        assert!(depth.overlaps(both));
        assert!(stencil.overlaps(both));
    }

    #[test]
    fn subtraction_preserves_every_non_overlapping_piece() {
        let whole = ImageSubresourceRange::new(ImageAspects::DEPTH_STENCIL, 0, 4, 0, 4);
        let center = ImageSubresourceRange::new(ImageAspects::DEPTH, 1, 2, 1, 2);
        let pieces = whole.subtract(center);

        assert_eq!(pieces.len(), 5);
        assert!(pieces.iter().all(|piece| !piece.overlaps(center)));
        assert!(
            pieces
                .iter()
                .any(|piece| piece.aspects == ImageAspects::STENCIL)
        );
    }
}
