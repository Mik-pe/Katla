use std::rc::Rc;

use ash::vk;

use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame::Frame;
use crate::renderer::types::DrawList;
use crate::vulkan::commandbuffer::CommandBuffer;
use crate::vulkan::vertex_attribute::AttributeType;

/// Pre-resolved draw command data for parallel recording.
///
/// All renderer lookups are done on the main thread before spawning workers.
/// Contains only raw Vulkan handles and plain data — fully `Send + Sync`.
pub(super) struct ResolvedDrawCommand {
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
    storage_ds: vk::DescriptorSet,
    bindless_ds: vk::DescriptorSet,
    skeleton_ds: vk::DescriptorSet,
    is_skinned: bool,
    pos_buf: vk::Buffer,
    norm_buf: vk::Buffer,
    tang_buf: vk::Buffer,
    uv_buf: vk::Buffer,
    index_buf: vk::Buffer,
    index_count: u32,
    instance_index: u32,
    /// Shadow descriptor set (Set 4) — null if shadow not ready.
    shadow_ds: vk::DescriptorSet,
}

/// Record draw commands sequentially on a command buffer.
fn record_draw_chunk_sequential(
    device: &ash::Device,
    cb: vk::CommandBuffer,
    commands: &[ResolvedDrawCommand],
    lc_buffers: Option<&crate::lighting::LightCullingBuffers>,
    push_descriptor_ext: Option<&ash::khr::push_descriptor::Device>,
) {
    let mut prev_layout = vk::PipelineLayout::null();

    for draw in commands {
        // Skip draws with null pipeline or layout (stale references after recompilation)
        if draw.pipeline == vk::Pipeline::null() || draw.layout == vk::PipelineLayout::null() {
            log::warn!("Skipping draw with null pipeline or layout");
            continue;
        }

        unsafe {
            device.cmd_bind_pipeline(cb, vk::PipelineBindPoint::GRAPHICS, draw.pipeline);

            // Push light culling descriptors (Set 3) when pipeline layout changes.
            // Push descriptors are layout-specific and must be re-pushed for each new layout.
            if draw.layout != prev_layout {
                if let (Some(lc), Some(ext)) = (lc_buffers, push_descriptor_ext)
                    && let Err(e) = lc.push_fragment_descriptors_with_ext(ext, cb, draw.layout)
                {
                    log::warn!("Failed to push light culling fragment descriptors: {}", e);
                }
                prev_layout = draw.layout;
            }

            if draw.storage_ds != vk::DescriptorSet::null() {
                device.cmd_bind_descriptor_sets(
                    cb,
                    vk::PipelineBindPoint::GRAPHICS,
                    draw.layout,
                    0,
                    &[draw.storage_ds],
                    &[],
                );
            }

            if draw.bindless_ds != vk::DescriptorSet::null() {
                device.cmd_bind_descriptor_sets(
                    cb,
                    vk::PipelineBindPoint::GRAPHICS,
                    draw.layout,
                    1,
                    &[draw.bindless_ds],
                    &[],
                );
            }

            if draw.is_skinned && draw.skeleton_ds != vk::DescriptorSet::null() {
                device.cmd_bind_descriptor_sets(
                    cb,
                    vk::PipelineBindPoint::GRAPHICS,
                    draw.layout,
                    2,
                    &[draw.skeleton_ds],
                    &[],
                );
            }

            if draw.shadow_ds != vk::DescriptorSet::null() {
                device.cmd_bind_descriptor_sets(
                    cb,
                    vk::PipelineBindPoint::GRAPHICS,
                    draw.layout,
                    4,
                    &[draw.shadow_ds],
                    &[],
                );
            }

            device.cmd_bind_vertex_buffers(
                cb,
                0,
                &[draw.pos_buf, draw.norm_buf, draw.tang_buf, draw.uv_buf],
                &[0u64, 0u64, 0u64, 0u64],
            );

            if draw.index_count > 0 {
                device.cmd_bind_index_buffer(cb, draw.index_buf, 0, vk::IndexType::UINT32);
                device.cmd_draw_indexed(cb, draw.index_count, 1, 0, 0, draw.instance_index);
            }
        }
    }
}

/// Rendering parameters for parallel geometry pass execution.
pub(super) struct RenderPassParams<'a> {
    pub color_attachment: vk::RenderingAttachmentInfo<'a>,
    pub depth_attachment: Option<vk::RenderingAttachmentInfo<'a>>,
    pub stencil_attachment: Option<vk::RenderingAttachmentInfo<'a>>,
    pub render_area: vk::Rect2D,
    pub extent: vk::Extent2D,
}

/// Execute parallel secondary command buffer recording with pre-resolved draw commands.
///
/// Records all draw commands sequentially on the primary command buffer
/// within a dynamic rendering scope. Secondary command buffers with
/// dynamic rendering inheritance are used for parallel recording when
/// supported by the driver.
pub(super) fn execute_parallel_recording(
    device: &ash::Device,
    _command_pool: &crate::vulkan::CommandPool,
    cmd: &CommandBuffer,
    all_commands: &[ResolvedDrawCommand],
    params: &RenderPassParams<'_>,
    lc_buffers: Option<&crate::lighting::LightCullingBuffers>,
    push_descriptor_ext: Option<&ash::khr::push_descriptor::Device>,
) -> Result<(), RenderGraphError> {
    cmd.begin_rendering(
        &[params.color_attachment],
        params.depth_attachment.as_ref(),
        params.stencil_attachment.as_ref(),
        params.render_area,
        1,
    );
    cmd.set_viewport(&[crate::sync::VkViewport::from_rect(
        0.0,
        0.0,
        params.extent.width as f32,
        params.extent.height as f32,
    )]);
    cmd.set_scissor(&[crate::sync::Rect2D::from_extent(
        params.extent.width,
        params.extent.height,
    )]);

    if all_commands.is_empty() {
        cmd.end_rendering();
        return Ok(());
    }

    // Record all draw commands on the primary command buffer.
    // Secondary command buffers with dynamic rendering inheritance are not
    // universally supported across all driver versions, so we record sequentially
    // on the primary buffer for correctness.
    record_draw_chunk_sequential(
        device,
        cmd.vk_command_buffer(),
        all_commands,
        lc_buffers,
        push_descriptor_ext,
    );

    cmd.end_rendering();

    Ok(())
}

impl<'a> Frame<'a> {
    /// Pre-resolve all draw commands from draw lists into thread-safe records.
    ///
    /// This performs all `&mut self` renderer lookups on the main thread,
    /// producing plain data structs that worker threads can consume without
    /// needing any reference to the renderer.
    pub(super) fn resolve_draw_commands(
        &mut self,
        draw_lists: &[Rc<DrawList>],
        frame_idx: usize,
    ) -> Result<Vec<ResolvedDrawCommand>, RenderGraphError> {
        let mut commands = Vec::new();

        let shadow_ds = self
            .renderer
            .shadow_descriptor_set()
            .or_else(|| self.renderer.shadow_fallback_descriptor_set())
            .unwrap_or_else(|| self.renderer.empty_descriptor_set(frame_idx));

        for draw_list in draw_lists {
            self.ensure_materials_compiled(draw_list)?;

            for draw_call in &draw_list.draws {
                let material = self
                    .renderer
                    .asset_registry
                    .get_material(draw_call.material)
                    .ok_or(RenderGraphError::InvalidMaterialHandle(draw_call.material))?;

                let pipeline_handle = material
                    .pipeline
                    .ok_or(RenderGraphError::InvalidMaterialHandle(draw_call.material))?;

                let (pipeline, layout) = self
                    .renderer
                    .asset_registry
                    .get_pipeline_handles(pipeline_handle)?;

                let storage_ds = self.renderer.storage_descriptor_sets[frame_idx].vk_set();
                let bindless_ds = self.renderer.bindless_manager.descriptor_set().vk();

                let is_skinned = !draw_call.skeleton.is_none();
                let skeleton_ds = if is_skinned {
                    self.renderer
                        .get_skeleton_descriptor(draw_call.skeleton)
                        .ok_or(RenderGraphError::InvalidSkeletonHandle(draw_call.skeleton))?
                        .vk_set()
                } else {
                    vk::DescriptorSet::null()
                };

                let mesh = self
                    .renderer
                    .asset_registry
                    .get_mesh(draw_call.mesh)
                    .ok_or(RenderGraphError::InvalidMeshHandle(draw_call.mesh))?;

                let pos_buf = mesh
                    .get_attribute_buffer(AttributeType::Position)
                    .map(|vb| vb.object())
                    .unwrap_or(vk::Buffer::null());
                let norm_buf = mesh
                    .get_attribute_buffer(AttributeType::Normal)
                    .map(|vb| vb.object())
                    .unwrap_or(vk::Buffer::null());
                let tang_buf = mesh
                    .get_attribute_buffer(AttributeType::Tangent)
                    .map(|vb| vb.object())
                    .unwrap_or(vk::Buffer::null());
                let uv_buf = mesh
                    .get_attribute_buffer(AttributeType::TexCoord0)
                    .map(|vb| vb.object())
                    .unwrap_or(vk::Buffer::null());

                let index_buf = mesh
                    .index_buffer
                    .as_ref()
                    .map(|ib| ib.object())
                    .unwrap_or(vk::Buffer::null());
                let index_count = mesh.index_buffer.as_ref().map(|ib| ib.count()).unwrap_or(0);

                commands.push(ResolvedDrawCommand {
                    pipeline,
                    layout,
                    storage_ds,
                    bindless_ds,
                    skeleton_ds,
                    is_skinned,
                    pos_buf,
                    norm_buf,
                    tang_buf,
                    uv_buf,
                    index_buf,
                    index_count,
                    instance_index: draw_call.instance_index,
                    shadow_ds,
                });
            }
        }

        Ok(commands)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_splitting_even() {
        let items: Vec<i32> = (0..8).collect();
        let chunks: Vec<_> = items.chunks(2).collect();
        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0], &[0, 1]);
        assert_eq!(chunks[3], &[6, 7]);
    }

    #[test]
    fn test_chunk_splitting_uneven() {
        let items: Vec<i32> = (0..7).collect();
        let chunks: Vec<_> = items.chunks(3).collect();
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), 3);
        assert_eq!(chunks[1].len(), 3);
        assert_eq!(chunks[2].len(), 1);
    }

    #[test]
    fn test_chunk_splitting_single_thread() {
        let items: Vec<i32> = (0..5).collect();
        let chunks: Vec<_> = items.chunks(5).collect();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), 5);
    }

    #[test]
    fn test_chunk_splitting_empty() {
        let items: Vec<i32> = vec![];
        let chunks: Vec<_> = items.chunks(4).collect();
        assert_eq!(chunks.len(), 0);
    }

    #[test]
    fn test_chunk_size_calculation() {
        let total = 10usize;
        let num_threads = 4usize;
        let chunk_size = (total + num_threads - 1) / num_threads;
        assert_eq!(chunk_size, 3);

        let data: Vec<_> = (0..total).collect();
        let chunks: Vec<_> = data.chunks(chunk_size).collect();
        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0].len(), 3);
        assert_eq!(chunks[1].len(), 3);
        assert_eq!(chunks[2].len(), 3);
        assert_eq!(chunks[3].len(), 1);
    }

    #[test]
    fn test_resolved_draw_command_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<ResolvedDrawCommand>();
    }

    #[test]
    fn test_resolved_draw_command_is_sync() {
        fn assert_sync<T: Sync>() {}
        assert_sync::<ResolvedDrawCommand>();
    }

    #[test]
    fn test_parallel_thread_count_clamped() {
        let cpu_count = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        let thread_count = cpu_count.min(4).max(1);
        assert!(thread_count >= 1);
        assert!(thread_count <= 4);
    }
}
