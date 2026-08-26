//! Pass types for render graph execution.

use std::collections::BTreeSet;

use crate::render_graph::ViewportRect;
use crate::render_graph::access::{
    ImageAccess, ImageAccessMode, ImagePipelineStage, ImageSubresourceRange, ImageUsage,
};
use crate::render_graph::handles::ResourceId;
use crate::render_graph::resource::GraphResourceHandle;
use crate::render_pass::{ClearValue, LoadOp, StoreOp};
use crate::texture::ImageFormat;

/// Callback for custom compute dispatch logic (Vulkan-specific).
///
/// Receives mutable access to the frame (and thus the renderer),
/// the command buffer, and the pipeline handle assigned to the pass.
pub type ComputeFn = Box<
    dyn for<'a> Fn(
        &mut super::frame::Frame<'a, crate::renderer::VulkanRenderer>,
        &crate::vulkan::commandbuffer::CommandBuffer,
        crate::handle::PipelineHandle,
    ) -> Result<(), super::error::RenderGraphError>,
>;

/// Type of render pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PassType {
    /// Graphics pass (rendering to attachments).
    #[default]
    Graphics,
    /// Compute pass (GPU compute work).
    Compute,
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
    /// Pass type (graphics, compute, transfer).
    pub pass_type: PassType,
    /// Optional pipeline handle (for fullscreen/compute passes).
    pub pipeline: Option<crate::handle::PipelineHandle>,
    /// Optional tonemap parameters (for HDR tonemapping passes).
    pub tonemap_params: Option<crate::render_graph::passes::TonemapParams>,
    /// Optional overlay parameters (for wallhack overlay passes).
    pub overlay_params: Option<crate::render_graph::passes::OverlayParams>,
    /// Optional material handle (for geometry passes).
    pub material: Option<crate::handle::MaterialHandle>,
    /// Output color format (for material format inference).
    pub output_format: Option<crate::texture::ImageFormat>,
    /// Color attachment load/store ops for each write target.
    pub color_attachments: Vec<(ResourceId, ImageFormat, LoadOp, StoreOp, ClearValue)>,
    /// Whether this pass uses depth testing (default true for graphics passes).
    pub uses_depth: bool,
    /// Depth attachment load/store/clear configuration.
    /// When None, defaults to (Clear, Store, depth=0.0) for reverse-Z.
    pub depth_attachment: Option<(LoadOp, StoreOp, ClearValue)>,
    /// Compositing pass data: viewport textures with rectangles.
    /// Set for CompositePass, None for other pass types.
    pub compositing_viewports: Option<Vec<(GraphResourceHandle, ViewportRect)>>,
    /// Optional compute dispatch callback for compute passes (Vulkan-specific).
    pub compute_fn: Option<ComputeFn>,

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
            pass_type,
            pipeline: None,
            tonemap_params: None,
            overlay_params: None,
            material: None,
            output_format: None,
            color_attachments: Vec::new(),
            uses_depth: true,
            depth_attachment: None,
            compositing_viewports: None,
            compute_fn: None,
            kind: None,
            side_effect: false,
        }
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
        self.reads = self
            .image_accesses
            .iter()
            .filter(|access| access.mode.reads())
            .map(|access| access.resource)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        self.writes = self
            .image_accesses
            .iter()
            .filter(|access| access.mode.writes())
            .map(|access| access.resource)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
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

    /// Refine compatibility accesses using the pass semantic and attachment operations.
    pub(crate) fn refine_inferred_image_accesses(&mut self) {
        for access in &mut self.image_accesses {
            let color_attachment = self
                .color_attachments
                .iter()
                .find(|(resource, ..)| *resource == access.resource);

            if let Some((_, _, load_op, _, _)) = color_attachment {
                access.mode = if *load_op == LoadOp::Load || access.mode.reads() {
                    ImageAccessMode::ReadWrite
                } else {
                    ImageAccessMode::Write
                };
                access.usage = ImageUsage::ColorAttachment;
                access.stage = ImagePipelineStage::ColorAttachmentOutput;
                access.range = ImageSubresourceRange::WHOLE_COLOR;
                continue;
            }

            if self.pass_type == PassType::Graphics && access.mode.writes() {
                if self.kind == Some(PassKind::Shadow) {
                    access.usage = ImageUsage::DepthStencilAttachment;
                    access.stage = ImagePipelineStage::DepthStencil;
                    access.range = ImageSubresourceRange::WHOLE_DEPTH;
                } else {
                    access.usage = ImageUsage::ColorAttachment;
                    access.stage = ImagePipelineStage::ColorAttachmentOutput;
                    access.range = ImageSubresourceRange::WHOLE_COLOR;
                }
                continue;
            }

            if self.kind == Some(PassKind::ObjectId) && access.mode.reads() {
                access.usage = ImageUsage::DepthStencilAttachment;
                access.stage = ImagePipelineStage::DepthStencil;
                access.range = ImageSubresourceRange::WHOLE_DEPTH_STENCIL;
            }
        }

        self.image_accesses.sort();
        self.synchronize_resource_sets();
    }

    /// Set the pipeline for this pass.
    pub fn with_pipeline(mut self, pipeline: crate::handle::PipelineHandle) -> Self {
        self.pipeline = Some(pipeline);
        self
    }

    /// Attach a compute dispatch callback to this pass (Vulkan-specific).
    pub fn with_compute_fn(
        mut self,
        f: impl Fn(
            &mut super::frame::Frame<'_, crate::renderer::VulkanRenderer>,
            &crate::vulkan::commandbuffer::CommandBuffer,
            crate::handle::PipelineHandle,
        ) -> Result<(), super::error::RenderGraphError>
        + 'static,
    ) -> Self {
        self.compute_fn = Some(Box::new(f));
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
    fn test_pass_desc_with_pipeline() {
        let desc = PassDesc::new("test", PassType::Graphics, vec![], vec![rid(1)])
            .with_pipeline(crate::handle::PipelineHandle::new(42));

        assert_eq!(desc.pipeline.unwrap().index(), 42);
    }

    #[test]
    fn test_pass_desc_defaults() {
        let desc = PassDesc::new("test", PassType::Graphics, vec![rid(1)], vec![rid(2)]);
        assert!(desc.pipeline.is_none());
        assert!(desc.tonemap_params.is_none());
        assert!(desc.material.is_none());
        assert!(desc.output_format.is_none());
        assert_eq!(desc.image_accesses.len(), 2);
        assert!(desc.image_accesses[0].mode.reads());
        assert!(desc.image_accesses[1].mode.writes());
        assert!(desc.color_attachments.is_empty());
        assert!(desc.depth_attachment.is_none());
        assert!(desc.compositing_viewports.is_none());
        assert!(desc.compute_fn.is_none());
        assert!(desc.kind.is_none());
        assert!(!desc.side_effect);
        assert!(desc.uses_depth);
    }

    #[test]
    fn explicit_accesses_drive_compatibility_sets() {
        let mut desc = PassDesc::new("test", PassType::Graphics, vec![rid(1)], vec![rid(2)]);
        desc.set_image_accesses(vec![ImageAccess::new(
            rid(3),
            ImageAccessMode::ReadWrite,
            ImageUsage::Storage,
            ImagePipelineStage::FragmentShader,
            ImageSubresourceRange::new(ImageAspects::COLOR, 2, 1, 0, 1),
        )]);

        assert_eq!(desc.reads, vec![rid(3)]);
        assert_eq!(desc.writes, vec![rid(3)]);
        assert_eq!(desc.image_accesses[0].range.base_mip_level, 2);
    }

    #[test]
    fn loaded_color_attachment_is_a_read_write_access() {
        let mut desc = PassDesc::new("blend", PassType::Graphics, Vec::new(), vec![rid(1)]);
        desc.color_attachments.push((
            rid(1),
            ImageFormat::R8G8B8A8Unorm,
            LoadOp::Load,
            StoreOp::Store,
            ClearValue::OPAQUE_BLACK,
        ));
        desc.refine_inferred_image_accesses();

        assert_eq!(desc.image_accesses.len(), 1);
        assert_eq!(desc.image_accesses[0].mode, ImageAccessMode::ReadWrite);
        assert_eq!(desc.image_accesses[0].usage, ImageUsage::ColorAttachment);
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
