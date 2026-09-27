//! Vulkan realization of the compiled synchronization plan.
//!
//! Each backend-neutral sync operation translates to one synchronization2
//! image barrier whose stage, access, layout, and subresource range come from
//! the typed access states — never from layout-pair inference. Buffer barriers
//! resolve both graph-owned and imported handles. Imported images still keep
//! their renderer-local acquire/present barriers, and backend-owned depth is
//! not a graph resource yet, so consecutive depth-using passes keep the
//! explicit render-pass-instance boundary barrier.

use crate::barrier::ImageBarrier;
use crate::render_graph::access::{
    BufferUsage, ImageAspects, ResourceAccessMode, ResourceAccessStage, ResourceAccessUsage,
};
use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame::Frame;
use crate::render_graph::{BufferSyncState, ImageSyncOp, ImageSyncState};
use crate::renderer::VulkanRenderer;
use crate::sync::{
    AccessFlags2, BufferMemoryBarrier2, DependencyInfo, ImageMemoryBarrier2, PipelineStage2Flags,
    VkBuffer, VkImage,
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

        let image_ops = self.graph.image_sync_ops(pass_index);
        let buffer_ops = self.graph.buffer_sync_ops(pass_index);
        if image_ops.is_empty() && buffer_ops.is_empty() {
            return Ok(());
        }
        let Some(pass) = self.graph.pass(pass_index) else {
            return Ok(());
        };
        let frame_idx = self.current_frame();

        let mut image_barriers = Vec::with_capacity(image_ops.len());
        for op in image_ops {
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
                image_barriers.push(barrier);
            }
        }

        let mut buffer_barriers = Vec::with_capacity(buffer_ops.len());
        for op in buffer_ops {
            let Some(buffer) = self
                .graph
                .buffer_by_id(self.renderer, op.resource, frame_idx)
            else {
                return Err(RenderGraphError::BackendError(format!(
                    "Pass '{}' cannot resolve graph buffer '{}' for synchronization",
                    pass.name,
                    self.graph.resource_name(op.resource).unwrap_or("?")
                )));
            };
            if op.before == BufferSyncState::Undefined {
                continue;
            }
            let buffer_size = buffer.size();
            if op.range.offset >= buffer_size {
                continue;
            }
            let range_size = if op.range.size == u64::MAX {
                buffer_size - op.range.offset
            } else {
                op.range.size.min(buffer_size - op.range.offset)
            };
            if range_size == 0 {
                continue;
            }

            let (src_stage, src_access) = buffer_state_masks(op.before);
            let (dst_stage, dst_access) = buffer_state_masks(op.after);
            buffer_barriers.push(
                BufferMemoryBarrier2::new(
                    VkBuffer::new(buffer.buffer),
                    op.range.offset,
                    range_size,
                )
                .src_stage(src_stage)
                .dst_stage(dst_stage)
                .src_access(src_access)
                .dst_access(dst_access),
            );
        }

        if image_barriers.is_empty() && buffer_barriers.is_empty() {
            return Ok(());
        }

        let mut dependency = DependencyInfo::new();
        for barrier in image_barriers {
            dependency = dependency.add_image_barrier(barrier);
        }
        for barrier in buffer_barriers {
            dependency = dependency.add_buffer_barrier2(barrier);
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

fn buffer_state_masks(state: BufferSyncState) -> (PipelineStage2Flags, AccessFlags2) {
    match state {
        BufferSyncState::Undefined => (PipelineStage2Flags::empty(), AccessFlags2::NONE),
        BufferSyncState::Access { usage, stage, mode } => {
            (stage_mask(stage), buffer_access_mask(usage, mode))
        }
    }
}

fn buffer_access_mask(usage: BufferUsage, mode: ResourceAccessMode) -> AccessFlags2 {
    let mut access = AccessFlags2::empty();
    match usage {
        BufferUsage::Uniform | BufferUsage::Storage => {
            if mode.reads() {
                access |= AccessFlags2::SHADER_READ;
            }
            if mode.writes() {
                access |= AccessFlags2::SHADER_WRITE;
            }
        }
        BufferUsage::Vertex => access |= AccessFlags2::VERTEX_ATTRIBUTE_READ,
        BufferUsage::Index => access |= AccessFlags2::INDEX_READ,
        BufferUsage::Indirect => access |= AccessFlags2::INDIRECT_COMMAND_READ,
        BufferUsage::TransferSource => access |= AccessFlags2::TRANSFER_READ,
        BufferUsage::TransferDestination => access |= AccessFlags2::TRANSFER_WRITE,
        BufferUsage::Readback => access |= AccessFlags2::TRANSFER_READ,
    }
    access
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
            // Contents are not observable through an undefined source layout,
            // but the transition itself writes the whole image: it must still
            // be ordered after every prior write to the physical memory it
            // occupies — which under aliasing is another member's store.
            (
                PipelineStage2Flags::ALL_COMMANDS,
                AccessFlags2::MEMORY_WRITE,
            )
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
            ResourceAccessUsage::Sampled => vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            ResourceAccessUsage::ColorAttachment => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            ResourceAccessUsage::DepthStencilAttachment => {
                vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
            }
            ResourceAccessUsage::Storage => vk::ImageLayout::GENERAL,
            ResourceAccessUsage::TransferSource => vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            ResourceAccessUsage::TransferDestination => vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            ResourceAccessUsage::Present => vk::ImageLayout::PRESENT_SRC_KHR,
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

fn stage_mask(stage: ResourceAccessStage) -> PipelineStage2Flags {
    match stage {
        ResourceAccessStage::VertexShader => PipelineStage2Flags::VERTEX_SHADER,
        ResourceAccessStage::FragmentShader => PipelineStage2Flags::FRAGMENT_SHADER,
        ResourceAccessStage::ComputeShader => PipelineStage2Flags::COMPUTE_SHADER,
        ResourceAccessStage::ColorAttachmentOutput => PipelineStage2Flags::COLOR_ATTACHMENT_OUTPUT,
        ResourceAccessStage::DepthStencil => {
            PipelineStage2Flags::EARLY_FRAGMENT_TESTS | PipelineStage2Flags::LATE_FRAGMENT_TESTS
        }
        ResourceAccessStage::Transfer => PipelineStage2Flags::TRANSFER,
        // The presentation engine reads the image after submission completes;
        // the present semaphore orders the hand-off, not a pipeline stage.
        ResourceAccessStage::Present => PipelineStage2Flags::BOTTOM_OF_PIPE,
        ResourceAccessStage::AllGraphics => PipelineStage2Flags::ALL_GRAPHICS,
    }
}

fn usage_access_mask(usage: ResourceAccessUsage, mode: ResourceAccessMode) -> AccessFlags2 {
    let read = mode.reads();
    let write = mode.writes();
    match usage {
        ResourceAccessUsage::Sampled => {
            let mut access = AccessFlags2::empty();
            if read {
                access |= AccessFlags2::SHADER_READ;
            }
            if write {
                access |= AccessFlags2::SHADER_WRITE;
            }
            access
        }
        ResourceAccessUsage::ColorAttachment => {
            let mut access = AccessFlags2::empty();
            if read {
                access |= AccessFlags2::COLOR_ATTACHMENT_READ;
            }
            if write {
                access |= AccessFlags2::COLOR_ATTACHMENT_WRITE;
            }
            access
        }
        ResourceAccessUsage::DepthStencilAttachment => {
            let mut access = AccessFlags2::empty();
            if read {
                access |= AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ;
            }
            if write {
                access |= AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE;
            }
            access
        }
        ResourceAccessUsage::Storage => {
            let mut access = AccessFlags2::empty();
            if read {
                access |= AccessFlags2::SHADER_READ;
            }
            if write {
                access |= AccessFlags2::SHADER_WRITE;
            }
            access
        }
        ResourceAccessUsage::TransferSource if read => AccessFlags2::TRANSFER_READ,
        ResourceAccessUsage::TransferDestination if write => AccessFlags2::TRANSFER_WRITE,
        ResourceAccessUsage::TransferSource | ResourceAccessUsage::TransferDestination => {
            AccessFlags2::NONE
        }
        // The presentation engine's read is ordered by the present semaphore.
        ResourceAccessUsage::Present => AccessFlags2::NONE,
    }
}
