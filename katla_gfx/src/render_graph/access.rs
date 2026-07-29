//! Backend-neutral image access declarations.
//!
//! Accesses describe what a pass does to an image, where in the pipeline the
//! access occurs, and which subresources participate. The compiler can retain
//! this vocabulary across Vulkan and Metal instead of reconstructing intent from
//! coarse read/write lists or native image layouts.

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

/// Whether an image access reads, writes, or updates an existing value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ImageAccessMode {
    Read,
    Write,
    ReadWrite,
}

impl ImageAccessMode {
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
pub enum ImageUsage {
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
pub enum ImagePipelineStage {
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
    pub mode: ImageAccessMode,
    pub usage: ImageUsage,
    pub stage: ImagePipelineStage,
    pub range: ImageSubresourceRange,
}

impl ImageAccess {
    pub const fn new(
        resource: ResourceId,
        mode: ImageAccessMode,
        usage: ImageUsage,
        stage: ImagePipelineStage,
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

    pub const fn sampled_read(resource: ResourceId) -> Self {
        Self::new(
            resource,
            ImageAccessMode::Read,
            ImageUsage::Sampled,
            ImagePipelineStage::FragmentShader,
            ImageSubresourceRange::WHOLE_COLOR,
        )
    }

    pub const fn storage_write(resource: ResourceId) -> Self {
        Self::new(
            resource,
            ImageAccessMode::Write,
            ImageUsage::Storage,
            ImagePipelineStage::AllGraphics,
            ImageSubresourceRange::WHOLE_COLOR,
        )
    }

    pub const fn storage_read_write(resource: ResourceId) -> Self {
        Self::new(
            resource,
            ImageAccessMode::ReadWrite,
            ImageUsage::Storage,
            ImagePipelineStage::AllGraphics,
            ImageSubresourceRange::WHOLE_COLOR,
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
    pub mode: ImageAccessMode,
    pub usage: ImageUsage,
    pub stage: ImagePipelineStage,
    pub range: ImageSubresourceRange,
}

impl NamedImageAccess {
    pub(crate) fn resolve(&self, resource: ResourceId) -> ImageAccess {
        ImageAccess::new(resource, self.mode, self.usage, self.stage, self.range)
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
        assert!(pieces
            .iter()
            .any(|piece| piece.aspects == ImageAspects::STENCIL));
    }
}
