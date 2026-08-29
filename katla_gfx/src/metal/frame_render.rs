//! Metal command encoding driven directly by compiled render-graph pass records.
//!
//! Every record keeps its stable [`PassId`](crate::render_graph::PassId), consumes
//! only submissions addressed to that pass, and is encoded in the graph compiler's
//! canonical order. Metal-specific encoder implementations remain private here.

use std::collections::{HashMap, HashSet};
use std::mem;

use objc2_metal::{MTLCommandBuffer, MTLRenderCommandEncoder};

use crate::backend::command::{
    ColorAttachmentInfo, DepthAttachmentInfo, GpuCommandBuffer, GpuRenderEncoder, IndexType,
    RenderPassInfo, ShaderStages,
};
use crate::backend::resource::GpuBuffer;
use crate::error::RendererError;
use crate::render_graph::{PassExecutionData, PassId, PassKind};
use crate::render_pass::{ClearValue, LoadOp, StoreOp};
use crate::renderer::types::{DrawList, FrameUniforms, UIDrawList};
use crate::texture::ImageFormat;

use super::command_buffer::MetalCommandBuffer;
use super::execution_plan::{MetalExecutionPlan, MetalPassRecord};
use super::metal_renderer::MetalRenderer;
use super::texture::{MetalTexture, MetalTextureView};

/// Canvas clear color that appears as #1E1E1E on an sRGB framebuffer.
const CANVAS_CLEAR_COLOR: (f64, f64, f64, f64) = (0.013, 0.013, 0.013, 1.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MetalPassOutcome {
    Encoded,
    SkippedNoWork,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MetalPassTrace {
    pass_id: PassId,
    name: String,
    kind: PassKind,
    outcome: MetalPassOutcome,
}

struct FrameEncodingState {
    drawable_view: MetalTextureView,
    drawable_written: bool,
    depth_prepass_ran: bool,
    tonemap_ran: bool,
    drawable_width: f32,
    drawable_height: f32,
    viewport_x: f32,
    viewport_y: f32,
    viewport_width: f32,
    viewport_height: f32,
}

fn tonemap_viewport(
    drawable_height: f32,
    panel_x: f32,
    panel_y: f32,
    panel_width: f32,
    panel_height: f32,
    offscreen: bool,
) -> (f32, f32, f32, f32) {
    if offscreen {
        (0.0, 0.0, panel_width, panel_height)
    } else {
        (
            panel_x,
            drawable_height - (panel_y + panel_height),
            panel_width,
            panel_height,
        )
    }
}

fn plan_requires_scene_depth(plan: &MetalExecutionPlan) -> bool {
    plan.passes().iter().any(|record| {
        matches!(
            record.kind,
            PassKind::DepthPrepass | PassKind::Geometry | PassKind::ObjectId | PassKind::Outline
        )
    })
}

fn has_later_kind(plan: &MetalExecutionPlan, position: usize, kind: PassKind) -> bool {
    plan.passes()
        .iter()
        .skip(position + 1)
        .any(|record| record.kind == kind)
}

fn has_later_ui_work(plan: &MetalExecutionPlan, position: usize, ui_work: &HashSet<usize>) -> bool {
    plan.passes()
        .iter()
        .skip(position + 1)
        .any(|record| record.kind == PassKind::Ui && ui_work.contains(&record.pass_index))
}

fn merge_draw_lists(data: &PassExecutionData) -> DrawList {
    let mut merged = DrawList::new();
    for draw_list in &data.draw_lists {
        for draw in &draw_list.draws {
            merged.push(draw.clone());
        }
    }
    merged
}

fn single_ui_draw_list(
    record: &MetalPassRecord,
    data: &PassExecutionData,
) -> Result<Option<UIDrawList>, RendererError> {
    match data.ui_draw_lists.as_slice() {
        [] => Ok(None),
        [draw_list] => Ok(Some(draw_list.clone())),
        lists => Err(RendererError::InvalidOperation(format!(
            "Metal UI pass '{}' ({:?}) received {} UI draw lists; submit one composed list per PassId",
            record.name,
            record.pass_id,
            lists.len()
        ))),
    }
}

impl MetalRenderer {
    pub(crate) fn render_frame(
        &mut self,
        plan: &MetalExecutionPlan,
        mut pending: HashMap<usize, PassExecutionData>,
    ) -> Result<(), RendererError> {
        let scheduled_passes = plan
            .passes()
            .iter()
            .map(|record| record.pass_index)
            .collect::<HashSet<_>>();
        if let Some(pass_index) = pending
            .keys()
            .copied()
            .find(|pass_index| !scheduled_passes.contains(pass_index))
        {
            return Err(RendererError::InvalidOperation(format!(
                "Metal received submissions for pass index {pass_index}, which is absent from the compiled execution plan"
            )));
        }
        for record in plan
            .passes()
            .iter()
            .filter(|record| record.kind == PassKind::Ui)
        {
            if let Some(data) = pending.get(&record.pass_index)
                && data.ui_draw_lists.len() > 1
            {
                return Err(RendererError::InvalidOperation(format!(
                    "Metal UI pass '{}' ({:?}) received {} UI draw lists; submit one composed list per PassId",
                    record.name,
                    record.pass_id,
                    data.ui_draw_lists.len()
                )));
            }
        }

        if plan_requires_scene_depth(plan) && self.depth_stencil_view.is_none() {
            self.current_drawable_texture = None;
            return Err(RendererError::InvalidOperation(
                "Metal execution plan requires a depth-stencil target".into(),
            ));
        }

        let drawable_texture = self
            .current_drawable_texture
            .take()
            .ok_or_else(|| RendererError::InvalidOperation("No drawable texture".into()))?;
        let drawable_view = MetalTextureView::new(
            drawable_texture.clone(),
            MetalTexture::new(drawable_texture, ImageFormat::B8G8R8A8Srgb),
        );

        let mut cmd_buffer = self.context.create_command_buffer();
        cmd_buffer.begin();
        {
            let label = objc2_foundation::NSString::from_str("render_graph_frame");
            cmd_buffer.inner.setLabel(Some(&label));
        }

        // Encode staged texture uploads before any consumer pass.
        if self.texture_uploads.has_pending() {
            use crate::backend::command::GpuBlitEncoder;
            let mut blit = cmd_buffer.begin_blit_pass();
            self.texture_uploads.encode_into(&mut blit);
            blit.end_encoding();
        }

        let drawable_width = self.drawable_size.width as f32;
        let drawable_height = self.drawable_size.height as f32;
        let panel = self.viewport_panel_rect;
        let viewport_x = panel.map_or(0.0, |rect| rect.min[0]);
        let viewport_y = panel.map_or(0.0, |rect| rect.min[1]);
        let viewport_width = panel.map_or(drawable_width, |rect| rect.width());
        let viewport_height = panel.map_or(drawable_height, |rect| rect.height());

        let ui_work = plan
            .passes()
            .iter()
            .filter(|record| record.kind == PassKind::Ui)
            .filter_map(|record| {
                pending
                    .get(&record.pass_index)
                    .is_some_and(|data| data.ui_draw_lists.iter().any(|list| !list.is_empty()))
                    .then_some(record.pass_index)
            })
            .collect::<HashSet<_>>();

        let mut state = FrameEncodingState {
            drawable_view,
            drawable_written: false,
            depth_prepass_ran: false,
            tonemap_ran: false,
            drawable_width,
            drawable_height,
            viewport_x,
            viewport_y,
            viewport_width,
            viewport_height,
        };
        let mut trace = Vec::with_capacity(plan.passes().len());

        log::debug!("Metal execution plan: {}", plan.trace().join(" -> "));

        for (position, record) in plan.passes().iter().enumerate() {
            let data = pending.remove(&record.pass_index).unwrap_or_default();
            let encoded = match record.kind {
                PassKind::Shadow => self.encode_shadow_record(&mut cmd_buffer, &state, &data)?,
                PassKind::DepthPrepass => {
                    let encoded =
                        self.encode_depth_prepass_record(&mut cmd_buffer, &state, &data)?;
                    state.depth_prepass_ran |= encoded;
                    encoded
                }
                PassKind::Geometry => self.encode_geometry_record(
                    &mut cmd_buffer,
                    &mut state,
                    &data,
                    has_later_kind(plan, position, PassKind::Fullscreen),
                )?,
                PassKind::ObjectId => {
                    self.encode_object_id_record(&mut cmd_buffer, &state, &data)?
                }
                PassKind::Outline => self.encode_outline_record(&mut cmd_buffer, &state, &data)?,
                PassKind::Fullscreen => {
                    let encoded = self.encode_fullscreen_record(
                        &mut cmd_buffer,
                        &mut state,
                        has_later_ui_work(plan, position, &ui_work),
                    )?;
                    state.tonemap_ran |= encoded;
                    encoded
                }
                PassKind::Ui => {
                    let ui_draw_list = single_ui_draw_list(record, &data)?;
                    self.encode_ui_record(&mut cmd_buffer, &mut state, ui_draw_list.as_ref())?
                }
                PassKind::Particles | PassKind::StencilIndicator | PassKind::Compositing => {
                    unreachable!("unsupported records are rejected while compiling the Metal plan")
                }
            };

            trace.push(MetalPassTrace {
                pass_id: record.pass_id,
                name: record.name.clone(),
                kind: record.kind,
                outcome: if encoded {
                    MetalPassOutcome::Encoded
                } else {
                    MetalPassOutcome::SkippedNoWork
                },
            });
        }

        if !state.drawable_written {
            let clear_pass_info = RenderPassInfo {
                color_attachments: vec![ColorAttachmentInfo {
                    view: state.drawable_view.clone(),
                    load_op: LoadOp::Clear,
                    store_op: StoreOp::Store,
                    clear_value: ClearValue::OPAQUE_BLACK,
                }],
                depth_attachment: None,
            };
            let encoder = cmd_buffer.begin_render_pass(clear_pass_info);
            encoder.end_encoding();
        }

        log::debug!("Metal encoder trace: {trace:?}");

        cmd_buffer.end();
        self.context.surface.present(&cmd_buffer.inner);
        self.last_command_buffer = Some(cmd_buffer.inner.clone());
        cmd_buffer.submit(&self.context);

        Ok(())
    }

    fn encode_shadow_record(
        &self,
        cmd_buffer: &mut MetalCommandBuffer,
        _state: &FrameEncodingState,
        data: &PassExecutionData,
    ) -> Result<bool, RendererError> {
        let draw_list = merge_draw_lists(data);
        if draw_list.draws.is_empty() {
            return Ok(false);
        }

        let shadow_pipeline = self.shadow.pipeline().ok_or_else(|| {
            RendererError::InvalidOperation(
                "Metal Shadow record requires an initialized shadow pipeline".into(),
            )
        })?;
        let shadow_map_view = self.shadow.shadow_map_view().ok_or_else(|| {
            RendererError::InvalidOperation(
                "Metal Shadow record requires a shadow-map target".into(),
            )
        })?;
        let frame_buf = self.current_frame_uniform_buffer().ok_or_else(|| {
            RendererError::InvalidOperation("Metal Shadow record requires frame uniforms".into())
        })?;
        let object_buf = self.current_object_storage_buffer().ok_or_else(|| {
            RendererError::InvalidOperation("Metal Shadow record requires object storage".into())
        })?;
        let shadow_buf = self.shadow_cascade_buffer.as_ref().ok_or_else(|| {
            RendererError::InvalidOperation("Metal Shadow record requires cascade data".into())
        })?;
        let shadow_resolution = self.shadow.shadow_resolution();

        super::shadow::render_cascades(
            cmd_buffer,
            shadow_pipeline,
            self.shadow.pipeline_skinned(),
            Some(&self.skeletons),
            shadow_map_view,
            shadow_resolution,
            frame_buf,
            object_buf,
            shadow_buf,
            self.buffer_sizes_buffer.as_ref(),
            self.shadow.cascade_count(),
            &self.meshes,
            &self.materials,
            &draw_list,
        );

        Ok(true)
    }

    fn encode_depth_prepass_record(
        &self,
        cmd_buffer: &mut MetalCommandBuffer,
        state: &FrameEncodingState,
        data: &PassExecutionData,
    ) -> Result<bool, RendererError> {
        let draw_list = merge_draw_lists(data);
        if draw_list.draws.is_empty() {
            return Ok(false);
        }

        let pipeline = self.depth_prepass.pipeline().ok_or_else(|| {
            RendererError::InvalidOperation(
                "Metal DepthPrepass record requires an initialized pipeline".into(),
            )
        })?;
        let depth_view = self.depth_stencil_view.as_ref().ok_or_else(|| {
            RendererError::InvalidOperation(
                "Metal DepthPrepass record requires a depth-stencil target".into(),
            )
        })?;
        let frame_buf = self.current_frame_uniform_buffer().ok_or_else(|| {
            RendererError::InvalidOperation(
                "Metal DepthPrepass record requires frame uniforms".into(),
            )
        })?;
        let object_buf = self.current_object_storage_buffer().ok_or_else(|| {
            RendererError::InvalidOperation(
                "Metal DepthPrepass record requires object storage".into(),
            )
        })?;

        super::depth_prepass::render_depth_prepass(
            cmd_buffer,
            pipeline,
            self.depth_prepass.pipeline_skinned(),
            self.depth_prepass.pipeline_billboard(),
            depth_view,
            state.viewport_width as u32,
            state.viewport_height as u32,
            frame_buf,
            object_buf,
            &self.meshes,
            &self.materials,
            &draw_list,
            &self.skeletons,
            self.bindless_manager.argument_buffer(),
            self.shared_sampler.as_ref(),
        );
        Ok(true)
    }

    fn encode_geometry_record(
        &self,
        cmd_buffer: &mut MetalCommandBuffer,
        state: &mut FrameEncodingState,
        data: &PassExecutionData,
        post_process_later: bool,
    ) -> Result<bool, RendererError> {
        let draw_list = merge_draw_lists(data);
        let depth_attachment = self
            .depth_stencil_view
            .as_ref()
            .map(|view| DepthAttachmentInfo {
                view: view.clone(),
                load_op: if state.depth_prepass_ran {
                    LoadOp::Load
                } else {
                    LoadOp::Clear
                },
                store_op: StoreOp::Store,
                clear_value: ClearValue::depth_stencil(0.0, 0),
                format: ImageFormat::D32SfloatS8Uint,
            });

        let color_view = if post_process_later {
            self.geometry_hdr_view.clone().ok_or_else(|| {
                RendererError::InvalidOperation(
                    "Metal Geometry record feeding a later Fullscreen record requires an HDR target"
                        .into(),
                )
            })?
        } else {
            state.drawable_written = true;
            state.drawable_view.clone()
        };

        let clear_value = if post_process_later {
            ClearValue::OPAQUE_BLACK
        } else {
            ClearValue::color(
                CANVAS_CLEAR_COLOR.0 as f32,
                CANVAS_CLEAR_COLOR.1 as f32,
                CANVAS_CLEAR_COLOR.2 as f32,
                CANVAS_CLEAR_COLOR.3 as f32,
            )
        };
        let pass_info = RenderPassInfo {
            color_attachments: vec![ColorAttachmentInfo {
                view: color_view,
                load_op: LoadOp::Clear,
                store_op: StoreOp::Store,
                clear_value,
            }],
            depth_attachment,
        };

        let mut encoder = cmd_buffer.begin_render_pass(pass_info);
        Self::bind_common_resources(self, &mut encoder);

        if let Some(ref sky_pipeline) = self.sky_pipeline {
            if let Some(ref dummy_vertex_buffer) = self.dummy_vertex_buffer {
                encoder.bind_vertex_buffer(dummy_vertex_buffer, 0, 10);
            }
            encoder.bind_graphics_pipeline(sky_pipeline);
            encoder.draw(3, 1, 0, 0);
        }
        if !draw_list.draws.is_empty() {
            Self::draw_objects(self, &mut encoder, &draw_list);
        }
        encoder.end_encoding();

        Ok(true)
    }

    fn encode_outline_record(
        &self,
        cmd_buffer: &mut MetalCommandBuffer,
        state: &FrameEncodingState,
        data: &PassExecutionData,
    ) -> Result<bool, RendererError> {
        let draw_list = merge_draw_lists(data);
        if draw_list.draws.is_empty() {
            return Ok(false);
        }

        let color_view = self.geometry_hdr_view.as_ref().ok_or_else(|| {
            RendererError::InvalidOperation(
                "Metal Outline record requires an HDR color target".into(),
            )
        })?;
        let depth_view = self.depth_stencil_view.as_ref().ok_or_else(|| {
            RendererError::InvalidOperation(
                "Metal Outline record requires a depth-stencil target".into(),
            )
        })?;
        let frame_buf = self.current_frame_uniform_buffer().ok_or_else(|| {
            RendererError::InvalidOperation("Metal Outline record requires frame uniforms".into())
        })?;
        let object_buf = self.current_object_storage_buffer().ok_or_else(|| {
            RendererError::InvalidOperation("Metal Outline record requires object storage".into())
        })?;
        let width = state.viewport_width as u32;
        let height = state.viewport_height as u32;
        let mut encoded = false;

        if let Some(pipeline) = self.outline.stencil_mark_pipeline() {
            super::outline::render_stencil_mark(
                cmd_buffer,
                pipeline,
                self.outline.stencil_mark_skinned_pipeline(),
                color_view,
                depth_view,
                width,
                height,
                frame_buf,
                object_buf,
                &self.meshes,
                &self.materials,
                &draw_list,
                &self.skeletons,
            );
            encoded = true;
        }
        if let Some(pipeline) = self.outline.outline_draw_pipeline() {
            super::outline::render_outline(
                cmd_buffer,
                pipeline,
                self.outline.outline_draw_skinned_pipeline(),
                color_view,
                depth_view,
                width,
                height,
                frame_buf,
                object_buf,
                &self.meshes,
                &self.materials,
                &draw_list,
                &self.skeletons,
            );
            encoded = true;
        }

        Ok(encoded)
    }

    fn encode_object_id_record(
        &self,
        cmd_buffer: &mut MetalCommandBuffer,
        state: &FrameEncodingState,
        data: &PassExecutionData,
    ) -> Result<bool, RendererError> {
        let draw_list = merge_draw_lists(data);
        if draw_list.draws.is_empty() {
            return Ok(false);
        }

        let Some(pipeline) = self.picking.pipeline() else {
            return Err(RendererError::InvalidOperation(
                "Metal ObjectId record requires an initialized picking pipeline".into(),
            ));
        };
        let id_view = self.picking.object_id_texture().ok_or_else(|| {
            RendererError::InvalidOperation(
                "Metal ObjectId record requires an object-ID target".into(),
            )
        })?;
        let depth_view = self.depth_stencil_view.as_ref().ok_or_else(|| {
            RendererError::InvalidOperation(
                "Metal ObjectId record requires a depth-stencil target".into(),
            )
        })?;
        let frame_buf = self.current_frame_uniform_buffer().ok_or_else(|| {
            RendererError::InvalidOperation("Metal ObjectId record requires frame uniforms".into())
        })?;
        let object_buf = self.current_object_storage_buffer().ok_or_else(|| {
            RendererError::InvalidOperation("Metal ObjectId record requires object storage".into())
        })?;

        super::picking::render_object_id_pass(
            cmd_buffer,
            pipeline,
            self.picking.pipeline_skinned(),
            id_view,
            depth_view,
            state.viewport_width as u32,
            state.viewport_height as u32,
            frame_buf,
            object_buf,
            &self.meshes,
            &self.materials,
            &draw_list,
            &self.skeletons,
        );
        Ok(true)
    }

    fn encode_fullscreen_record(
        &mut self,
        cmd_buffer: &mut MetalCommandBuffer,
        state: &mut FrameEncodingState,
        composite_to_later_ui: bool,
    ) -> Result<bool, RendererError> {
        let tonemap_pipeline = self.tonemap_pipeline.as_ref().ok_or_else(|| {
            RendererError::InvalidOperation(
                "Metal Fullscreen record requires an initialized tonemap pipeline".into(),
            )
        })?;
        let hdr_view = self.geometry_hdr_view.clone().ok_or_else(|| {
            RendererError::InvalidOperation(
                "Metal Fullscreen record requires an HDR input target".into(),
            )
        })?;

        if let Some(hdr_slot) = self.geometry_hdr_bindless_slot {
            self.frame_uniforms.tonemap[3] = hdr_slot as f32;
            if let Some(frame_buf) = self.current_frame_uniform_buffer() {
                let ptr = frame_buf.map();
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        &self.frame_uniforms as *const FrameUniforms as *const u8,
                        ptr,
                        mem::size_of::<FrameUniforms>(),
                    );
                }
                frame_buf.unmap();
            }
        }

        let (target, load_op, offscreen) = if composite_to_later_ui {
            let target = self.tonemap_output_view.clone().ok_or_else(|| {
                RendererError::InvalidOperation(
                    "Metal Fullscreen record feeding a later UI record requires an output texture"
                        .into(),
                )
            })?;
            (target, LoadOp::Clear, true)
        } else {
            state.drawable_written = true;
            (state.drawable_view.clone(), LoadOp::Clear, false)
        };
        let (x, y, width, height) = tonemap_viewport(
            state.drawable_height,
            state.viewport_x,
            state.viewport_y,
            state.viewport_width,
            state.viewport_height,
            offscreen,
        );

        let pass_info = RenderPassInfo {
            color_attachments: vec![ColorAttachmentInfo {
                view: target,
                load_op,
                store_op: StoreOp::Store,
                clear_value: ClearValue::color(
                    CANVAS_CLEAR_COLOR.0 as f32,
                    CANVAS_CLEAR_COLOR.1 as f32,
                    CANVAS_CLEAR_COLOR.2 as f32,
                    CANVAS_CLEAR_COLOR.3 as f32,
                ),
            }],
            depth_attachment: None,
        };
        let mut encoder = cmd_buffer.begin_render_pass(pass_info);
        encoder.set_viewport(x, y, width, height, 0.0, 1.0);
        encoder.set_scissor(x as u32, y as u32, width as u32, height as u32);

        if let Some(frame_buf) = self.current_frame_uniform_buffer() {
            encoder.bind_storage_buffer(frame_buf, 0, 0, ShaderStages::VERTEX_FRAGMENT);
        }
        if let Some(object_buf) = self.current_object_storage_buffer() {
            encoder.bind_storage_buffer(object_buf, 0, 1, ShaderStages::VERTEX_FRAGMENT);
        }
        if let Some(argument_buffer) = self.bindless_manager.argument_buffer() {
            unsafe {
                encoder
                    .inner
                    .setVertexBuffer_offset_atIndex(Some(argument_buffer), 0, 9);
                encoder
                    .inner
                    .setFragmentBuffer_offset_atIndex(Some(argument_buffer), 0, 9);
            }
            encoder.use_buffer(
                argument_buffer,
                objc2_metal::MTLResourceUsage::Read,
                objc2_metal::MTLRenderStages::Fragment,
            );
        }
        if let Some(ref sampler) = self.shared_sampler {
            unsafe {
                encoder
                    .inner
                    .setFragmentSamplerState_atIndex(Some(&sampler.inner), 0);
            }
        }
        if let Some(ref buffer_sizes) = self.buffer_sizes_buffer {
            encoder.bind_storage_buffer(buffer_sizes, 0, 8, ShaderStages::VERTEX_FRAGMENT);
        }
        if let Some(ref dummy_vertex_buffer) = self.dummy_vertex_buffer {
            encoder.bind_vertex_buffer(dummy_vertex_buffer, 0, 10);
        }
        encoder.use_texture(
            &hdr_view.inner,
            objc2_metal::MTLResourceUsage::Read,
            objc2_metal::MTLRenderStages::Fragment,
        );
        encoder.bind_graphics_pipeline(tonemap_pipeline);
        encoder.draw(3, 1, 0, 0);

        if let Some(ref fence) = self.tonemap_fence {
            encoder
                .inner
                .updateFence_afterStages(fence, objc2_metal::MTLRenderStages::Fragment);
        }
        encoder.end_encoding();
        Ok(true)
    }

    fn encode_ui_record(
        &mut self,
        cmd_buffer: &mut MetalCommandBuffer,
        state: &mut FrameEncodingState,
        draw_list: Option<&UIDrawList>,
    ) -> Result<bool, RendererError> {
        let Some(draw_list) = draw_list.filter(|draw_list| !draw_list.is_empty()) else {
            return Ok(false);
        };

        self.ui_renderer
            .upload_draw_list(&self.context, draw_list)
            .map_err(|error| {
                RendererError::InvalidOperation(format!(
                    "Metal UI record failed to upload its draw list: {error}"
                ))
            })?;
        let material_handle = self.ui_renderer.ui_material().ok_or_else(|| {
            RendererError::InvalidOperation("Metal UI record has no material".into())
        })?;
        let pipeline = self
            .materials
            .get(material_handle.index())
            .and_then(|material| material.pipeline.as_ref())
            .ok_or_else(|| {
                RendererError::InvalidOperation(
                    "Metal UI record material has no graphics pipeline".into(),
                )
            })?;

        let load_op = if state.drawable_written {
            LoadOp::Load
        } else {
            LoadOp::Clear
        };
        state.drawable_written = true;
        let pass_info = RenderPassInfo {
            color_attachments: vec![ColorAttachmentInfo {
                view: state.drawable_view.clone(),
                load_op,
                store_op: StoreOp::Store,
                clear_value: ClearValue::color(
                    CANVAS_CLEAR_COLOR.0 as f32,
                    CANVAS_CLEAR_COLOR.1 as f32,
                    CANVAS_CLEAR_COLOR.2 as f32,
                    CANVAS_CLEAR_COLOR.3 as f32,
                ),
            }],
            depth_attachment: None,
        };
        let mut encoder = cmd_buffer.begin_render_pass(pass_info);

        if state.tonemap_ran
            && let Some(ref fence) = self.tonemap_fence
        {
            encoder
                .inner
                .waitForFence_beforeStages(fence, objc2_metal::MTLRenderStages::Fragment);
        }
        encoder.set_viewport(
            0.0,
            0.0,
            state.drawable_width,
            state.drawable_height,
            0.0,
            1.0,
        );
        encoder.bind_graphics_pipeline(pipeline);

        if let Some(argument_buffer) = self.bindless_manager.argument_buffer() {
            unsafe {
                encoder
                    .inner
                    .setVertexBuffer_offset_atIndex(Some(argument_buffer), 0, 9);
                encoder
                    .inner
                    .setFragmentBuffer_offset_atIndex(Some(argument_buffer), 0, 9);
            }
            encoder.use_buffer(
                argument_buffer,
                objc2_metal::MTLResourceUsage::Read,
                objc2_metal::MTLRenderStages::Vertex | objc2_metal::MTLRenderStages::Fragment,
            );
            for texture in self.bindless_manager.registered_textures() {
                encoder.use_texture(
                    texture,
                    objc2_metal::MTLResourceUsage::Read,
                    objc2_metal::MTLRenderStages::Vertex | objc2_metal::MTLRenderStages::Fragment,
                );
            }
        }
        if let Some(ref sampler) = self.shared_sampler {
            unsafe {
                encoder
                    .inner
                    .setFragmentSamplerState_atIndex(Some(&sampler.inner), 0);
            }
        }
        if let Some(vertex_buffer) = self.ui_renderer.vertex_buffer() {
            encoder.bind_vertex_buffer(vertex_buffer, 0, 10);
        }
        if let Some(index_buffer) = self.ui_renderer.index_buffer() {
            encoder.bind_index_buffer(index_buffer, 0, IndexType::Uint32);
        }
        if let Some(ref buffer_sizes) = self.buffer_sizes_buffer {
            encoder.bind_storage_buffer(buffer_sizes, 0, 8, ShaderStages::VERTEX_FRAGMENT);
        }
        self.ui_renderer.render_ui_commands(
            &mut encoder,
            draw_list,
            pipeline,
            self.drawable_size.width,
            self.drawable_size.height,
        );
        encoder.end_encoding();
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::{has_later_kind, plan_requires_scene_depth, tonemap_viewport};
    use crate::metal::execution_plan::MetalExecutionPlan;
    use crate::render_graph::PassKind;

    #[test]
    fn ui_only_plan_does_not_require_scene_depth() {
        let plan = MetalExecutionPlan::for_test(&[PassKind::Ui]);
        assert!(!plan_requires_scene_depth(&plan));
    }

    #[test]
    fn object_id_plan_requires_scene_depth() {
        let plan = MetalExecutionPlan::for_test(&[PassKind::ObjectId]);
        assert!(plan_requires_scene_depth(&plan));
    }

    #[test]
    fn later_post_process_is_position_sensitive() {
        let plan = MetalExecutionPlan::for_test(&[
            PassKind::Fullscreen,
            PassKind::Geometry,
            PassKind::Fullscreen,
        ]);
        assert!(has_later_kind(&plan, 1, PassKind::Fullscreen));
        assert!(!has_later_kind(&plan, 2, PassKind::Fullscreen));
    }

    #[test]
    fn offscreen_tonemap_uses_local_target_coordinates() {
        assert_eq!(
            tonemap_viewport(1080.0, 100.0, 200.0, 640.0, 360.0, true),
            (0.0, 0.0, 640.0, 360.0)
        );
    }

    #[test]
    fn drawable_tonemap_flips_panel_y_for_metal() {
        assert_eq!(
            tonemap_viewport(1080.0, 100.0, 200.0, 640.0, 360.0, false),
            (100.0, 520.0, 640.0, 360.0)
        );
    }
}
