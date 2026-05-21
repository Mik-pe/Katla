//! Metal-native UI rendering subsystem.
//!
//! Manages dynamic vertex/index buffers for
//! immediate-mode UI rendering on the Metal backend.

use crate::backend::command::GpuRenderEncoder;
use crate::backend::resource::GpuBuffer;
use crate::error::RendererError;
use crate::renderer::types::UIDrawList;

use super::buffer::MetalBuffer;
use super::context::MetalContext;

const INITIAL_VERTEX_BUFFER_SIZE: u64 = 1 << 20; // 1 MB
const INITIAL_INDEX_BUFFER_SIZE: u64 = 1 << 20; // 1 MB

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct UiUniforms {
    screen_size: [f32; 2],
    ndc_y_flip: f32,
    texture_index: u32,
}

/// Metal-native UI rendering subsystem.
///
/// Owns dynamic vertex/index buffers.
/// The actual rendering is driven by `MetalRenderer::render_ui_pass()`
/// which binds textures and pipelines from the renderer's own storage.
pub(crate) struct MetalUIRenderer {
    vertex_buffer: Option<MetalBuffer>,
    index_buffer: Option<MetalBuffer>,
    vertex_buffer_capacity: u64,
    index_buffer_capacity: u64,
    ui_material: Option<crate::handle::MaterialHandle>,
}

impl MetalUIRenderer {
    pub(crate) fn new() -> Self {
        Self {
            vertex_buffer: None,
            index_buffer: None,
            vertex_buffer_capacity: 0,
            index_buffer_capacity: 0,
            ui_material: None,
        }
    }

    pub(crate) fn ui_material(&self) -> Option<crate::handle::MaterialHandle> {
        self.ui_material
    }

    pub(crate) fn set_ui_material(&mut self, handle: crate::handle::MaterialHandle) {
        self.ui_material = Some(handle);
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

    /// Upload UI draw list vertex and index data into dynamic buffers.
    ///
    /// Call this before `render_ui_commands()`. Returns references to the
    /// bound buffers so the renderer can bind them to the encoder.
    pub(crate) fn upload_draw_list(
        &mut self,
        context: &MetalContext,
        draw_list: &UIDrawList,
    ) -> Result<(), RendererError> {
        if draw_list.is_empty() {
            return Ok(());
        }

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

        Ok(())
    }

    /// Issue draw calls for all UI commands on the given encoder.
    ///
    /// The caller must have already bound the UI pipeline, vertex buffer,
    /// index buffer, and viewport.
    pub(crate) fn render_ui_commands(
        &self,
        encoder: &mut super::render_encoder::MetalRenderEncoder,
        draw_list: &UIDrawList,
        render_pass_w: u32,
        _render_pass_h: u32,
    ) {
        for cmd in &draw_list.commands {
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
            encoder.draw_indexed(cmd.index_count, 1, cmd.index_offset, 0, 0);
        }
    }
}
