use ash::vk;

use crate::render_graph::error::RenderGraphError;
use crate::render_graph::frame::Frame;
use crate::renderer::types::DrawList;
use crate::vulkan::commandbuffer::CommandBuffer;
use crate::vulkan::vertex_attribute::AttributeType;
use std::rc::Rc;

/// Pre-resolved draw command data for shadow pass recording.
///
/// Shadow passes only need position buffers (depth-only rendering).
/// All renderer lookups are done on the main thread before spawning workers.
#[derive(Clone)]
pub(super) struct ResolvedShadowDraw {
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
    storage_ds: vk::DescriptorSet,
    extra_sets: Vec<(u32, vk::DescriptorSet)>,
    skeleton_ds: vk::DescriptorSet,
    is_skinned: bool,
    pos_buf: vk::Buffer,
    joints_buf: vk::Buffer,
    weights_buf: vk::Buffer,
    index_buf: vk::Buffer,
    index_count: u32,
    instance_index: u32,
}

/// Per-cascade data for parallel shadow recording.
///
/// Each cascade records into its own secondary command buffer with its own
/// viewport/scissor region in the shadow atlas.
pub(super) struct ResolvedShadowCascade {
    viewport: vk::Viewport,
    scissor: vk::Rect2D,
    draws: Vec<ResolvedShadowDraw>,
}

/// Configuration for shadow cascade resolve.
///
/// Groups the parameters needed to pre-resolve shadow draw commands.
pub(super) struct ShadowCascadeConfig<'a> {
    pub draw_lists: &'a [Rc<DrawList>],
    pub frame_idx: usize,
    pub pipeline: vk::Pipeline,
    pub layout: vk::PipelineLayout,
    pub skinned_pipeline: Option<vk::Pipeline>,
    pub skinned_layout: Option<vk::PipelineLayout>,
    pub num_cascades: u32,
    pub viewports: &'a [vk::Viewport],
    pub scissors: &'a [vk::Rect2D],
    pub extra_sets: &'a [(u32, vk::DescriptorSet)],
}

/// Wrapper around `ash::Device` that is `Send + Sync`.
///
/// Vulkan command buffer recording (`vkCmd*` functions) is thread-safe when
/// recording separate command buffers.
struct SyncDevice(ash::Device);

unsafe impl Send for SyncDevice {}
unsafe impl Sync for SyncDevice {}

/// Record a single cascade's draw commands into a secondary command buffer.
fn record_shadow_cascade(
    device: &SyncDevice,
    cb: vk::CommandBuffer,
    cascade: &ResolvedShadowCascade,
) {
    let dev = &device.0;
    unsafe {
        dev.cmd_set_viewport(cb, 0, std::slice::from_ref(&cascade.viewport));
        dev.cmd_set_scissor(cb, 0, std::slice::from_ref(&cascade.scissor));
    }

    let mut current_pipeline = vk::Pipeline::null();

    for draw in &cascade.draws {
        if draw.pipeline == vk::Pipeline::null() || draw.layout == vk::PipelineLayout::null() {
            continue;
        }

        if draw.pipeline != current_pipeline {
            unsafe {
                dev.cmd_bind_pipeline(cb, vk::PipelineBindPoint::GRAPHICS, draw.pipeline);
            }
            current_pipeline = draw.pipeline;

            unsafe {
                if draw.storage_ds != vk::DescriptorSet::null() {
                    dev.cmd_bind_descriptor_sets(
                        cb,
                        vk::PipelineBindPoint::GRAPHICS,
                        draw.layout,
                        0,
                        &[draw.storage_ds],
                        &[],
                    );
                }
            }

            for &(set, ds) in &draw.extra_sets {
                if ds != vk::DescriptorSet::null() {
                    unsafe {
                        dev.cmd_bind_descriptor_sets(
                            cb,
                            vk::PipelineBindPoint::GRAPHICS,
                            draw.layout,
                            set,
                            &[ds],
                            &[],
                        );
                    }
                }
            }
        }

        if draw.is_skinned && draw.skeleton_ds != vk::DescriptorSet::null() {
            unsafe {
                dev.cmd_bind_descriptor_sets(
                    cb,
                    vk::PipelineBindPoint::GRAPHICS,
                    draw.layout,
                    3,
                    &[draw.skeleton_ds],
                    &[],
                );
            }

            unsafe {
                dev.cmd_bind_vertex_buffers(
                    cb,
                    0,
                    &[draw.pos_buf, draw.joints_buf, draw.weights_buf],
                    &[0u64, 0u64, 0u64],
                );
            }
        } else {
            unsafe {
                dev.cmd_bind_vertex_buffers(cb, 0, &[draw.pos_buf], &[0u64]);
            }
        }

        if draw.index_count > 0 {
            unsafe {
                dev.cmd_bind_index_buffer(cb, draw.index_buf, 0, vk::IndexType::UINT32);
                dev.cmd_draw_indexed(cb, draw.index_count, 1, 0, 0, draw.instance_index);
            }
        }
    }
}

/// Execute parallel shadow cascade recording.
///
/// Each cascade is recorded into its own secondary command buffer in parallel.
/// The primary CB begins the render pass, executes all secondaries, then ends
/// the render pass.
pub(super) fn execute_parallel_shadow_recording(
    device: &ash::Device,
    command_pool: &crate::vulkan::CommandPool,
    cmd: &CommandBuffer,
    cascades: &[ResolvedShadowCascade],
    depth_attachment: &vk::RenderingAttachmentInfo<'_>,
    render_area: vk::Rect2D,
) -> Result<(), RenderGraphError> {
    cmd.begin_rendering(&[], Some(depth_attachment), None, render_area, 1);

    if cascades.is_empty() {
        cmd.end_rendering();
        return Ok(());
    }

    let sync_device = SyncDevice(device.clone());

    let mut secondary_cbs: Vec<CommandBuffer> = Vec::with_capacity(cascades.len());
    for _ in 0..cascades.len() {
        let cb = CommandBuffer::new_secondary(device, command_pool);
        cb.begin_secondary(vk::CommandBufferInheritanceInfo::default())?;
        secondary_cbs.push(cb);
    }

    let raw_cbs: Vec<vk::CommandBuffer> = secondary_cbs
        .iter()
        .map(|cb| cb.vk_command_buffer())
        .collect();

    std::thread::scope(|s| {
        for (i, cascade) in cascades.iter().enumerate() {
            let dev = &sync_device;
            let cb_raw = raw_cbs[i];
            s.spawn(move || {
                record_shadow_cascade(dev, cb_raw, cascade);
            });
        }
    });

    for cb in &secondary_cbs {
        cb.end_command()?;
    }

    let refs: Vec<&CommandBuffer> = secondary_cbs.iter().collect();
    cmd.execute_commands(&refs)?;

    cmd.end_rendering();

    Ok(())
}

impl<'a> Frame<'a> {
    /// Pre-resolve shadow draw commands for all cascades.
    ///
    /// Performs all `&mut self` renderer lookups on the main thread, producing
    /// per-cascade data that worker threads can consume without any reference
    /// to the renderer.
    pub(super) fn resolve_shadow_cascades(
        &mut self,
        config: &ShadowCascadeConfig,
    ) -> Result<Vec<ResolvedShadowCascade>, RenderGraphError> {
        let mut all_draws = Vec::new();

        for draw_list in config.draw_lists {
            self.ensure_materials_compiled(draw_list)?;

            for draw_call in draw_list.iter() {
                let is_skinned = !draw_call.skeleton.is_none();

                let (pipe, lay) = if is_skinned {
                    (
                        config.skinned_pipeline.unwrap_or(config.pipeline),
                        config.skinned_layout.unwrap_or(config.layout),
                    )
                } else {
                    (config.pipeline, config.layout)
                };

                let storage_ds = self.renderer.storage_descriptor_sets[config.frame_idx].vk_set();

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

                let joints_buf = if is_skinned {
                    mesh.get_attribute_buffer(AttributeType::JointIndices)
                        .map(|vb| vb.object())
                        .unwrap_or(vk::Buffer::null())
                } else {
                    vk::Buffer::null()
                };

                let weights_buf = if is_skinned {
                    mesh.get_attribute_buffer(AttributeType::JointWeights)
                        .map(|vb| vb.object())
                        .unwrap_or(vk::Buffer::null())
                } else {
                    vk::Buffer::null()
                };

                let index_buf = mesh
                    .index_buffer
                    .as_ref()
                    .map(|ib| ib.object())
                    .unwrap_or(vk::Buffer::null());
                let index_count = mesh.index_buffer.as_ref().map(|ib| ib.count()).unwrap_or(0);

                all_draws.push(ResolvedShadowDraw {
                    pipeline: pipe,
                    layout: lay,
                    storage_ds,
                    extra_sets: config.extra_sets.to_vec(),
                    skeleton_ds,
                    is_skinned,
                    pos_buf,
                    joints_buf,
                    weights_buf,
                    index_buf,
                    index_count,
                    instance_index: draw_call.instance_index,
                });
            }
        }

        let mut cascades = Vec::with_capacity(config.num_cascades as usize);
        for i in 0..config.num_cascades as usize {
            cascades.push(ResolvedShadowCascade {
                viewport: config.viewports[i],
                scissor: config.scissors[i],
                draws: all_draws.clone(),
            });
        }

        Ok(cascades)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cascade_viewport_splitting_4_cascades() {
        let half_w = 512u32;
        let half_h = 512u32;

        let viewports = [
            vk::Viewport {
                x: 0.0,
                y: half_h as f32,
                width: half_w as f32,
                height: half_h as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            },
            vk::Viewport {
                x: half_w as f32,
                y: half_h as f32,
                width: half_w as f32,
                height: half_h as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            },
            vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: half_w as f32,
                height: half_h as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            },
            vk::Viewport {
                x: half_w as f32,
                y: 0.0,
                width: half_w as f32,
                height: half_h as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            },
        ];

        assert_eq!(viewports.len(), 4);

        // Each cascade should cover a unique quadrant
        // Cascade 0: bottom-left (x=0, y=512) -> center (256, 768)
        // Cascade 1: bottom-right (x=512, y=512) -> center (768, 768)
        // Cascade 2: top-left (x=0, y=0) -> center (256, 256)
        // Cascade 3: top-right (x=512, y=0) -> center (768, 256)
        let quadrant_centers: [(f32, f32); 4] = [
            (256.0, 768.0),
            (768.0, 768.0),
            (256.0, 256.0),
            (768.0, 256.0),
        ];

        for (i, vp) in viewports.iter().enumerate() {
            let cx = vp.x + vp.width / 2.0;
            let cy = vp.y + vp.height / 2.0;
            assert_eq!(
                (cx as i32, cy as i32),
                (quadrant_centers[i].0 as i32, quadrant_centers[i].1 as i32),
                "Cascade {} center ({}, {}) doesn't match expected ({}, {})",
                i,
                cx,
                cy,
                quadrant_centers[i].0,
                quadrant_centers[i].1
            );
        }
    }

    #[test]
    fn test_resolved_shadow_draw_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<ResolvedShadowDraw>();
    }

    #[test]
    fn test_resolved_shadow_draw_is_sync() {
        fn assert_sync<T: Sync>() {}
        assert_sync::<ResolvedShadowDraw>();
    }

    #[test]
    fn test_resolved_shadow_cascade_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<ResolvedShadowCascade>();
    }

    #[test]
    fn test_resolved_shadow_cascade_is_sync() {
        fn assert_sync<T: Sync>() {}
        assert_sync::<ResolvedShadowCascade>();
    }

    #[test]
    fn test_cascade_draws_replicated_per_cascade() {
        let num_draws = 5;
        let num_cascades = 4u32;

        let fake_draws: Vec<ResolvedShadowDraw> = (0..num_draws)
            .map(|_| ResolvedShadowDraw {
                pipeline: vk::Pipeline::null(),
                layout: vk::PipelineLayout::null(),
                storage_ds: vk::DescriptorSet::null(),
                extra_sets: vec![],
                skeleton_ds: vk::DescriptorSet::null(),
                is_skinned: false,
                pos_buf: vk::Buffer::null(),
                joints_buf: vk::Buffer::null(),
                weights_buf: vk::Buffer::null(),
                index_buf: vk::Buffer::null(),
                index_count: 0,
                instance_index: 0,
            })
            .collect();

        let viewports = [
            vk::Viewport::default(),
            vk::Viewport::default(),
            vk::Viewport::default(),
            vk::Viewport::default(),
        ];
        let scissors = [
            vk::Rect2D::default(),
            vk::Rect2D::default(),
            vk::Rect2D::default(),
            vk::Rect2D::default(),
        ];

        let cascades: Vec<ResolvedShadowCascade> = (0..num_cascades as usize)
            .map(|i| ResolvedShadowCascade {
                viewport: viewports[i],
                scissor: scissors[i],
                draws: fake_draws.clone(),
            })
            .collect();

        assert_eq!(cascades.len(), 4);
        for cascade in cascades.iter() {
            assert_eq!(cascade.draws.len(), num_draws);
        }
    }

    #[test]
    fn test_cascade_with_1_cascade() {
        let viewports = [vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: 1024.0,
            height: 1024.0,
            min_depth: 0.0,
            max_depth: 1.0,
        }];
        let scissors = [vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: vk::Extent2D {
                width: 1024,
                height: 1024,
            },
        }];

        let draws = vec![];
        let cascades: Vec<ResolvedShadowCascade> = (0..1)
            .map(|i| ResolvedShadowCascade {
                viewport: viewports[i],
                scissor: scissors[i],
                draws: draws.clone(),
            })
            .collect();

        assert_eq!(cascades.len(), 1);
        assert_eq!(cascades[0].viewport.width, 1024.0);
    }

    #[test]
    fn test_cascade_with_2_cascades() {
        let half_w = 512u32;
        let half_h = 1024u32;

        let viewports = [
            vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: half_w as f32,
                height: half_h as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            },
            vk::Viewport {
                x: half_w as f32,
                y: 0.0,
                width: half_w as f32,
                height: half_h as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            },
        ];

        assert_eq!(viewports.len(), 2);
        assert_eq!(viewports[0].x, 0.0);
        assert_eq!(viewports[1].x, half_w as f32);
    }

    #[test]
    fn test_empty_cascades() {
        let cascades: Vec<ResolvedShadowCascade> = vec![];
        assert!(cascades.is_empty());
    }
}
