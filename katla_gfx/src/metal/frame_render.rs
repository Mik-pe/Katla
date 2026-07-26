//! Metal frame rendering driven by a validated semantic frame schedule.
//!
//! Encoder implementations remain backend-specific, while pass presence and order
//! come from the compiled render graph schedule.

use std::mem;

use objc2_metal::{MTLCommandBuffer, MTLRenderCommandEncoder};

use crate::backend::command::{
    ColorAttachmentInfo, DepthAttachmentInfo, GpuCommandBuffer, GpuRenderEncoder, IndexType,
    RenderPassInfo, ShaderStages,
};
use crate::backend::resource::GpuBuffer;
use crate::error::RendererError;
use crate::render_graph::PassKind;
use crate::render_pass::{ClearValue, LoadOp, StoreOp};
use crate::renderer::types::FrameUniforms;
use crate::texture::ImageFormat;

use super::frame_schedule::MetalFrameSchedule;
use super::metal_renderer::MetalRenderer;
use super::texture::{MetalTexture, MetalTextureView};

/// Canvas clear color — appears as #1E1E1E on an sRGB framebuffer.
///
/// The drawable is BGRA8Unorm_sRGB, so Metal interprets clear values as linear.
/// sRGB #1E1E1E (30/255 ≈ 0.118) corresponds to linear ≈ 0.013.
const CANVAS_CLEAR_COLOR: (f64, f64, f64, f64) = (0.013, 0.013, 0.013, 1.0);

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

impl MetalRenderer {
    pub(crate) fn render_frame(
        &mut self,
        schedule: &MetalFrameSchedule,
    ) -> Result<(), RendererError> {
        if self.depth_stencil_view.is_none() {
            log::error!("METAL render_frame: EARLY RETURN — no depth_stencil_view");
            self.current_drawable_texture = None;
            return Ok(());
        }

        let mut drawable_written = false;

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
            let label = objc2_foundation::NSString::from_str("main_render");
            cmd_buffer.inner.setLabel(Some(&label));
        }

        let width = self.drawable_size.width as f32;
        let height = self.drawable_size.height as f32;

        // Viewport panel rect in physical pixels — restrict 3D scene to this area.
        let vp = self.viewport_panel_rect;
        let vp_x = vp.map_or(0.0, |r| r.min[0]);
        let vp_y = vp.map_or(0.0, |r| r.min[1]);
        let vp_w = vp.map_or(width, |r| r.width());
        let vp_h = vp.map_or(height, |r| r.height());
        let ui_will_composite_viewport = schedule.contains(PassKind::Ui)
            && self
                .pending_ui_draw_list
                .as_ref()
                .is_some_and(|draw_list| !draw_list.is_empty())
            && self.tonemap_output_view.is_some();

        // =========================================================================
        // Pass 0: Shadow cascade rendering → shadow map texture
        // =========================================================================
        if schedule.contains(PassKind::Shadow)
            && let Some(shadow_draw_list) = self.pending_shadow_draw_list.take()
            && !shadow_draw_list.draws.is_empty()
            && let (Some(shadow_pipeline), Some(shadow_map_view)) =
                (self.shadow.pipeline(), self.shadow.shadow_map_view())
        {
            let shadow_res = self.shadow.shadow_resolution();
            let frame_buf = self.current_frame_uniform_buffer().unwrap();
            let object_buf = self.current_object_storage_buffer().unwrap();
            let shadow_buf = self.shadow_cascade_buffer.as_ref().unwrap();
            for cascade_idx in 0..self.shadow.cascade_count() as usize {
                super::shadow::render_cascade(
                    &mut cmd_buffer,
                    shadow_pipeline,
                    shadow_map_view,
                    shadow_res,
                    frame_buf,
                    object_buf,
                    shadow_buf,
                    cascade_idx as u32,
                    &self.meshes,
                    &self.materials,
                    &shadow_draw_list,
                );
            }
        }

        // =========================================================================
        // Pass 1: Depth prepass → shared scene depth
        // =========================================================================
        let mut depth_prepass_ran = false;
        if schedule.contains(PassKind::DepthPrepass)
            && let Some(depth_draw_list) = self.pending_depth_prepass_draw_list.take()
            && !depth_draw_list.draws.is_empty()
        {
            let depth_pipeline = self.depth_prepass.pipeline().ok_or_else(|| {
                RendererError::InvalidOperation(
                    "Metal depth prepass is scheduled but its pipeline is not initialized".into(),
                )
            })?;
            let depth_view = self.depth_stencil_view.as_ref().ok_or_else(|| {
                RendererError::InvalidOperation(
                    "Metal depth prepass is scheduled without a depth-stencil target".into(),
                )
            })?;
            let frame_buf = self.current_frame_uniform_buffer().ok_or_else(|| {
                RendererError::InvalidOperation(
                    "Metal depth prepass is scheduled without frame uniforms".into(),
                )
            })?;
            let object_buf = self.current_object_storage_buffer().ok_or_else(|| {
                RendererError::InvalidOperation(
                    "Metal depth prepass is scheduled without object storage".into(),
                )
            })?;

            super::depth_prepass::render_depth_prepass(
                &mut cmd_buffer,
                depth_pipeline,
                self.depth_prepass.pipeline_skinned(),
                self.depth_prepass.pipeline_billboard(),
                depth_view,
                vp_w as u32,
                vp_h as u32,
                frame_buf,
                object_buf,
                &self.meshes,
                &self.materials,
                &depth_draw_list,
                &self.skeletons,
            );
            depth_prepass_ran = true;
        }

        // =========================================================================
        // Pass 2: Geometry (sky + PBR) → HDR intermediate texture
        // =========================================================================
        let has_tonemap = schedule.contains(PassKind::Fullscreen)
            && self.tonemap_pipeline.is_some()
            && self.geometry_hdr_view.is_some();
        let draw_list = self.pending_draw_list.take();
        let picking_draw_list = draw_list.clone();

        if has_tonemap {
            let geometry_hdr_view = self.geometry_hdr_view.as_ref().unwrap();

            let depth_attachment =
                self.depth_stencil_view
                    .as_ref()
                    .map(|view| DepthAttachmentInfo {
                        view: view.clone(),
                        load_op: if depth_prepass_ran {
                            LoadOp::Load
                        } else {
                            LoadOp::Clear
                        },
                        store_op: StoreOp::Store,
                        clear_value: ClearValue::depth_stencil(0.0, 0),
                        format: ImageFormat::D32SfloatS8Uint,
                    });

            let geometry_pass_info = RenderPassInfo {
                color_attachments: vec![ColorAttachmentInfo {
                    view: geometry_hdr_view.clone(),
                    load_op: LoadOp::Clear,
                    store_op: StoreOp::Store,
                    clear_value: ClearValue::OPAQUE_BLACK,
                }],
                depth_attachment,
            };

            let mut encoder = cmd_buffer.begin_render_pass(geometry_pass_info);

            Self::bind_common_resources(self, &mut encoder);

            if let Some(ref sky_pipeline) = self.sky_pipeline {
                if let Some(ref dummy_vb) = self.dummy_vertex_buffer {
                    encoder.bind_vertex_buffer(dummy_vb, 0, 10);
                }
                encoder.bind_graphics_pipeline(sky_pipeline);
                encoder.draw(3, 1, 0, 0);
            }

            if let Some(draw_list) = &draw_list {
                Self::draw_objects(self, &mut encoder, draw_list);
            }

            encoder.end_encoding();
        } else {
            // No tonemap pipeline — render geometry directly to drawable (legacy path)
            drawable_written = true;
            let depth_attachment =
                self.depth_stencil_view
                    .as_ref()
                    .map(|view| DepthAttachmentInfo {
                        view: view.clone(),
                        load_op: if depth_prepass_ran {
                            LoadOp::Load
                        } else {
                            LoadOp::Clear
                        },
                        store_op: StoreOp::Store,
                        clear_value: ClearValue::depth_stencil(0.0, 0),
                        format: ImageFormat::D32SfloatS8Uint,
                    });

            let render_pass_info = RenderPassInfo {
                color_attachments: vec![ColorAttachmentInfo {
                    view: drawable_view.clone(),
                    load_op: LoadOp::Clear,
                    store_op: StoreOp::Store,
                    clear_value: ClearValue::color(
                        CANVAS_CLEAR_COLOR.0 as f32,
                        CANVAS_CLEAR_COLOR.1 as f32,
                        CANVAS_CLEAR_COLOR.2 as f32,
                        CANVAS_CLEAR_COLOR.3 as f32,
                    ),
                }],
                depth_attachment,
            };

            let mut encoder = cmd_buffer.begin_render_pass(render_pass_info);

            Self::bind_common_resources(self, &mut encoder);

            if let Some(ref sky_pipeline) = self.sky_pipeline {
                if let (Some(frame_buf), Some(object_buf)) = (
                    self.current_frame_uniform_buffer(),
                    self.current_object_storage_buffer(),
                ) {
                    let stages = ShaderStages::VERTEX_FRAGMENT;
                    encoder.bind_storage_buffer(frame_buf, 0, 0, stages);
                    encoder.bind_storage_buffer(object_buf, 0, 1, stages);
                }
                if let Some(ref buf_sizes) = self.buffer_sizes_buffer {
                    encoder.bind_storage_buffer(buf_sizes, 0, 8, ShaderStages::VERTEX_FRAGMENT);
                }
                if let Some(ref dummy_vb) = self.dummy_vertex_buffer {
                    encoder.bind_vertex_buffer(dummy_vb, 0, 10);
                }
                encoder.bind_graphics_pipeline(sky_pipeline);
                encoder.draw(3, 1, 0, 0);
            }

            if let Some(draw_list) = &draw_list {
                Self::draw_objects(self, &mut encoder, draw_list);
            }

            encoder.end_encoding();
        }

        // =========================================================================
        // Pass 1.5: Outline (stencil mark + outline draw on HDR texture)
        // =========================================================================
        if schedule.contains(PassKind::Outline)
            && let Some(outline_draw_list) = self.pending_outline_draw_list.take()
            && !outline_draw_list.draws.is_empty()
            && has_tonemap
        {
            let geometry_hdr_view = self.geometry_hdr_view.as_ref().unwrap();
            let depth_view = self.depth_stencil_view.as_ref().ok_or_else(|| {
                RendererError::InvalidOperation(
                    "Outline pass requires D32SfloatS8Uint depth-stencil texture".into(),
                )
            })?;
            let frame_buf = self.current_frame_uniform_buffer().unwrap();
            let object_buf = self.current_object_storage_buffer().unwrap();
            // Scene render targets are panel-sized, so the outline viewport must
            // match the panel — not the drawable.
            let w = vp_w as u32;
            let h = vp_h as u32;

            if let Some(stencil_pipeline) = self.outline.stencil_mark_pipeline() {
                super::outline::render_stencil_mark(
                    &mut cmd_buffer,
                    stencil_pipeline,
                    self.outline.stencil_mark_skinned_pipeline(),
                    geometry_hdr_view,
                    depth_view,
                    w,
                    h,
                    frame_buf,
                    object_buf,
                    &self.meshes,
                    &self.materials,
                    &outline_draw_list,
                    &self.skeletons,
                );
            }

            if let Some(outline_pipeline) = self.outline.outline_draw_pipeline() {
                super::outline::render_outline(
                    &mut cmd_buffer,
                    outline_pipeline,
                    self.outline.outline_draw_skinned_pipeline(),
                    geometry_hdr_view,
                    depth_view,
                    w,
                    h,
                    frame_buf,
                    object_buf,
                    &self.meshes,
                    &self.materials,
                    &outline_draw_list,
                    &self.skeletons,
                );
            }
        }

        // =========================================================================
        // Pass 1.6: Object-ID picking → R32Uint texture
        // =========================================================================
        if let Some(ref picking_dl) = picking_draw_list
            && !picking_dl.draws.is_empty()
            && let (Some(picking_pipeline), Some(id_view), Some(depth_view)) = (
                self.picking.pipeline(),
                self.picking.object_id_texture(),
                self.depth_stencil_view.as_ref(),
            )
        {
            let frame_buf = self.current_frame_uniform_buffer().unwrap();
            let object_buf = self.current_object_storage_buffer().unwrap();
            // Picking texture is panel-sized, so use the panel dimensions.
            let w = vp_w as u32;
            let h = vp_h as u32;

            super::picking::render_object_id_pass(
                &mut cmd_buffer,
                picking_pipeline,
                self.picking.pipeline_skinned(),
                id_view,
                depth_view,
                w,
                h,
                frame_buf,
                object_buf,
                &self.meshes,
                &self.materials,
                picking_dl,
                &self.skeletons,
            );
        }

        // =========================================================================
        // Pass 2: Tonemap (HDR intermediate → viewport_0 LDR texture)
        // =========================================================================
        if has_tonemap {
            let tonemap_pipeline = self.tonemap_pipeline.as_ref().unwrap();

            // Patch frame uniforms with HDR texture bindless index for the tonemap shader
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

            let (tonemap_target, tonemap_load_op) = if ui_will_composite_viewport {
                // The editor UI samples viewport_0. Render in the panel-sized
                // texture's local coordinate system so the image fills the whole
                // viewport widget instead of occupying a drawable-relative quadrant.
                (
                    self.tonemap_output_view
                        .as_ref()
                        .expect("viewport_0 view checked above")
                        .clone(),
                    LoadOp::Clear,
                )
            } else {
                // Non-editor/headless fallback when there is no UI composition pass.
                drawable_written = true;
                (drawable_view.clone(), LoadOp::Clear)
            };
            let (tonemap_x, tonemap_y, tonemap_w, tonemap_h) =
                tonemap_viewport(height, vp_x, vp_y, vp_w, vp_h, ui_will_composite_viewport);

            let tonemap_pass_info = RenderPassInfo {
                color_attachments: vec![ColorAttachmentInfo {
                    view: tonemap_target,
                    load_op: tonemap_load_op,
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

            let mut encoder = cmd_buffer.begin_render_pass(tonemap_pass_info);
            encoder.set_viewport(tonemap_x, tonemap_y, tonemap_w, tonemap_h, 0.0, 1.0);
            encoder.set_scissor(
                tonemap_x as u32,
                tonemap_y as u32,
                tonemap_w as u32,
                tonemap_h as u32,
            );

            if let Some(frame_buf) = self.current_frame_uniform_buffer() {
                encoder.bind_storage_buffer(frame_buf, 0, 0, ShaderStages::VERTEX_FRAGMENT);
            }
            if let Some(object_buf) = self.current_object_storage_buffer() {
                encoder.bind_storage_buffer(object_buf, 0, 1, ShaderStages::VERTEX_FRAGMENT);
            }
            if let Some(arg_buffer) = self.bindless_manager.argument_buffer() {
                unsafe {
                    encoder
                        .inner
                        .setVertexBuffer_offset_atIndex(Some(arg_buffer), 0, 9);
                    encoder
                        .inner
                        .setFragmentBuffer_offset_atIndex(Some(arg_buffer), 0, 9);
                }
            }
            if let Some(ref sampler) = self.shared_sampler {
                unsafe {
                    encoder
                        .inner
                        .setFragmentSamplerState_atIndex(Some(&sampler.inner), 0);
                }
            }
            if let Some(ref buf_sizes) = self.buffer_sizes_buffer {
                encoder.bind_storage_buffer(buf_sizes, 0, 8, ShaderStages::VERTEX_FRAGMENT);
            }
            if let Some(ref dummy_vb) = self.dummy_vertex_buffer {
                encoder.bind_vertex_buffer(dummy_vb, 0, 10);
            }

            // Make argument buffer and HDR texture readable by fragment shader
            if let Some(arg_buffer) = self.bindless_manager.argument_buffer() {
                encoder.use_buffer(
                    arg_buffer,
                    objc2_metal::MTLResourceUsage::Read,
                    objc2_metal::MTLRenderStages::Fragment,
                );
            }
            if let Some(ref hdr_view) = self.geometry_hdr_view {
                encoder.use_texture(
                    &hdr_view.inner,
                    objc2_metal::MTLResourceUsage::Read,
                    objc2_metal::MTLRenderStages::Fragment,
                );
            }

            encoder.bind_graphics_pipeline(tonemap_pipeline);
            encoder.draw(3, 1, 0, 0);

            if let Some(ref fence) = self.tonemap_fence {
                encoder
                    .inner
                    .updateFence_afterStages(fence, objc2_metal::MTLRenderStages::Fragment);
            }

            encoder.end_encoding();
        }

        // =========================================================================
        // Pass 3: UI overlay → drawable (loads previous pass output)
        // =========================================================================
        if schedule.contains(PassKind::Ui)
            && let Some(ui_draw_list) = self.pending_ui_draw_list.take()
            && !ui_draw_list.is_empty()
            && self
                .ui_renderer
                .upload_draw_list(&self.context, &ui_draw_list)
                .is_ok()
        {
            let ui_material_handle = self.ui_renderer.ui_material();
            if let Some(ui_mat_handle) = ui_material_handle
                && let Some(ui_material) = self.materials.get(ui_mat_handle.index())
                && let Some(ref ui_pipeline) = ui_material.pipeline
            {
                // When tonemap wrote viewport_0, UI is the first drawable writer
                // and must clear the canvas. Direct-to-drawable fallbacks are loaded.
                let ui_load_op = if drawable_written {
                    LoadOp::Load
                } else {
                    LoadOp::Clear
                };
                drawable_written = true;
                let ui_pass_info = RenderPassInfo {
                    color_attachments: vec![ColorAttachmentInfo {
                        view: drawable_view.clone(),
                        load_op: ui_load_op,
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

                let mut encoder = cmd_buffer.begin_render_pass(ui_pass_info);

                if let Some(ref fence) = self.tonemap_fence {
                    encoder
                        .inner
                        .waitForFence_beforeStages(fence, objc2_metal::MTLRenderStages::Fragment);
                }

                let dw = self.drawable_size.width as f32;
                let dh = self.drawable_size.height as f32;
                encoder.set_viewport(0.0, 0.0, dw, dh, 0.0, 1.0);

                encoder.bind_graphics_pipeline(ui_pipeline);

                if let Some(arg_buffer) = self.bindless_manager.argument_buffer() {
                    unsafe {
                        encoder
                            .inner
                            .setVertexBuffer_offset_atIndex(Some(arg_buffer), 0, 9);
                        encoder
                            .inner
                            .setFragmentBuffer_offset_atIndex(Some(arg_buffer), 0, 9);
                    }
                    encoder.use_buffer(
                        arg_buffer,
                        objc2_metal::MTLResourceUsage::Read,
                        objc2_metal::MTLRenderStages::Vertex
                            | objc2_metal::MTLRenderStages::Fragment,
                    );
                    for texture in self.bindless_manager.registered_textures() {
                        encoder.use_texture(
                            texture,
                            objc2_metal::MTLResourceUsage::Read,
                            objc2_metal::MTLRenderStages::Vertex
                                | objc2_metal::MTLRenderStages::Fragment,
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

                if let Some(vb) = self.ui_renderer.vertex_buffer() {
                    encoder.bind_vertex_buffer(vb, 0, 10);
                }
                if let Some(ib) = self.ui_renderer.index_buffer() {
                    encoder.bind_index_buffer(ib, 0, IndexType::Uint32);
                }
                if let Some(ref buf_sizes) = self.buffer_sizes_buffer {
                    encoder.bind_storage_buffer(buf_sizes, 0, 8, ShaderStages::VERTEX_FRAGMENT);
                }
                self.ui_renderer.render_ui_commands(
                    &mut encoder,
                    &ui_draw_list,
                    ui_pipeline,
                    self.drawable_size.width,
                    self.drawable_size.height,
                );

                encoder.end_encoding();
            }
        }

        // Safety: if no render pass wrote to the drawable (e.g. tonemap went to
        // viewport_0 and the UI pass was skipped), clear it to black before
        // presenting to avoid stale/undefined content from the drawable pool.
        if !drawable_written {
            log::warn!("METAL render_frame: NO pass wrote to drawable — clearing to black!");
            let clear_pass_info = RenderPassInfo {
                color_attachments: vec![ColorAttachmentInfo {
                    view: drawable_view,
                    load_op: LoadOp::Clear,
                    store_op: StoreOp::Store,
                    clear_value: ClearValue::OPAQUE_BLACK,
                }],
                depth_attachment: None,
            };
            let encoder = cmd_buffer.begin_render_pass(clear_pass_info);
            encoder.end_encoding();
        } else {
            // drawable_written stays true, no need to clear
        }

        cmd_buffer.end();
        self.context.surface.present(&cmd_buffer.inner);
        self.last_command_buffer = Some(cmd_buffer.inner.clone());
        cmd_buffer.submit(&self.context);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::tonemap_viewport;

    #[test]
    fn offscreen_tonemap_uses_texture_local_coordinates() {
        assert_eq!(
            tonemap_viewport(1200.0, 240.0, 80.0, 960.0, 720.0, true),
            (0.0, 0.0, 960.0, 720.0)
        );
    }

    #[test]
    fn direct_tonemap_converts_top_down_panel_origin_for_metal() {
        assert_eq!(
            tonemap_viewport(1200.0, 240.0, 80.0, 960.0, 720.0, false),
            (240.0, 400.0, 960.0, 720.0)
        );
    }
}
