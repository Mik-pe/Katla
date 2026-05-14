//! Shared draw helpers for skinned/non-skinned/billboard mesh drawing.
//!
//! Extracts the common draw loop pattern used across depth prepass, shadow pass,
//! outline pass, and geometry pass. Each pass has the same core loop:
//! iterate draw lists, switch pipeline for skinned or billboard meshes, bind vertex
//! buffers, bind skeleton descriptors, draw indexed.

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
    /// Extra descriptor sets to bind ONLY when the skinned pipeline variant is active.
    /// Used by outline draw where params are at Set 1 (non-skinned) vs Set 3 (skinned).
    pub skinned_extra_sets: Vec<(u32, vk::DescriptorSet)>,
}

/// Parameters for the shared skinned/non-skinned/billboard draw loop.
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
    /// Optional billboard depth pipeline for PBR-vertex-layout meshes (e.g. billboard icons).
    /// When a draw call's mesh has TexCoord0, this pipeline is used instead of the base pipeline.
    pub billboard_pipeline: Option<vk::Pipeline>,
    pub billboard_layout: Option<vk::PipelineLayout>,
}

/// Execute the common skinned/non-skinned/billboard draw loop.
///
/// Iterates all draw lists, switching between regular, skinned, and billboard
/// pipelines as needed. Billboard draws are detected by mesh vertex layout
/// (presence of TexCoord0) and use a dedicated pipeline that binds Set 1
/// (bindless textures) for alpha discard.
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
        billboard_pipeline,
        billboard_layout,
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

    /// Pipeline variant currently bound: regular, skinned, or billboard.
    #[derive(Clone, Copy, PartialEq)]
    enum PipelineVariant {
        Regular,
        Skinned,
        Billboard,
    }

    let mut current_variant = PipelineVariant::Regular;

    for draw_list in draw_lists {
        for draw_call in draw_list.iter() {
            let is_skinned = !draw_call.skeleton.is_none();

            if is_skinned && skinned_pipeline.is_none() {
                continue;
            }

            // Determine which mesh this draw call uses
            let mesh = renderer
                .asset_registry
                .get_mesh(draw_call.mesh)
                .ok_or(RenderGraphError::InvalidMeshHandle(draw_call.mesh))?;

            // Billboard draws are identified by the is_billboard flag on the draw call
            let is_billboard = draw_call.is_billboard && billboard_pipeline.is_some();

            let target_variant = if is_skinned {
                PipelineVariant::Skinned
            } else if is_billboard {
                PipelineVariant::Billboard
            } else {
                PipelineVariant::Regular
            };

            // Switch pipeline if needed
            if target_variant != current_variant {
                let (new_pipe, new_layout) = match target_variant {
                    PipelineVariant::Skinned => (
                        skinned_pipeline.expect("skinned pipeline required after is_skinned check"),
                        skinned_layout.expect("skinned layout required after is_skinned check"),
                    ),
                    PipelineVariant::Billboard => (
                        billboard_pipeline
                            .expect("billboard pipeline required after is_billboard check"),
                        billboard_layout
                            .expect("billboard layout required after is_billboard check"),
                    ),
                    PipelineVariant::Regular => (pipeline, layout),
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

                // Set 1 binding depends on pipeline variant:
                // - Billboard: bindless textures for alpha discard
                // - Skinned: empty descriptor set (pipeline declares empty layout at Set 1)
                // - Regular with bind_textures: bindless textures
                // - Regular without bind_textures: may or may not have Set 1 (handled by extra_sets)
                match target_variant {
                    PipelineVariant::Billboard => {
                        let bindless_ds = renderer.bindless_manager.descriptor_set().vk();
                        cmd.bind_descriptor_sets(new_layout, 1, &[bindless_ds], &[]);
                    }
                    PipelineVariant::Skinned => {
                        let empty_ds = renderer.empty_descriptor_set(frame_idx);
                        cmd.bind_descriptor_sets(new_layout, 1, &[empty_ds], &[]);
                    }
                    PipelineVariant::Regular => {
                        if descriptors.bind_textures {
                            let bindless_ds = renderer.bindless_manager.descriptor_set().vk();
                            cmd.bind_descriptor_sets(new_layout, 1, &[bindless_ds], &[]);
                        }
                    }
                }

                // Bind extra sets — use skinned-specific sets when in skinned variant,
                // otherwise use the regular extra sets.
                let active_extra_sets = if target_variant == PipelineVariant::Skinned
                    && !descriptors.skinned_extra_sets.is_empty()
                {
                    &descriptors.skinned_extra_sets
                } else {
                    &descriptors.extra_sets
                };
                for &(set, ds) in active_extra_sets {
                    cmd.bind_descriptor_sets(new_layout, set, &[ds], &[]);
                }
                current_variant = target_variant;
            }

            // Bind skeleton descriptor set for skinned meshes
            if is_skinned {
                let skel_layout = skinned_layout
                    .expect("skinned layout required for skeleton descriptor binding");
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
            bind_vertex_buffers(cmd, mesh, is_skinned, is_billboard);

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

/// Bind vertex buffers for position-only mode with optional skinning or billboard attributes.
fn bind_vertex_buffers(
    cmd: &CommandBuffer,
    mesh: &crate::renderer::registry::MeshAsset,
    is_skinned: bool,
    is_billboard: bool,
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
    } else if is_billboard {
        // PBR vertex layout: position(0), normal(1), tangent(2), uv(3)
        let normal_buf = mesh
            .get_attribute_buffer(AttributeType::Normal)
            .map(|vb| vb.object())
            .unwrap_or(vk::Buffer::null());
        let tangent_buf = mesh
            .get_attribute_buffer(AttributeType::Tangent)
            .map(|vb| vb.object())
            .unwrap_or(vk::Buffer::null());
        let uv_buf = mesh
            .get_attribute_buffer(AttributeType::TexCoord0)
            .map(|vb| vb.object())
            .unwrap_or(vk::Buffer::null());
        cmd.bind_vertex_buffers_at_locations(&[
            (0, pos_buf),
            (1, normal_buf),
            (2, tangent_buf),
            (3, uv_buf),
        ]);
    } else {
        cmd.bind_vertex_buffers_at_locations(&[(0, pos_buf)]);
    }
}
