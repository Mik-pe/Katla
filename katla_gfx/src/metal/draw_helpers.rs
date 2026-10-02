use crate::backend::command::{GpuRenderEncoder, IndexType, ShaderStages};
use crate::renderer::types::PreparedDraws;

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

        if let Some(snapshot) = self.bindless_manager.snapshot() {
            encoder.bind_bindless(snapshot);
        }

        if let Some(ref sampler) = self.shared_sampler {
            encoder.bind_native_sampler(
                &sampler.inner,
                0,
                crate::backend::command::ShaderStages::VERTEX,
            );
            encoder.bind_native_sampler(
                &sampler.inner,
                0,
                crate::backend::command::ShaderStages::FRAGMENT,
            );
        }

        if let Some(ref lc) = self.light_culling {
            let stages = ShaderStages::FRAGMENT;
            encoder.bind_storage_buffer(lc.light_buffer(), 0, 3, stages);
            encoder.bind_storage_buffer(lc.tile_index_buffer(), 0, 4, stages);
            encoder.bind_storage_buffer(lc.tile_count_buffer(), 0, 5, stages);
        }

        if let Some(ref shadow_buf) = self.shadow_cascade_buffers[self.frame_index()] {
            let stages = ShaderStages::FRAGMENT;
            encoder.bind_storage_buffer(shadow_buf, 0, 7, stages);
        }

        if let Some(ref sampler) = self.shadow_sampler {
            encoder.bind_native_sampler(
                &sampler.inner,
                1,
                crate::backend::command::ShaderStages::FRAGMENT,
            );
        }
    }

    pub(crate) fn draw_objects(
        &self,
        encoder: &mut MetalRenderEncoder,
        color_format: crate::texture::ImageFormat,
        draws: PreparedDraws<'_>,
    ) {
        log::debug!(
            "METAL draw_objects: {} draws, frame_buf={}, object_buf={}",
            draws.counts().draw_calls,
            self.current_frame_uniform_buffer().is_some(),
            self.current_object_storage_buffer().is_some(),
        );
        let stages = ShaderStages::VERTEX_FRAGMENT;
        let last_draw = draws.counts().draw_calls.saturating_sub(1);
        for (i, draw) in draws.iter().enumerate() {
            let Some(mesh) = self.meshes.get(draw.mesh) else {
                log::warn!("Draw {}: mesh index {} not found", i, draw.mesh.index());
                continue;
            };
            let Some(material) = self.materials.get(draw.material) else {
                log::warn!(
                    "Draw {}: material index {} not found",
                    i,
                    draw.material.index()
                );
                continue;
            };
            let key = crate::renderer::pipeline_variant::PipelineVariantKey::resolve(
                &material.descriptor,
                color_format,
            );
            let Some(pipeline) = material.variants.get(&key) else {
                log::warn!(
                    "Draw {}: material has no pipeline variant for color {:?} / depth {:?}",
                    i,
                    key.color_format(),
                    key.depth_format()
                );
                continue;
            };

            if i < 3 || i == last_draw {
                log::debug!(
                    "METAL draw_objects[{}]: mesh_idx={}, mat_idx={}, instance_index={}, \
                     skeleton={:?}, index_count={}, tex_indices={:?}, vertex_type={:?}",
                    i,
                    draw.mesh.index(),
                    draw.material.index(),
                    draw.instance_index,
                    draw.skeleton,
                    mesh.index_count,
                    material.textures,
                    material.descriptor.vertex,
                );
            }

            encoder.bind_graphics_pipeline(pipeline);

            if !draw.skeleton.is_none()
                && let Some(skeleton_buf) = self.skeletons[self.frame_index()].get(draw.skeleton)
            {
                encoder.bind_storage_buffer(skeleton_buf, 0, 2, stages);
            }

            // Metal's instance_id starts from 0 regardless of baseInstance,
            // so rebind the object storage buffer with a byte offset so that
            // objects[0] in the shader maps to the correct per-object data.
            let object_offset = draw.instance_index as usize * OBJECT_UNIFORM_SIZE as usize;
            if let Some(object_buf) = self.current_object_storage_buffer() {
                encoder.bind_native_buffer(
                    &object_buf.inner,
                    (object_offset) as u64,
                    1,
                    crate::backend::command::ShaderStages::VERTEX,
                );
                encoder.bind_native_buffer(
                    &object_buf.inner,
                    (object_offset) as u64,
                    1,
                    crate::backend::command::ShaderStages::FRAGMENT,
                );
            }

            // An empty dynamic mesh draws nothing.
            if mesh.index_count == 0 {
                continue;
            }
            encoder.bind_vertex_buffer(&mesh.vertex_buffer, 0, 10);
            encoder.bind_index_buffer(&mesh.index_buffer, 0, IndexType::Uint32);
            encoder.draw_indexed(mesh.index_count, draw.instance_count().max(1), 0, 0, 0);
        }
    }
}
