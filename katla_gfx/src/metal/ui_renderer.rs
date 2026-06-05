//! Metal-native UI rendering subsystem.
//!
//! Manages dynamic vertex/index/instance buffers for immediate-mode UI
//! rendering on the Metal backend with GPU instancing support.

use crate::backend::command::GpuRenderEncoder;
use crate::backend::resource::GpuBuffer;
use crate::error::RendererError;
use crate::renderer::types::UIDrawList;
use crate::vertex::{UNIT_QUAD_INDICES, UNIT_QUAD_VERTICES};

use objc2_metal::MTLRenderCommandEncoder;

use super::buffer::MetalBuffer;
use super::context::MetalContext;
use super::pipeline::MetalGraphicsPipeline;

const INITIAL_VERTEX_BUFFER_SIZE: u64 = 1 << 20; // 1 MB
const INITIAL_INDEX_BUFFER_SIZE: u64 = 1 << 20; // 1 MB
const INITIAL_INSTANCE_BUFFER_SIZE: u64 = 1 << 20; // 1 MB

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct UiUniforms {
    screen_size: [f32; 2],
    ndc_y_flip: f32,
    texture_index: u32,
}

/// Metal-native UI rendering subsystem.
///
/// Owns dynamic vertex/index/instance buffers.
/// The actual rendering is driven by `MetalRenderer::render_ui_pass()`
/// which binds textures and pipelines from the renderer's own storage.
pub(crate) struct MetalUIRenderer {
    vertex_buffer: Option<MetalBuffer>,
    index_buffer: Option<MetalBuffer>,
    instance_buffer: Option<MetalBuffer>,
    unit_quad_vertex_buffer: Option<MetalBuffer>,
    unit_quad_index_buffer: Option<MetalBuffer>,
    vertex_buffer_capacity: u64,
    index_buffer_capacity: u64,
    instance_buffer_capacity: u64,
    ui_material: Option<crate::handle::MaterialHandle>,
    instanced_pipeline: Option<MetalGraphicsPipeline>,
}

impl MetalUIRenderer {
    pub(crate) fn new() -> Self {
        Self {
            vertex_buffer: None,
            index_buffer: None,
            instance_buffer: None,
            unit_quad_vertex_buffer: None,
            unit_quad_index_buffer: None,
            vertex_buffer_capacity: 0,
            index_buffer_capacity: 0,
            instance_buffer_capacity: 0,
            ui_material: None,
            instanced_pipeline: None,
        }
    }

    pub(crate) fn ui_material(&self) -> Option<crate::handle::MaterialHandle> {
        self.ui_material
    }

    pub(crate) fn set_ui_material(&mut self, handle: crate::handle::MaterialHandle) {
        self.ui_material = Some(handle);
    }

    pub(crate) fn set_instanced_pipeline(&mut self, pipeline: MetalGraphicsPipeline) {
        self.instanced_pipeline = Some(pipeline);
    }

    pub(crate) fn vertex_buffer(&self) -> Option<&MetalBuffer> {
        self.vertex_buffer.as_ref()
    }

    pub(crate) fn index_buffer(&self) -> Option<&MetalBuffer> {
        self.index_buffer.as_ref()
    }

    fn ensure_vertex_buffer(
        &mut self,
        context: &MetalContext,
        required: u64,
    ) -> Result<(), RendererError> {
        if self.vertex_buffer_capacity >= required {
            return Ok(());
        }
        let new_cap = if self.vertex_buffer_capacity == 0 {
            INITIAL_VERTEX_BUFFER_SIZE.max(required)
        } else {
            let mut cap = self.vertex_buffer_capacity;
            while cap < required {
                cap *= 2;
            }
            cap
        };
        self.vertex_buffer = Some(context.create_buffer(new_cap, true)?);
        self.vertex_buffer_capacity = new_cap;
        Ok(())
    }

    fn ensure_index_buffer(
        &mut self,
        context: &MetalContext,
        required: u64,
    ) -> Result<(), RendererError> {
        if self.index_buffer_capacity >= required {
            return Ok(());
        }
        let new_cap = if self.index_buffer_capacity == 0 {
            INITIAL_INDEX_BUFFER_SIZE.max(required)
        } else {
            let mut cap = self.index_buffer_capacity;
            while cap < required {
                cap *= 2;
            }
            cap
        };
        self.index_buffer = Some(context.create_buffer(new_cap, true)?);
        self.index_buffer_capacity = new_cap;
        Ok(())
    }

    fn ensure_instance_buffer(
        &mut self,
        context: &MetalContext,
        required: u64,
    ) -> Result<(), RendererError> {
        if self.instance_buffer_capacity >= required {
            return Ok(());
        }
        let new_cap = if self.instance_buffer_capacity == 0 {
            INITIAL_INSTANCE_BUFFER_SIZE.max(required)
        } else {
            let mut cap = self.instance_buffer_capacity;
            while cap < required {
                cap *= 2;
            }
            cap
        };
        self.instance_buffer = Some(context.create_buffer(new_cap, true)?);
        self.instance_buffer_capacity = new_cap;
        Ok(())
    }

    /// Upload UI draw list data into dynamic buffers.
    pub(crate) fn upload_draw_list(
        &mut self,
        context: &MetalContext,
        draw_list: &UIDrawList,
    ) -> Result<(), RendererError> {
        if draw_list.is_empty() {
            return Ok(());
        }

        // Upload vertex/index data for complex geometry
        if !draw_list.indices.is_empty() {
            let vertex_data = bytemuck::cast_slice(&draw_list.vertices);
            let vertex_size = vertex_data.len() as u64;
            let index_data = bytemuck::cast_slice(&draw_list.indices);
            let index_size = index_data.len() as u64;

            self.ensure_vertex_buffer(context, vertex_size)?;
            self.ensure_index_buffer(context, index_size)?;

            if let Some(ref vb) = self.vertex_buffer {
                let ptr = vb.map();
                unsafe {
                    std::ptr::copy_nonoverlapping(vertex_data.as_ptr(), ptr, vertex_data.len());
                }
                vb.unmap();
            }

            if let Some(ref ib) = self.index_buffer {
                let ptr = ib.map();
                unsafe {
                    std::ptr::copy_nonoverlapping(index_data.as_ptr(), ptr, index_data.len());
                }
                ib.unmap();
            }
        }

        // Upload instance data for instanced quads
        if !draw_list.instances.is_empty() {
            let instance_data = bytemuck::cast_slice(&draw_list.instances);
            let instance_size = instance_data.len() as u64;
            self.ensure_instance_buffer(context, instance_size)?;

            if let Some(ref ib) = self.instance_buffer {
                let ptr = ib.map();
                unsafe {
                    std::ptr::copy_nonoverlapping(instance_data.as_ptr(), ptr, instance_data.len());
                }
                ib.unmap();
            }
        }

        // Upload unit quad buffers (small, static data)
        let quad_vb_data = bytemuck::cast_slice(&UNIT_QUAD_VERTICES);
        let quad_ib_data = bytemuck::cast_slice(&UNIT_QUAD_INDICES);

        self.ensure_instance_buffer(context, quad_vb_data.len() as u64)?;
        // Reuse instance buffer capacity check for unit quad buffers
        if self.unit_quad_vertex_buffer.is_none() {
            self.unit_quad_vertex_buffer = Some(context.create_buffer(256, true)?);
            self.unit_quad_index_buffer = Some(context.create_buffer(256, true)?);

            if let Some(ref vb) = self.unit_quad_vertex_buffer {
                let ptr = vb.map();
                unsafe {
                    std::ptr::copy_nonoverlapping(quad_vb_data.as_ptr(), ptr, quad_vb_data.len());
                }
                vb.unmap();
            }
            if let Some(ref ib) = self.unit_quad_index_buffer {
                let ptr = ib.map();
                unsafe {
                    std::ptr::copy_nonoverlapping(quad_ib_data.as_ptr(), ptr, quad_ib_data.len());
                }
                ib.unmap();
            }
        }

        Ok(())
    }

    /// Issue draw calls for all UI commands on the given encoder.
    ///
    /// `non_instanced_pipeline` is the standard UI pipeline (vs_main/fs_main)
    /// used for complex geometry draws. The instanced pipeline is stored internally.
    pub(crate) fn render_ui_commands(
        &self,
        encoder: &mut super::render_encoder::MetalRenderEncoder,
        draw_list: &UIDrawList,
        non_instanced_pipeline: &super::pipeline::MetalGraphicsPipeline,
        render_pass_w: u32,
        render_pass_h: u32,
    ) {
        let full_scissor = (0u32, 0u32, render_pass_w, render_pass_h);
        let mut prev_scissor = full_scissor;
        let mut using_instanced_pipeline = false;

        log::debug!(
            "METAL render_ui_commands: {} commands, {} instances, screen_size={:?}, scale_factor={}",
            draw_list.commands.len(),
            draw_list.instances.len(),
            draw_list.screen_size,
            draw_list.scale_factor,
        );

        for cmd in draw_list.commands.iter() {
            let scissor = if let Some([x, y, w, h]) = cmd.clip_rect {
                let s = draw_list.scale_factor;
                let sx = (x * s).max(0.0) as u32;
                let sy = (y * s).max(0.0) as u32;
                let sw = (w * s).max(0.0) as u32;
                let sh = (h * s).max(0.0) as u32;
                (
                    sx.min(render_pass_w),
                    sy.min(render_pass_h),
                    sw.min(render_pass_w.saturating_sub(sx)),
                    sh.min(render_pass_h.saturating_sub(sy)),
                )
            } else {
                full_scissor
            };

            if scissor != prev_scissor {
                encoder.set_scissor(scissor.0, scissor.1, scissor.2, scissor.3);
                prev_scissor = scissor;
            }

            if cmd.is_instanced {
                // Switch to instanced pipeline if not already bound.
                // Use bind_graphics_pipeline (not raw setRenderPipelineState) to
                // ensure depth stencil state, cull mode, and front face winding
                // are properly set for the instanced pipeline.
                if !using_instanced_pipeline {
                    if let Some(inst_pipe) = self.instanced_pipeline.as_ref() {
                        encoder.bind_graphics_pipeline(inst_pipe);
                    }
                    using_instanced_pipeline = true;
                }

                // Instanced draw: bind instance buffer + unit quad, draw instanced
                let uniform_data = UiUniforms {
                    screen_size: [draw_list.screen_size[0], draw_list.screen_size[1]],
                    ndc_y_flip: -1.0,
                    texture_index: 0,
                };
                encoder.set_push_constants(
                    bytemuck::cast_slice(&[uniform_data]),
                    3,
                    crate::backend::command::ShaderStages::VERTEX_FRAGMENT,
                );
                // Bind instance data as storage buffer at buffer 11 (vertex stage).
                // Metal's instance_id starts from 0 regardless of baseInstance,
                // so we bind the buffer with a byte offset so instance_data[0]
                // maps to the correct batch offset.
                let instance_offset =
                    cmd.offset as usize * std::mem::size_of::<crate::vertex::VertexUIInstance>();
                if let Some(ref inst_buf) = self.instance_buffer {
                    unsafe {
                        encoder.inner.setVertexBuffer_offset_atIndex(
                            Some(&inst_buf.inner),
                            instance_offset,
                            11,
                        );
                    }
                }
                if let Some(ref quad_ib) = self.unit_quad_index_buffer {
                    encoder.bind_index_buffer(
                        quad_ib,
                        0,
                        crate::backend::command::IndexType::Uint32,
                    );
                }
                if let Some(ref quad_vb) = self.unit_quad_vertex_buffer {
                    encoder.bind_vertex_buffer(quad_vb, 0, 10);
                }
                encoder.draw_indexed(6, cmd.count, 0, 0, 0);
            } else {
                // Switch back to non-instanced pipeline if needed
                if using_instanced_pipeline {
                    encoder.bind_graphics_pipeline(non_instanced_pipeline);
                    using_instanced_pipeline = false;
                }

                // Vertex-based draw: complex geometry
                let uniform_data = UiUniforms {
                    screen_size: [draw_list.screen_size[0], draw_list.screen_size[1]],
                    ndc_y_flip: -1.0,
                    texture_index: cmd.texture.index(),
                };
                encoder.set_push_constants(
                    bytemuck::cast_slice(&[uniform_data]),
                    3,
                    crate::backend::command::ShaderStages::VERTEX_FRAGMENT,
                );
                // Re-bind original vertex/index buffers for complex geometry
                if let Some(ref vb) = self.vertex_buffer {
                    encoder.bind_vertex_buffer(vb, 0, 10);
                }
                if let Some(ref ib) = self.index_buffer {
                    encoder.bind_index_buffer(ib, 0, crate::backend::command::IndexType::Uint32);
                }
                encoder.draw_indexed(cmd.count, 1, cmd.offset, 0, 0);
            }
        }

        if prev_scissor != full_scissor {
            encoder.set_scissor(
                full_scissor.0,
                full_scissor.1,
                full_scissor.2,
                full_scissor.3,
            );
        }
    }
}
