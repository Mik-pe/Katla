use objc2_metal::MTLRenderCommandEncoder;

use crate::backend::command::{GpuRenderEncoder, IndexType, ShaderStages};
use crate::renderer::types::DrawList;

use super::metal_renderer::{MetalRenderer, OBJECT_UNIFORM_SIZE};
use super::render_encoder::MetalRenderEncoder;

impl MetalRenderer {
    pub(crate) fn bind_common_resources(&self, encoder: &mut MetalRenderEncoder) {
        if let (Some(frame_buf), Some(object_buf)) = (
            self.current_frame_uniform_buffer(),
            self.current_object_storage_buffer(),
        ) {
            let stages = ShaderStages::VERTEX_FRAGMENT;
            encoder.bind_storage_buffer(frame_buf, 0, 0, stages);
            encoder.bind_storage_buffer(object_buf, 0, 1, stages);
        }

        if let Some(arg_buffer) = self.bindless_manager.argument_buffer() {
            let stages = ShaderStages::VERTEX_FRAGMENT;
            unsafe {
                if stages.vertex {
                    encoder
                        .inner
                        .setVertexBuffer_offset_atIndex(Some(arg_buffer), 0, 9);
                }
                if stages.fragment {
                    encoder
                        .inner
                        .setFragmentBuffer_offset_atIndex(Some(arg_buffer), 0, 9);
                }
            }
        }

        if let Some(ref sampler) = self.shared_sampler {
            unsafe {
                encoder
                    .inner
                    .setVertexSamplerState_atIndex(Some(&sampler.inner), 0);
                encoder
                    .inner
                    .setFragmentSamplerState_atIndex(Some(&sampler.inner), 0);
            }
        }

        if let Some(ref buf_sizes) = self.buffer_sizes_buffer {
            let stages = ShaderStages::VERTEX_FRAGMENT;
            encoder.bind_storage_buffer(buf_sizes, 0, 8, stages);
        }

        if let Some(ref lc) = self.light_culling {
            let stages = ShaderStages::FRAGMENT;
            encoder.bind_storage_buffer(lc.light_buffer(), 0, 3, stages);
            encoder.bind_storage_buffer(lc.tile_index_buffer(), 0, 4, stages);
            encoder.bind_storage_buffer(lc.tile_count_buffer(), 0, 5, stages);
        }

        if let Some(ref shadow_buf) = self.shadow_cascade_buffer {
            let stages = ShaderStages::FRAGMENT;
            encoder.bind_storage_buffer(shadow_buf, 0, 7, stages);
        }

        if let Some(shadow_view) = self.shadow.shadow_map_view() {
            unsafe {
                encoder
                    .inner
                    .setFragmentTexture_atIndex(Some(&shadow_view.inner), 1);
            }
        }

        if let Some(ref sampler) = self.shadow_sampler {
            unsafe {
                encoder
                    .inner
                    .setFragmentSamplerState_atIndex(Some(&sampler.inner), 1);
            }
        }

        if let Some(arg_buffer) = self.bindless_manager.argument_buffer() {
            encoder.use_buffer(
                arg_buffer,
                objc2_metal::MTLResourceUsage::Read,
                objc2_metal::MTLRenderStages::Fragment,
            );
        }
        for texture in self.bindless_manager.registered_textures() {
            encoder.use_texture(
                texture,
                objc2_metal::MTLResourceUsage::Read,
                objc2_metal::MTLRenderStages::Fragment,
            );
        }
    }

    pub(crate) fn draw_objects(&self, encoder: &mut MetalRenderEncoder, draw_list: &DrawList) {
        let stages = ShaderStages::VERTEX_FRAGMENT;
        for (i, draw) in draw_list.draws.iter().enumerate() {
            let Some(mesh) = self.meshes.get(draw.mesh.index()) else {
                log::warn!("Draw {}: mesh index {} not found", i, draw.mesh.index());
                continue;
            };
            let Some(material) = self.materials.get(draw.material.index()) else {
                log::warn!(
                    "Draw {}: material index {} not found",
                    i,
                    draw.material.index()
                );
                continue;
            };
            let Some(ref pipeline) = material.pipeline else {
                log::warn!("Draw {}: no pipeline", i);
                continue;
            };

            encoder.bind_graphics_pipeline(pipeline);

            if !draw.skeleton.is_none()
                && let Some(skeleton_buf) = self.skeletons.get(draw.skeleton.index())
            {
                encoder.bind_storage_buffer(skeleton_buf, 0, 2, stages);
            }

            // Metal's instance_id starts from 0 regardless of baseInstance,
            // so rebind the object storage buffer with a byte offset so that
            // objects[0] in the shader maps to the correct per-object data.
            let object_offset = draw.instance_index as usize * OBJECT_UNIFORM_SIZE as usize;
            if let Some(object_buf) = self.current_object_storage_buffer() {
                unsafe {
                    encoder.inner.setVertexBuffer_offset_atIndex(
                        Some(&object_buf.inner),
                        object_offset,
                        1,
                    );
                    encoder.inner.setFragmentBuffer_offset_atIndex(
                        Some(&object_buf.inner),
                        object_offset,
                        1,
                    );
                }
            }

            encoder.bind_vertex_buffer(&mesh.vertex_buffer, 0, 10);
            encoder.bind_index_buffer(&mesh.index_buffer, 0, IndexType::Uint32);
            encoder.draw_indexed(mesh.index_count, 1, 0, 0, 0);
        }
    }
}
