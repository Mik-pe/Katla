//! Shared draw helpers for skinned/non-skinned mesh drawing.
//!
//! Extracts the common draw loop pattern used across depth prepass, shadow pass,
//! outline pass, and geometry pass. Each pass has the same core loop:
//! iterate draw lists, switch pipeline for skinned meshes, bind vertex buffers,
//! bind skeleton descriptors, draw indexed.

use std::rc::Rc;

use crate::render_graph::error::RenderGraphError;
use crate::renderer::VulkanRenderer;
use crate::renderer::types::DrawList;
use crate::vulkan::commandbuffer::CommandBuffer;
use crate::vulkan::vertex_attribute::AttributeType;
use ash::vk;

/// Configures which descriptor sets to bind besides storage (set 0).
pub(super) struct DescriptorConfig {
    /// Bind set 1 (bindless textures)?
    pub bind_textures: bool,
    /// Which descriptor set number to bind skeleton descriptors at.
    /// Shadow pass uses set 3; other passes use set 2.
    pub skeleton_set: u32,
    /// Extra descriptor sets to bind after set 0 (e.g., shadow cascades).
    pub extra_sets: Vec<(u32, vk::DescriptorSet)>,
}

/// Parameters for the shared skinned/non-skinned draw loop.
pub(super) struct DrawParams<'a> {
    pub cmd: &'a CommandBuffer,
    pub renderer: &'a mut VulkanRenderer,
    pub draw_lists: &'a [Rc<DrawList>],
    pub pipeline: vk::Pipeline,
    pub layout: vk::PipelineLayout,
    pub skinned_pipeline: Option<vk::Pipeline>,
    pub skinned_layout: Option<vk::PipelineLayout>,
    pub frame_idx: usize,
    pub descriptors: DescriptorConfig,
}

/// Execute the common skinned/non-skinned draw loop.
///
/// Iterates all draw lists, switching between regular and skinned pipelines
/// as needed, binding vertex buffers and skeleton descriptors per draw call.
pub(super) fn draw_meshes_with_skinning(params: DrawParams<'_>) -> Result<(), RenderGraphError> {
    let DrawParams {
        cmd,
        renderer,
        draw_lists,
        pipeline,
        layout,
        skinned_pipeline,
        skinned_layout,
        frame_idx,
        descriptors,
    } = params;

    // Bind the base (non-skinned) pipeline
    unsafe {
        renderer.context.device.cmd_bind_pipeline(
            cmd.vk_command_buffer(),
            vk::PipelineBindPoint::GRAPHICS,
            pipeline,
        );
    }

    let storage_ds = renderer.storage_descriptor_sets[frame_idx].vk_set();
    cmd.bind_descriptor_sets(layout, 0, &[storage_ds], &[]);

    if descriptors.bind_textures {
        let bindless_ds = renderer.bindless_manager.descriptor_set().vk();
        cmd.bind_descriptor_sets(layout, 1, &[bindless_ds], &[]);
    }

    for &(set, ds) in &descriptors.extra_sets {
        cmd.bind_descriptor_sets(layout, set, &[ds], &[]);
    }

    let mut current_is_skinned = false;

    for draw_list in draw_lists {
        for draw_call in draw_list.iter() {
            let is_skinned = !draw_call.skeleton.is_none();

            if is_skinned && skinned_pipeline.is_none() {
                continue;
            }

            // Switch pipeline if needed
            if is_skinned != current_is_skinned {
                let (new_pipe, new_layout) = if is_skinned {
                    (skinned_pipeline.unwrap(), skinned_layout.unwrap())
                } else {
                    (pipeline, layout)
                };
                unsafe {
                    renderer.context.device.cmd_bind_pipeline(
                        cmd.vk_command_buffer(),
                        vk::PipelineBindPoint::GRAPHICS,
                        new_pipe,
                    );
                }
                let storage_ds = renderer.storage_descriptor_sets[frame_idx].vk_set();
                cmd.bind_descriptor_sets(new_layout, 0, &[storage_ds], &[]);
                if descriptors.bind_textures {
                    let bindless_ds = renderer.bindless_manager.descriptor_set().vk();
                    cmd.bind_descriptor_sets(new_layout, 1, &[bindless_ds], &[]);
                }
                for &(set, ds) in &descriptors.extra_sets {
                    cmd.bind_descriptor_sets(new_layout, set, &[ds], &[]);
                }
                current_is_skinned = is_skinned;
            }

            // Bind skeleton descriptor set for skinned meshes
            if is_skinned {
                let skel_layout = skinned_layout.unwrap();
                let skeleton_ds = renderer
                    .get_skeleton_descriptor(draw_call.skeleton)
                    .ok_or(RenderGraphError::InvalidSkeletonHandle(draw_call.skeleton))?;
                cmd.bind_descriptor_sets(
                    skel_layout,
                    descriptors.skeleton_set,
                    &[skeleton_ds.vk_set()],
                    &[],
                );
            }

            // Bind mesh vertex buffers
            let mesh = renderer
                .asset_registry
                .get_mesh(draw_call.mesh)
                .ok_or(RenderGraphError::InvalidMeshHandle(draw_call.mesh))?;

            bind_vertex_buffers(cmd, mesh, is_skinned);

            if let Some(ib) = &mesh.index_buffer {
                cmd.bind_index_buffer(ib.object(), 0, vk::IndexType::UINT32);
            }

            let index_count = mesh.index_buffer.as_ref().map(|ib| ib.count()).unwrap_or(0);

            unsafe {
                renderer.context.device.cmd_draw_indexed(
                    cmd.vk_command_buffer(),
                    index_count,
                    1,
                    0,
                    0,
                    draw_call.instance_index,
                );
            }
        }
    }

    Ok(())
}

/// Bind vertex buffers for position-only mode with optional skinning attributes.
fn bind_vertex_buffers(
    cmd: &CommandBuffer,
    mesh: &crate::renderer::registry::MeshAsset,
    is_skinned: bool,
) {
    let pos_buf = mesh
        .get_attribute_buffer(AttributeType::Position)
        .map(|vb| vb.object())
        .unwrap_or(vk::Buffer::null());

    if is_skinned {
        let joints_buf = mesh
            .get_attribute_buffer(AttributeType::JointIndices)
            .map(|vb| vb.object())
            .unwrap_or(vk::Buffer::null());
        let weights_buf = mesh
            .get_attribute_buffer(AttributeType::JointWeights)
            .map(|vb| vb.object())
            .unwrap_or(vk::Buffer::null());
        cmd.bind_vertex_buffers_at_locations(&[(0, pos_buf), (4, joints_buf), (5, weights_buf)]);
    } else {
        cmd.bind_vertex_buffers_at_locations(&[(0, pos_buf)]);
    }
}
