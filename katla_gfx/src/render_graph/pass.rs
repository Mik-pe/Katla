//! Pass types for render graph execution.

use std::collections::BTreeSet;

use crate::render_graph::ViewportRect;
use crate::render_graph::access::{
    BufferAccess, ImageAccess, ImageSubresourceRange, ResourceAccessMode, ResourceAccessStage,
    ResourceAccessUsage,
};
use crate::render_graph::handles::ResourceId;
use crate::render_graph::resource::GraphResourceHandle;
use crate::render_pass::{AttachmentOps, DepthStencilAttachmentOps, LoadOp};

/// Type of render pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PassType {
    /// Graphics pass (rendering to attachments).
    #[default]
    Graphics,
    /// Compute pass (GPU compute work).
    Compute,
    /// Buffer and image transfer work.
    Transfer,
}

/// Semantic kind of a render pass, used for dispatch routing.
///
/// Set at build time by each pass template. Eliminates structural heuristics
/// (checking `material.is_none() && pipeline.is_none()`) in the execution loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassKind {
    /// Depth prepass — renders depth and optional object IDs.
    DepthPrepass,
    /// Shadow mapping — renders shadow depth into atlas.
    Shadow,
    /// Geometry — renders 3D scene geometry with material.
    Geometry,
    /// Object-ID — renders stable object identifiers for GPU picking.
    ObjectId,
    /// Particles — renders GPU particles with alpha blending.
    Particles,
    /// Outline — stencil-based selection highlight.
    Outline,
    /// Stencil indicator — writes R8 mask where stencil==2.
    StencilIndicator,
    /// Fullscreen — post-processing (tonemap, etc.) with pipeline.
    Fullscreen,
    /// Compositing — multi-viewport compositing.
    Compositing,
    /// UI overlay — composites UI commands over the rendered frame.
    Ui,
}

/// Internal pass descriptor.
pub struct PassDesc {
    /// Human-readable name for debugging.
    pub name: String,
    /// Resources this pass reads from.
    pub reads: Vec<ResourceId>,
    /// Resources this pass writes to.
    pub writes: Vec<ResourceId>,
    /// Typed image accesses. These preserve usage, pipeline visibility, and subresources.
    pub image_accesses: Vec<ImageAccess>,
    /// Typed buffer accesses. These preserve usage, pipeline visibility, and
    /// byte ranges, and drive the same range-aware hazard analysis as images.
    pub buffer_accesses: Vec<BufferAccess>,
    /// Pass type (graphics, compute, transfer).
    pub pass_type: PassType,
    /// Optional material handle (for geometry passes).
    pub material: Option<crate::handle::MaterialHandle>,
    /// Output color format (for material format inference).
    pub output_format: Option<crate::texture::ImageFormat>,
    /// Declared load/store/clear operations per color target.
    ///
    /// The authoritative attachment contract for a pass: execution resolves
    /// targets and translates exactly these operations. Populated at graph
    /// build from the pass templates and validated before compilation.
    pub color_attachments: Vec<(ResourceId, AttachmentOps)>,
    /// Whether this pass uses depth testing (default true for graphics passes).
    pub uses_depth: bool,
    /// Explicit graph depth/stencil target.
    pub depth_target: Option<ResourceId>,
    /// Depth and stencil attachment operations for this pass's depth target.
    ///
    /// Normalized to a canonical default at graph build when a graphics pass
    /// uses depth but declares nothing, so execution always consumes a
    /// declaration.
    pub depth_attachment: Option<DepthStencilAttachmentOps>,
    /// Compositing pass data: viewport textures with rectangles.
    /// Set for CompositePass, None for other pass types.
    pub compositing_viewports: Option<Vec<(GraphResourceHandle, ViewportRect)>>,
    /// Backend-neutral commands executed by a compute or transfer pass.
    pub commands: Vec<super::compute::ComputeCommand>,

    /// Explicit reflected graphics inputs owned by the application.
    pub bindings: crate::renderer::frame_bindings::PassBindings,

    /// Semantic kind of this pass, used for dispatch routing.
    /// Set at build time by each pass template.
    pub kind: Option<PassKind>,
    /// Whether this pass has an externally observable effect that is not represented
    /// by a graph resource write. Side-effect passes are roots for liveness analysis.
    pub side_effect: bool,
}

impl PassDesc {
    /// Create a new pass descriptor.
    pub fn new(
        name: impl Into<String>,
        pass_type: PassType,
        reads: Vec<ResourceId>,
        writes: Vec<ResourceId>,
    ) -> Self {
        let image_accesses = Self::default_image_accesses(&reads, &writes);
        Self {
            name: name.into(),
            reads,
            writes,
            image_accesses,
            buffer_accesses: Vec::new(),
            pass_type,
            material: None,
            output_format: None,
            color_attachments: Vec::new(),
            uses_depth: false,
            depth_target: None,
            depth_attachment: None,
            compositing_viewports: None,
            commands: Vec::new(),
            bindings: Default::default(),
            kind: None,
            side_effect: false,
        }
    }

    /// Supply explicit pipeline and resource inputs without changing access declarations.
    pub fn with_bindings(
        mut self,
        bindings: crate::renderer::frame_bindings::PassBindings,
    ) -> Self {
        self.bindings = bindings;
        self
    }

    fn default_image_accesses(reads: &[ResourceId], writes: &[ResourceId]) -> Vec<ImageAccess> {
        let resources = reads.iter().chain(writes).copied().collect::<BTreeSet<_>>();

        resources
            .into_iter()
            .map(
                |resource| match (reads.contains(&resource), writes.contains(&resource)) {
                    (true, true) => ImageAccess::storage_read_write(resource),
                    (true, false) => ImageAccess::sampled_read(resource),
                    (false, true) => ImageAccess::storage_write(resource),
                    (false, false) => unreachable!("resource came from the read/write union"),
                },
            )
            .collect()
    }

    fn synchronize_resource_sets(&mut self) {
        let reads = self
            .image_accesses
            .iter()
            .filter(|access| access.mode.reads())
            .map(|access| access.resource)
            .chain(
                self.buffer_accesses
                    .iter()
                    .filter(|access| access.mode.reads())
                    .map(|access| access.resource),
            );
        self.reads = reads.collect::<BTreeSet<_>>().into_iter().collect();

        let writes = self
            .image_accesses
            .iter()
            .filter(|access| access.mode.writes())
            .map(|access| access.resource)
            .chain(
                self.buffer_accesses
                    .iter()
                    .filter(|access| access.mode.writes())
                    .map(|access| access.resource),
            );
        self.writes = writes.collect::<BTreeSet<_>>().into_iter().collect();
    }

    /// Replace the pass image-access contract and synchronize coarse compatibility sets.
    pub fn set_image_accesses(&mut self, accesses: Vec<ImageAccess>) {
        self.image_accesses = accesses;
        self.synchronize_resource_sets();
    }

    /// Replace inferred accesses with an explicit typed image-access contract.
    pub fn with_image_accesses(mut self, accesses: impl IntoIterator<Item = ImageAccess>) -> Self {
        self.set_image_accesses(accesses.into_iter().collect());
        self
    }

    /// Set the pass's typed buffer-access contract.
    ///
    /// Buffer accesses are additive to the coarse read/write sets: the builder
    /// that declares them by name keeps both in sync, so this only stores them.
    pub fn with_buffer_accesses(
        mut self,
        accesses: impl IntoIterator<Item = BufferAccess>,
    ) -> Self {
        self.set_buffer_accesses(accesses.into_iter().collect());
        self
    }

    /// Replace the typed buffer-access contract and synchronize compatibility sets.
    pub fn set_buffer_accesses(&mut self, accesses: Vec<BufferAccess>) {
        let buffer_resources = accesses
            .iter()
            .map(|access| access.resource)
            .collect::<BTreeSet<_>>();
        self.image_accesses
            .retain(|access| !buffer_resources.contains(&access.resource));
        self.buffer_accesses = accesses;
        self.synchronize_resource_sets();
    }

    /// Refine compatibility accesses using the pass semantic and attachment operations.
    pub(crate) fn refine_inferred_image_accesses(&mut self) {
        for access in &mut self.image_accesses {
            let color_attachment = self
                .color_attachments
                .iter()
                .find(|(resource, ..)| *resource == access.resource);

            if let Some((_, ops)) = color_attachment {
                access.mode = if ops.load == LoadOp::Load || access.mode.reads() {
                    ResourceAccessMode::ReadWrite
                } else {
                    ResourceAccessMode::Write
                };
                access.usage = ResourceAccessUsage::ColorAttachment;
                access.stage = ResourceAccessStage::ColorAttachmentOutput;
                access.range = ImageSubresourceRange::WHOLE_COLOR;
                continue;
            }

            if self.pass_type == PassType::Graphics && access.mode.writes() {
                if self.kind == Some(PassKind::Shadow) {
                    access.usage = ResourceAccessUsage::DepthStencilAttachment;
                    access.stage = ResourceAccessStage::DepthStencil;
                    access.range = ImageSubresourceRange::WHOLE_DEPTH;
                } else {
                    access.usage = ResourceAccessUsage::ColorAttachment;
                    access.stage = ResourceAccessStage::ColorAttachmentOutput;
                    access.range = ImageSubresourceRange::WHOLE_COLOR;
                }
                continue;
            }

            if self.kind == Some(PassKind::ObjectId) && access.mode.reads() {
                access.usage = ResourceAccessUsage::DepthStencilAttachment;
                access.stage = ResourceAccessStage::DepthStencil;
                access.range = ImageSubresourceRange::WHOLE_DEPTH_STENCIL;
            }
        }

        self.image_accesses.sort();
        self.synchronize_resource_sets();
    }

    /// Record backend-neutral compute and transfer commands.
    pub fn with_commands(
        mut self,
        commands: impl IntoIterator<Item = super::compute::ComputeCommand>,
    ) -> Self {
        self.commands = commands.into_iter().collect();
        self.uses_depth = false;
        self
    }

    #[inline]
    pub fn writes_to(&self, id: ResourceId) -> bool {
        self.writes.contains(&id)
    }

    /// Mark this pass as an externally observable side effect.
    ///
    /// Prefer declaring resource outputs whenever possible. Use this only for work
    /// such as timestamps, callbacks, or backend-owned state that cannot yet be
    /// represented as a graph resource.
    pub fn with_side_effect(mut self) -> Self {
        self.side_effect = true;
        self
    }

    /// Check if this pass reads from a specific resource.
    #[inline]
    pub fn reads_from(&self, id: ResourceId) -> bool {
        self.reads.contains(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_graph::access::ImageAspects;

    fn rid(n: u32) -> ResourceId {
        ResourceId(n)
    }

    #[test]
    fn test_pass_desc_defaults() {
        let desc = PassDesc::new("test", PassType::Graphics, vec![rid(1)], vec![rid(2)]);
        assert!(desc.material.is_none());
        assert!(desc.output_format.is_none());
        assert_eq!(desc.image_accesses.len(), 2);
        assert!(desc.image_accesses[0].mode.reads());
        assert!(desc.image_accesses[1].mode.writes());
        assert!(desc.color_attachments.is_empty());
        assert!(desc.depth_attachment.is_none());
        assert!(desc.compositing_viewports.is_none());
        assert!(desc.commands.is_empty());
        assert!(desc.kind.is_none());
        assert!(!desc.side_effect);
        assert!(!desc.uses_depth);
    }

    #[test]
    fn explicit_accesses_drive_compatibility_sets() {
        let mut desc = PassDesc::new("test", PassType::Graphics, vec![rid(1)], vec![rid(2)]);
        desc.set_image_accesses(vec![ImageAccess::new(
            rid(3),
            ResourceAccessMode::ReadWrite,
            ResourceAccessUsage::Storage,
            ResourceAccessStage::FragmentShader,
            ImageSubresourceRange::new(ImageAspects::COLOR, 2, 1, 0, 1),
        )]);

        assert_eq!(desc.reads, vec![rid(3)]);
        assert_eq!(desc.writes, vec![rid(3)]);
        assert_eq!(desc.image_accesses[0].range.base_mip_level, 2);
    }

    #[test]
    fn loaded_color_attachment_is_a_read_write_access() {
        let mut desc = PassDesc::new("blend", PassType::Graphics, Vec::new(), vec![rid(1)]);
        desc.color_attachments
            .push((rid(1), crate::render_pass::AttachmentOps::load()));
        desc.refine_inferred_image_accesses();

        assert_eq!(desc.image_accesses.len(), 1);
        assert_eq!(desc.image_accesses[0].mode, ResourceAccessMode::ReadWrite);
        assert_eq!(
            desc.image_accesses[0].usage,
            ResourceAccessUsage::ColorAttachment
        );
        assert!(desc.reads_from(rid(1)));
        assert!(desc.writes_to(rid(1)));
    }

    #[test]
    fn test_pass_desc_writes_to_reads_from() {
        let desc = PassDesc::new("test", PassType::Graphics, vec![rid(1)], vec![rid(2)]);
        assert!(desc.reads_from(rid(1)));
        assert!(!desc.reads_from(rid(2)));
        assert_eq!(desc.image_accesses.len(), 2);
        assert!(desc.writes_to(rid(2)));
        assert!(!desc.writes_to(rid(1)));
    }
}
