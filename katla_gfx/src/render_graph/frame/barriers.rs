//! Vulkan realization of the compiled synchronization plan.
//!
//! Native stage, access, layout, and range scopes come from compiled accesses.
//! Imported textures and the acquired output are resolved alongside transients.

use crate::render_graph::access::{
    BufferUsage, ImageAspects, ResourceAccessMode, ResourceAccessStage, ResourceAccessUsage,
};
use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame::Frame;
use crate::render_graph::{BufferSyncState, ImageSyncOp, ImageSyncState, SyncReason};
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
        let cmd_vk = cmd.vk_command_buffer();

        let image_ops = self.graph.image_sync_ops(pass_index).to_vec();
        let buffer_ops = self.graph.buffer_sync_ops(pass_index);
        let alias_handoff = self.graph.texture_alias_handoff_before(pass_index);
        if image_ops.is_empty() && buffer_ops.is_empty() && !alias_handoff {
            return Ok(());
        }
        let Some(pass) = self.graph.pass(pass_index) else {
            return Ok(());
        };
        let frame_idx = self.current_frame();

        let mut image_barriers = Vec::with_capacity(image_ops.len());
        for op in &image_ops {
            image_barriers.extend(self.resolve_image_sync_barriers(op)?);
        }

        let mut buffer_barriers = Vec::with_capacity(buffer_ops.len());
        for op in buffer_ops {
            let Some(buffer) = self
                .graph
                .buffer_by_id(self.renderer, op.resource, frame_idx)
            else {
                if self.graph.is_builtin_buffer(op.resource) {
                    continue;
                }
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
                    buffer.offset + op.range.offset,
                    range_size,
                )
                .src_stage(src_stage)
                .dst_stage(dst_stage)
                .src_access(src_access)
                .dst_access(dst_access),
            );
        }

        if image_barriers.is_empty() && buffer_barriers.is_empty() && !alias_handoff {
            return Ok(());
        }

        let mut dependency = DependencyInfo::new();
        if alias_handoff {
            dependency.memory_barriers.push(
                vk::MemoryBarrier2::default()
                    .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                    .src_access_mask(vk::AccessFlags2::MEMORY_WRITE)
                    .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                    .dst_access_mask(
                        vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE,
                    ),
            );
        }
        for barrier in image_barriers {
            dependency = dependency.add_image_barrier(barrier);
        }
        for barrier in buffer_barriers {
            dependency = dependency.add_buffer_barrier2(barrier);
        }
        dependency.build(|dependency| unsafe {
            self.renderer
                .context
                .device
                .cmd_pipeline_barrier2(cmd_vk, dependency);
        });

        Ok(())
    }

    fn resolve_image_sync_barriers(
        &mut self,
        op: &ImageSyncOp,
    ) -> Result<Vec<ImageMemoryBarrier2>, RenderGraphError> {
        if let Some(texture) = self
            .graph
            .transient_texture_by_id(op.resource, self.current_frame())
        {
            self.renderer
                .frame_context
                .pending_transient_layouts
                .borrow_mut()
                .record(texture);
            return Ok(sync_op_barriers(op, texture));
        }
        let (image, aspects, initial_layout) =
            if self.graph.resource_id(crate::render_graph::BACKBUFFER_NAME) == Some(op.resource) {
                (
                    self.renderer.frame_context.swapchain_images[self.image_index as usize],
                    ImageAspects::COLOR,
                    self.renderer.frame_context.swapchain_image_layouts[self.image_index as usize]
                        .get(),
                )
            } else {
                let texture = self
                    .graph
                    .imported_images
                    .get(&op.resource)
                    .and_then(|&handle| self.renderer.texture_manager.get_texture(handle))
                    .ok_or_else(|| {
                        RenderGraphError::ResourceNotFound(format!(
                            "Cannot resolve imported image '{}' for synchronization",
                            self.graph.resource_name(op.resource).unwrap_or("?")
                        ))
                    })?;
                (
                    texture.image(),
                    format_aspects(texture.format().into()),
                    state_layout(op.before),
                )
            };
        let pieces = self.imported_image_states.entry(op.resource).or_default();
        let mut ranges = Vec::new();
        let mut remainder = vec![op.range];
        for &(piece, state) in pieces.iter() {
            if let Some(range) = piece.intersection(op.range) {
                ranges.push((range, state_layout(state)));
                remainder = remainder
                    .iter()
                    .flat_map(|range| range.subtract(piece))
                    .collect();
            }
        }
        ranges.extend(remainder.into_iter().map(|range| (range, initial_layout)));
        let mut barriers = Vec::new();
        for (range, layout) in ranges {
            let ranged_op = ImageSyncOp { range, ..*op };
            if let Some(barrier) = image_sync_barrier(&ranged_op, image, aspects, layout) {
                *pieces = pieces
                    .iter()
                    .flat_map(|&(piece, old)| {
                        piece
                            .subtract(range)
                            .into_iter()
                            .map(move |piece| (piece, old))
                    })
                    .collect();
                pieces.push((range, op.after));
                barriers.push(barrier);
            }
        }
        Ok(barriers)
    }

    /// Insert the frame-end operations satisfying imported final-state
    /// contracts, after the last live pass.
    ///
    /// Acquired and imported images obey the same compiled final operations.
    pub(super) fn insert_final_sync_barriers(
        &mut self,
        cmd: &CommandBuffer,
    ) -> Result<(), RenderGraphError> {
        let ops = self.graph.final_image_sync_ops().to_vec();
        if ops.is_empty() {
            return Ok(());
        }

        let cmd_vk = cmd.vk_command_buffer();

        let mut barriers = Vec::with_capacity(ops.len());
        for op in &ops {
            barriers.extend(self.resolve_image_sync_barriers(op)?);
        }

        if barriers.is_empty() {
            return Ok(());
        }

        let mut dependency = DependencyInfo::new();
        for barrier in barriers {
            dependency = dependency.add_image_barrier(barrier);
        }
        dependency.build(|dependency| unsafe {
            self.renderer
                .context
                .device
                .cmd_pipeline_barrier2(cmd_vk, dependency);
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
        BufferUsage::Uniform => access |= AccessFlags2::UNIFORM_READ,
        BufferUsage::Storage => {
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
        BufferUsage::Readback => access |= AccessFlags2::HOST_READ,
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
fn sync_op_barriers(
    op: &ImageSyncOp,
    transient: &crate::render_graph::TransientTexture,
) -> Vec<ImageMemoryBarrier2> {
    let texture_aspects = format_aspects(transient.format);
    let ranges = transient
        .layouts
        .borrow()
        .ranges(op.range, vk::ImageLayout::UNDEFINED);
    let mut barriers = Vec::new();
    for (range, layout) in ranges {
        let ranged_op = ImageSyncOp { range, ..*op };
        if let Some(barrier) = image_sync_barrier(
            &ranged_op,
            VkImage::new(transient.image),
            texture_aspects,
            layout,
        ) {
            transient.set_range_layout(range, barrier.new_layout);
            barriers.push(barrier);
        }
    }
    barriers
}

fn format_aspects(format: vk::Format) -> ImageAspects {
    match format {
        vk::Format::D32_SFLOAT => ImageAspects::DEPTH,
        vk::Format::D32_SFLOAT_S8_UINT | vk::Format::D24_UNORM_S8_UINT => {
            ImageAspects::DEPTH | ImageAspects::STENCIL
        }
        _ => ImageAspects::COLOR,
    }
}

fn image_sync_barrier(
    op: &ImageSyncOp,
    image: VkImage,
    texture_aspects: ImageAspects,
    tracked_layout: vk::ImageLayout,
) -> Option<ImageMemoryBarrier2> {
    let aspects = op.range.aspects & texture_aspects;
    if aspects.is_empty() {
        return None;
    }

    let needed_layout = state_layout(op.after);
    if tracked_layout == needed_layout
        && op.before == ImageSyncState::Undefined
        && op.reason == SyncReason::InitialUse
    {
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

    let barrier = ImageMemoryBarrier2::new(image)
        .src_stage(src_stage)
        .dst_stage(dst_stage)
        .src_access(src_access)
        .dst_access(dst_access)
        .old_layout(tracked_layout)
        .new_layout(needed_layout)
        .subresource_range(vk_subresource_range(aspects, op));

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
        ResourceAccessStage::VertexInput => PipelineStage2Flags::VERTEX_INPUT,
        ResourceAccessStage::DrawIndirect => PipelineStage2Flags::DRAW_INDIRECT,
        ResourceAccessStage::Host => PipelineStage2Flags::HOST,
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
        ResourceAccessStage::Present => PipelineStage2Flags::empty(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_graph::{BufferAccess, ResourceId};

    fn storage_op() -> ImageSyncOp {
        ImageSyncOp {
            resource: ResourceId(0),
            range: crate::render_graph::ImageSubresourceRange::WHOLE_COLOR,
            before: ImageSyncState::Access {
                usage: ResourceAccessUsage::Storage,
                stage: ResourceAccessStage::ComputeShader,
                mode: ResourceAccessMode::Write,
            },
            after: ImageSyncState::Access {
                usage: ResourceAccessUsage::Storage,
                stage: ResourceAccessStage::FragmentShader,
                mode: ResourceAccessMode::Read,
            },
            before_pass: Some(0),
            pass: 1,
            reason: crate::render_graph::SyncReason::Hazard(
                crate::render_graph::ResourceHazardKind::ReadAfterWrite,
            ),
        }
    }

    #[test]
    fn test_storage_write_to_read_keeps_the_same_layout_memory_barrier() {
        let barrier = image_sync_barrier(
            &storage_op(),
            VkImage::new(vk::Image::null()),
            ImageAspects::COLOR,
            vk::ImageLayout::GENERAL,
        )
        .expect("same-layout RAW hazard must reach the driver")
        .into_vk();
        assert_eq!(barrier.old_layout, vk::ImageLayout::GENERAL);
        assert_eq!(barrier.new_layout, vk::ImageLayout::GENERAL);
        assert_eq!(
            barrier.src_stage_mask,
            vk::PipelineStageFlags2::COMPUTE_SHADER
        );
        assert_eq!(barrier.src_access_mask, vk::AccessFlags2::SHADER_WRITE);
        assert_eq!(
            barrier.dst_stage_mask,
            vk::PipelineStageFlags2::FRAGMENT_SHADER
        );
        assert_eq!(barrier.dst_access_mask, vk::AccessFlags2::SHADER_READ);
    }

    #[test]
    fn test_only_satisfied_initial_layout_bootstraps_are_coalesced() {
        let mut op = storage_op();
        op.before = ImageSyncState::Undefined;
        op.before_pass = None;
        op.reason = SyncReason::InitialUse;
        let image = VkImage::new(vk::Image::null());
        assert!(
            image_sync_barrier(&op, image, ImageAspects::COLOR, vk::ImageLayout::GENERAL).is_none()
        );
        let barrier =
            image_sync_barrier(&op, image, ImageAspects::COLOR, vk::ImageLayout::UNDEFINED)
                .unwrap()
                .into_vk();
        assert_eq!(
            barrier.src_stage_mask,
            vk::PipelineStageFlags2::ALL_COMMANDS
        );
        assert_eq!(barrier.src_access_mask, vk::AccessFlags2::MEMORY_WRITE);
        assert_eq!(barrier.new_layout, vk::ImageLayout::GENERAL);
    }

    #[test]
    fn test_matching_layout_keeps_cross_stage_read_ordering() {
        let mut op = storage_op();
        op.before = ImageSyncState::Access {
            usage: ResourceAccessUsage::Storage,
            stage: ResourceAccessStage::ComputeShader,
            mode: ResourceAccessMode::Read,
        };
        op.reason = SyncReason::StateChange;
        let barrier = image_sync_barrier(
            &op,
            VkImage::new(vk::Image::null()),
            ImageAspects::COLOR,
            vk::ImageLayout::GENERAL,
        )
        .unwrap()
        .into_vk();
        assert_eq!(
            barrier.src_stage_mask,
            vk::PipelineStageFlags2::COMPUTE_SHADER
        );
        assert_eq!(
            barrier.dst_stage_mask,
            vk::PipelineStageFlags2::FRAGMENT_SHADER
        );
    }

    #[test]
    fn test_buffer_consumers_lower_to_the_native_execution_stages() {
        let resource = ResourceId(0);
        for (access, expected_stage, expected_access) in [
            (
                BufferAccess::vertex_read(resource),
                PipelineStage2Flags::VERTEX_INPUT,
                AccessFlags2::VERTEX_ATTRIBUTE_READ,
            ),
            (
                BufferAccess::index_read(resource),
                PipelineStage2Flags::VERTEX_INPUT,
                AccessFlags2::INDEX_READ,
            ),
            (
                BufferAccess::indirect_read(resource),
                PipelineStage2Flags::DRAW_INDIRECT,
                AccessFlags2::INDIRECT_COMMAND_READ,
            ),
            (
                BufferAccess::uniform_read(resource).with_stage(ResourceAccessStage::ComputeShader),
                PipelineStage2Flags::COMPUTE_SHADER,
                AccessFlags2::UNIFORM_READ,
            ),
            (
                BufferAccess::readback_read(resource),
                PipelineStage2Flags::HOST,
                AccessFlags2::HOST_READ,
            ),
            (
                BufferAccess::storage_read_write(resource),
                PipelineStage2Flags::COMPUTE_SHADER,
                AccessFlags2::SHADER_READ | AccessFlags2::SHADER_WRITE,
            ),
            (
                BufferAccess::transfer_write(resource),
                PipelineStage2Flags::TRANSFER,
                AccessFlags2::TRANSFER_WRITE,
            ),
        ] {
            assert_eq!(
                buffer_state_masks(BufferSyncState::Access {
                    usage: access.usage,
                    stage: access.stage,
                    mode: access.mode,
                }),
                (expected_stage, expected_access)
            );
        }
    }
    #[test]
    fn test_present_contract_releases_to_the_external_engine_without_a_fake_stage() {
        let mut op = storage_op();
        op.after = ImageSyncState::Access {
            usage: ResourceAccessUsage::Present,
            stage: ResourceAccessStage::Present,
            mode: ResourceAccessMode::Write,
        };
        op.reason = SyncReason::ImportedFinal;
        let barrier = image_sync_barrier(
            &op,
            VkImage::new(vk::Image::null()),
            ImageAspects::COLOR,
            vk::ImageLayout::GENERAL,
        )
        .unwrap()
        .into_vk();
        assert_eq!(barrier.new_layout, vk::ImageLayout::PRESENT_SRC_KHR);
        assert_eq!(
            barrier.src_stage_mask,
            vk::PipelineStageFlags2::COMPUTE_SHADER
        );
        assert_eq!(barrier.src_access_mask, vk::AccessFlags2::SHADER_WRITE);
        assert!(barrier.dst_stage_mask.is_empty());
        assert!(barrier.dst_access_mask.is_empty());
    }

    #[test]
    fn test_subresource_barriers_retain_mips_layers_and_both_depth_aspects() {
        let mut op = storage_op();
        op.range = crate::render_graph::ImageSubresourceRange::new(
            ImageAspects::DEPTH | ImageAspects::STENCIL,
            2,
            1,
            3,
            2,
        );
        let barrier = image_sync_barrier(
            &op,
            VkImage::new(vk::Image::null()),
            ImageAspects::DEPTH | ImageAspects::STENCIL,
            vk::ImageLayout::GENERAL,
        )
        .unwrap()
        .into_vk();
        assert_eq!(
            barrier.subresource_range.aspect_mask,
            vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL
        );
        assert_eq!(barrier.subresource_range.base_mip_level, 2);
        assert_eq!(barrier.subresource_range.level_count, 1);
        assert_eq!(barrier.subresource_range.base_array_layer, 3);
        assert_eq!(barrier.subresource_range.layer_count, 2);
    }
}
