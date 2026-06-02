//! Metal frame rendering — hardcoded pass sequence.
//!
//! This is the legacy Metal render path that executes passes directly
//! without going through the frame graph. Used by Metal validation tests.

use std::mem;

use objc2_metal::{
    MTLBlitCommandEncoder, MTLCommandBuffer, MTLCommandEncoder, MTLRenderCommandEncoder,
};

use crate::backend::command::{
    ColorAttachmentInfo, DepthAttachmentInfo, GpuCommandBuffer, GpuRenderEncoder, IndexType,
    RenderPassInfo, ShaderStages,
};
use crate::backend::resource::GpuBuffer;
use crate::error::RendererError;
use crate::render_pass::{ClearValue, LoadOp, StoreOp};
use crate::renderer::types::FrameUniforms;
use crate::texture::ImageFormat;

use super::metal_renderer::MetalRenderer;
use super::texture::{MetalTexture, MetalTextureView};

impl MetalRenderer {
    pub(crate) fn render_frame(&mut self) -> Result<(), RendererError> {
        log::warn!(
            "METAL render_frame: depth_view={}, drawable_size={}x{}, \
             tonemap_pipeline={}, geometry_hdr_view={}, geometry_hdr_bindless_slot={:?}, \
             tonemap_output_view={}, sky_pipeline={}, \
             pending_draw_list={}, pending_shadow_draw_list={}, pending_ui_draw_list={}",
            self.depth_stencil_view.is_some(),
            self.drawable_size.width,
            self.drawable_size.height,
            self.tonemap_pipeline.is_some(),
            self.geometry_hdr_view.is_some(),
            self.geometry_hdr_bindless_slot,
            self.tonemap_output_view.is_some(),
            self.sky_pipeline.is_some(),
            self.pending_draw_list
                .as_ref()
                .map(|dl| dl.draws.len())
                .unwrap_or(0),
            self.pending_shadow_draw_list
                .as_ref()
                .map(|dl| dl.draws.len())
                .unwrap_or(0),
            self.pending_ui_draw_list
                .as_ref()
                .map(|dl| dl.commands.len())
                .unwrap_or(0),
        );
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

        // =========================================================================
        // Pass 0: Shadow cascade rendering → shadow map texture
        // =========================================================================
        if let Some(shadow_draw_list) = self.pending_shadow_draw_list.take()
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
        // Pass 1: Geometry (sky + PBR) → HDR intermediate texture
        // =========================================================================
        let has_tonemap = self.tonemap_pipeline.is_some() && self.geometry_hdr_view.is_some();
        let draw_list = self.pending_draw_list.take();
        let picking_draw_list = draw_list.clone();

        log::warn!(
            "METAL render_frame: has_tonemap={}, hdr_view={}, hdr_slot={:?}, \
             draw_list has {} draws, frame_uniform_buf={}, object_storage_buf={}",
            has_tonemap,
            self.geometry_hdr_view.is_some(),
            self.geometry_hdr_bindless_slot,
            draw_list.as_ref().map(|dl| dl.draws.len()).unwrap_or(0),
            self.current_frame_uniform_buffer().is_some(),
            self.current_object_storage_buffer().is_some(),
        );

        if has_tonemap {
            let geometry_hdr_view = self.geometry_hdr_view.as_ref().unwrap();

            let depth_attachment =
                self.depth_stencil_view
                    .as_ref()
                    .map(|view| DepthAttachmentInfo {
                        view: view.clone(),
                        load_op: LoadOp::Clear,
                        store_op: StoreOp::DontCare,
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

            if width > 0.0 && height > 0.0 {
                encoder.set_viewport(0.0, 0.0, width, height, 0.0, 1.0);
            }

            Self::bind_common_resources(self, &mut encoder);

            log::warn!(
                "METAL geometry pass (HDR): sky_pipeline={}, draw_list={} draws, \
                 camera_pos=[{:.2},{:.2},{:.2}], light_dir=[{:.2},{:.2},{:.2}]",
                self.sky_pipeline.is_some(),
                draw_list.as_ref().map(|dl| dl.draws.len()).unwrap_or(0),
                self.frame_uniforms.camera_position[0],
                self.frame_uniforms.camera_position[1],
                self.frame_uniforms.camera_position[2],
                self.frame_uniforms.light_direction[0],
                self.frame_uniforms.light_direction[1],
                self.frame_uniforms.light_direction[2],
            );

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
                        load_op: LoadOp::Clear,
                        store_op: StoreOp::DontCare,
                        clear_value: ClearValue::depth_stencil(0.0, 0),
                        format: ImageFormat::D32SfloatS8Uint,
                    });

            let render_pass_info = RenderPassInfo {
                color_attachments: vec![ColorAttachmentInfo {
                    view: drawable_view.clone(),
                    load_op: LoadOp::Clear,
                    store_op: StoreOp::Store,
                    clear_value: ClearValue::OPAQUE_BLACK,
                }],
                depth_attachment,
            };

            let mut encoder = cmd_buffer.begin_render_pass(render_pass_info);

            if width > 0.0 && height > 0.0 {
                encoder.set_viewport(0.0, 0.0, width, height, 0.0, 1.0);
            }

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
        if let Some(outline_draw_list) = self.pending_outline_draw_list.take()
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
            let w = self.drawable_size.width;
            let h = self.drawable_size.height;

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
            let w = self.drawable_size.width;
            let h = self.drawable_size.height;

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

            // Write tonemapped output to viewport_0 LDR texture (for UI compositing),
            // or fall back to the drawable if no viewport texture is available.
            let tonemap_writes_to_drawable = self.tonemap_output_view.is_none();
            if tonemap_writes_to_drawable {
                drawable_written = true;
            }
            let tonemap_target = self
                .tonemap_output_view
                .as_ref()
                .unwrap_or(&drawable_view)
                .clone();

            let tonemap_pass_info = RenderPassInfo {
                color_attachments: vec![ColorAttachmentInfo {
                    view: tonemap_target,
                    load_op: LoadOp::Clear,
                    store_op: StoreOp::Store,
                    clear_value: ClearValue::OPAQUE_BLACK,
                }],
                depth_attachment: None,
            };

            let mut encoder = cmd_buffer.begin_render_pass(tonemap_pass_info);

            if width > 0.0 && height > 0.0 {
                encoder.set_viewport(0.0, 0.0, width, height, 0.0, 1.0);
            }

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

            log::warn!(
                "METAL render_frame: tonemap pass submitted, hdr_slot={:?}, writes_to_drawable={}",
                self.geometry_hdr_bindless_slot,
                tonemap_writes_to_drawable,
            );

            if let Some(ref fence) = self.tonemap_fence {
                encoder
                    .inner
                    .updateFence_afterStages(fence, objc2_metal::MTLRenderStages::Fragment);
            }

            encoder.end_encoding();
        }

        // Blit the tonemapped viewport_0 texture into the drawable so the
        // 3D scene is visible underneath the UI overlay.
        let mut scene_blitted_to_drawable = false;
        if let Some(ref tonemap_view) = self.tonemap_output_view {
            if let Some(ref fence) = self.tonemap_fence {
                // Wait for tonemap to finish before reading its output.
                let blit = cmd_buffer.inner.blitCommandEncoder();
                if let Some(b) = blit {
                    b.waitForFence(fence);
                    b.endEncoding();
                }
            }

            // Copy viewport_0 (LDR tonemapped scene) → drawable texture.
            let src = &tonemap_view.inner;
            let dst = &drawable_view.inner;
            let w = self.drawable_size.width as usize;
            let h = self.drawable_size.height as usize;
            if w > 0 && h > 0 {
                let blit = cmd_buffer.inner.blitCommandEncoder();
                if let Some(b) = blit {
                    unsafe {
                        b.copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toTexture_destinationSlice_destinationLevel_destinationOrigin(
                            src,
                            0,
                            0,
                            objc2_metal::MTLOrigin { x: 0, y: 0, z: 0 },
                            objc2_metal::MTLSize {
                                width: w,
                                height: h,
                                depth: 1,
                            },
                            dst,
                            0,
                            0,
                            objc2_metal::MTLOrigin { x: 0, y: 0, z: 0 },
                        );
                    }
                    b.endEncoding();
                    scene_blitted_to_drawable = true;
                    drawable_written = true;
                }
            }
        }

        // =========================================================================
        // Pass 3: UI overlay → drawable (loads previous pass output)
        // =========================================================================
        if let Some(ui_draw_list) = self.pending_ui_draw_list.take()
            && !ui_draw_list.is_empty()
            && self
                .ui_renderer
                .upload_draw_list(&self.context, &ui_draw_list)
                .is_ok()
        {
            log::warn!(
                "METAL render_frame: UI pass — {} commands, {} vertices, {} indices, {} instances, \
                 ui_load_op={:?}, screen_size={:?}",
                ui_draw_list.commands.len(),
                ui_draw_list.vertices.len(),
                ui_draw_list.indices.len(),
                ui_draw_list.instances.len(),
                if self.tonemap_output_view.is_some() {
                    LoadOp::Clear
                } else {
                    LoadOp::Load
                },
                ui_draw_list.screen_size,
            );
            let ui_material_handle = self.ui_renderer.ui_material();
            if let Some(ui_mat_handle) = ui_material_handle
                && let Some(ui_material) = self.materials.get(ui_mat_handle.index())
                && let Some(ref ui_pipeline) = ui_material.pipeline
            {
                drawable_written = true;
                // When the scene was blitted to the drawable, load it so the UI
                // draws on top of the 3D scene. Otherwise (tonemap wrote directly
                // to drawable, or no tonemap) also load the prior pass output.
                let ui_load_op = if scene_blitted_to_drawable {
                    LoadOp::Load
                } else if self.tonemap_output_view.is_some() {
                    LoadOp::Clear
                } else {
                    LoadOp::Load
                };
                let ui_pass_info = RenderPassInfo {
                    color_attachments: vec![ColorAttachmentInfo {
                        view: drawable_view.clone(),
                        load_op: ui_load_op,
                        store_op: StoreOp::Store,
                        clear_value: ClearValue::OPAQUE_BLACK,
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
                    let tex_count = self.bindless_manager.registered_textures().count();
                    log::warn!(
                        "METAL render_frame: UI bindless — arg_buffer bound, {} registered textures, \
                         font_atlas={:?} (bindless_slot={:?})",
                        tex_count,
                        self.ui_font_atlas,
                        self.ui_font_atlas
                            .and_then(|h| self.get_bindless_slot_impl(h)),
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
            log::warn!("METAL render_frame: drawable was written to by render passes");
        }

        cmd_buffer.end();
        self.context.surface.present(&cmd_buffer.inner);
        self.last_command_buffer = Some(cmd_buffer.inner.clone());
        cmd_buffer.submit(&self.context);

        Ok(())
    }
}
