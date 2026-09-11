//! Vulkan realization of the compiled synchronization plan.
//!
//! Each backend-neutral sync operation translates to one synchronization2
//! image barrier whose stage, access, layout, and subresource range come from
//! the typed access states — never from layout-pair inference. Only graph
//! transient textures are realized here: the imported backbuffer keeps its
//! renderer-local acquire/present barriers until backbuffer contract
//! consumption lands, and the backend-owned depth texture is not a graph
//! resource yet, so consecutive depth-using passes keep the explicit
//! render-pass-instance boundary barrier.

use crate::barrier::ImageBarrier;
use crate::render_graph::access::{ImageAccessMode, ImageAspects, ImagePipelineStage, ImageUsage};
use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame::Frame;
use crate::render_graph::{ImageSyncOp, ImageSyncState};
use crate::renderer::VulkanRenderer;
use crate::sync::{
    AccessFlags2, DependencyInfo, ImageMemoryBarrier2, PipelineStage2Flags, VkImage,
};
use crate::vulkan::commandbuffer::CommandBuffer;
use ash::vk;

impl Frame<'_, VulkanRenderer> {
    /// Insert the compiled synchronization operations preceding one pass.
    pub(super) fn insert_sync_barriers(
        &mut self,
        cmd: &CommandBuffer,
        pass_index: usize,
    ) -> Result<(), RenderGraphError> {
        let device = &self.renderer.context.device;
        let cmd_vk = cmd.vk_command_buffer();

        // Backend-owned depth: not a graph resource yet. The Vulkan spec
        // requires a barrier between render-pass instances sharing an
        // attachment even when the layout does not change.
        if self
            .graph
            .pass(pass_index)
            .is_some_and(|pass| pass.uses_depth)
            && self.depth_buffer_written
        {
            let frame_idx = self.current_frame();
            if let Some(depth_texture) = self
                .renderer
                .frame_context
                .depth_render_textures
                .get(frame_idx)
            {
                ImageBarrier::depth_render_pass_sync(&cmd_vk, device, depth_texture.image.vk());
            }
        }

        let ops = self.graph.image_sync_ops(pass_index);
        if ops.is_empty() {
            return Ok(());
        }
        let Some(pass) = self.graph.pass(pass_index) else {
            return Ok(());
        };
        let frame_idx = self.current_frame();

        let mut barriers = Vec::with_capacity(ops.len());
        for op in ops {
            let Some(transient) = self.graph.transient_texture_by_id(op.resource, frame_idx) else {
                log::debug!(
                    "[SYNC] Pass '{}' op on non-transient '{}': realized by the importer",
                    pass.name,
                    self.graph.resource_name(op.resource).unwrap_or("?")
                );
                continue;
            };

            let barrier = sync_op_barrier(op, transient);
            if let Some(barrier) = barrier {
                barriers.push(barrier);
            }
        }

        if barriers.is_empty() {
            return Ok(());
        }

        let mut dependency = DependencyInfo::new();
        for barrier in barriers {
            dependency = dependency.add_image_barrier(barrier);
        }
        dependency.build(|dependency| unsafe {
            device.cmd_pipeline_barrier2(cmd_vk, dependency);
        });

        Ok(())
    }

    /// Insert the frame-end operations satisfying imported final-state
    /// contracts, after the last live pass.
    ///
    /// Only operations resolving to graph transients realize here; imported
    /// images (including the backbuffer) are realized by their importer.
    pub(super) fn insert_final_sync_barriers(
        &mut self,
        cmd: &CommandBuffer,
    ) -> Result<(), RenderGraphError> {
        let ops = self.graph.final_image_sync_ops();
        if ops.is_empty() {
            return Ok(());
        }

        let device = &self.renderer.context.device;
        let cmd_vk = cmd.vk_command_buffer();
        let frame_idx = self.current_frame();

        let mut barriers = Vec::with_capacity(ops.len());
        for op in ops {
            let Some(transient) = self.graph.transient_texture_by_id(op.resource, frame_idx) else {
                log::debug!(
                    "[SYNC] Frame-end op on non-transient '{}': realized by the importer",
                    self.graph.resource_name(op.resource).unwrap_or("?")
                );
                continue;
            };

            if let Some(barrier) = sync_op_barrier(op, transient) {
                barriers.push(barrier);
            }
        }

        if barriers.is_empty() {
            return Ok(());
        }

        let mut dependency = DependencyInfo::new();
        for barrier in barriers {
            dependency = dependency.add_image_barrier(barrier);
        }
        dependency.build(|dependency| unsafe {
            device.cmd_pipeline_barrier2(cmd_vk, dependency);
        });

        Ok(())
    }
}

/// Translate one sync operation into a synchronization2 image barrier.
///
/// The tracked layout is ground truth: freshly created textures sit in
/// `UNDEFINED`, and steady state follows the compiled plan. Bootstrap
/// operations (undefined -> target) coalesce away once the tracked layout
/// already satisfies the operation; hazard operations (same state before and
/// after) always encode because render-pass instances may overlap without
/// them. Returns `None` when nothing must be encoded, or when the operation's
/// aspects do not intersect the image's real aspects.
fn sync_op_barrier(
    op: &ImageSyncOp,
    transient: &crate::render_graph::TransientTexture,
) -> Option<ImageMemoryBarrier2> {
    let texture_aspects = if transient.format == vk::Format::D32_SFLOAT {
        ImageAspects::DEPTH
    } else {
        ImageAspects::COLOR
    };
    let aspects = op.range.aspects & texture_aspects;
    if aspects.is_empty() {
        return None;
    }

    let needed_layout = state_layout(op.after);
    let tracked_layout = transient.current_layout();
    if tracked_layout == needed_layout && op.before != op.after {
        // The steady-state layout already satisfies this operation; only a
        // freshly created texture needed it.
        return None;
    }

    let (dst_stage, dst_access) = state_masks(op.after);
    let (src_stage, src_access) =
        if tracked_layout == vk::ImageLayout::UNDEFINED || op.before == ImageSyncState::Undefined {
            // Contents are not observable through an undefined source layout.
            (dst_stage, AccessFlags2::NONE)
        } else {
            state_masks(op.before)
        };

    let barrier = ImageMemoryBarrier2::new(VkImage::new(transient.image))
        .src_stage(src_stage)
        .dst_stage(dst_stage)
        .src_access(src_access)
        .dst_access(dst_access)
        .old_layout(tracked_layout)
        .new_layout(needed_layout)
        .subresource_range(vk_subresource_range(aspects, op));

    transient.set_layout(needed_layout);
    Some(barrier)
}

fn vk_subresource_range(aspects: ImageAspects, op: &ImageSyncOp) -> vk::ImageSubresourceRange {
    let mut aspect_mask = vk::ImageAspectFlags::empty();
    if aspects.contains(ImageAspects::COLOR) {
        aspect_mask |= vk::ImageAspectFlags::COLOR;
    }
    if aspects.contains(ImageAspects::DEPTH) {
        aspect_mask |= vk::ImageAspectFlags::DEPTH;
    }
    if aspects.contains(ImageAspects::STENCIL) {
        aspect_mask |= vk::ImageAspectFlags::STENCIL;
    }

    vk::ImageSubresourceRange {
        aspect_mask,
        base_mip_level: op.range.base_mip_level,
        level_count: op.range.mip_level_count,
        base_array_layer: op.range.base_array_layer,
        layer_count: op.range.array_layer_count,
    }
}

/// Layout a sync state's usage maps to.
fn state_layout(state: ImageSyncState) -> vk::ImageLayout {
    match state {
        ImageSyncState::Undefined => vk::ImageLayout::UNDEFINED,
        ImageSyncState::Access { usage, .. } => match usage {
            ImageUsage::Sampled => vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            ImageUsage::ColorAttachment => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            ImageUsage::DepthStencilAttachment => vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            ImageUsage::Storage => vk::ImageLayout::GENERAL,
            ImageUsage::TransferSource => vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            ImageUsage::TransferDestination => vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            ImageUsage::Present => vk::ImageLayout::PRESENT_SRC_KHR,
        },
    }
}

/// Pipeline stage and access mask one sync state's accesses run at.
fn state_masks(state: ImageSyncState) -> (PipelineStage2Flags, AccessFlags2) {
    match state {
        ImageSyncState::Undefined => (PipelineStage2Flags::empty(), AccessFlags2::NONE),
        ImageSyncState::Access { usage, stage, mode } => {
            (stage_mask(stage), usage_access_mask(usage, mode))
        }
    }
}

fn stage_mask(stage: ImagePipelineStage) -> PipelineStage2Flags {
    match stage {
        ImagePipelineStage::VertexShader => PipelineStage2Flags::VERTEX_SHADER,
        ImagePipelineStage::FragmentShader => PipelineStage2Flags::FRAGMENT_SHADER,
        ImagePipelineStage::ComputeShader => PipelineStage2Flags::COMPUTE_SHADER,
        ImagePipelineStage::ColorAttachmentOutput => PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT,
        ImagePipelineStage::DepthStencil => {
            PipelineStage2Flags::EARLY_FRAGMENT_TESTS | PipelineStage2Flags::LATE_FRAGMENT_TESTS
        }
        ImagePipelineStage::Transfer => PipelineStage2Flags::TRANSFER,
        // The presentation engine reads the image after submission completes;
        // the present semaphore orders the hand-off, not a pipeline stage.
        ImagePipelineStage::Present => PipelineStage2Flags::BOTTOM_OF_PIPE,
        ImagePipelineStage::AllGraphics => PipelineStage2Flags::ALL_GRAPHICS,
    }
}

fn usage_access_mask(usage: ImageUsage, mode: ImageAccessMode) -> AccessFlags2 {
    let read = mode.reads();
    let write = mode.writes();
    match usage {
        ImageUsage::Sampled => {
            let mut access = AccessFlags2::empty();
            if read {
                access |= AccessFlags2::SHADER_READ;
            }
            if write {
                access |= AccessFlags2::SHADER_WRITE;
            }
            access
        }
        ImageUsage::ColorAttachment => {
            let mut access = AccessFlags2::empty();
            if read {
                access |= AccessFlags2::COLOR_ATTACHMENT_READ;
            }
            if write {
                access |= AccessFlags2::COLOR_ATTACHMENT_WRITE;
            }
            access
        }
        ImageUsage::DepthStencilAttachment => {
            let mut access = AccessFlags2::empty();
            if read {
                access |= AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ;
            }
            if write {
                access |= AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE;
            }
            access
        }
        ImageUsage::Storage => {
            let mut access = AccessFlags2::empty();
            if read {
                access |= AccessFlags2::SHADER_READ;
            }
            if write {
                access |= AccessFlags2::SHADER_WRITE;
            }
            access
        }
        ImageUsage::TransferSource if read => AccessFlags2::TRANSFER_READ,
        ImageUsage::TransferDestination if write => AccessFlags2::TRANSFER_WRITE,
        ImageUsage::TransferSource | ImageUsage::TransferDestination => AccessFlags2::NONE,
        // The presentation engine's read is ordered by the present semaphore.
        ImageUsage::Present => AccessFlags2::NONE,
    }
}
