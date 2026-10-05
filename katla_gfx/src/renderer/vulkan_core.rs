//! Vulkan implementation of the device and frame contract.

use super::frame_scope::{FrameAcquisition, FrameToken, PresentOutcome};
use super::gpu_renderer::GpuRenderer;
use super::pipeline_descriptor::PipelineDescriptor;
use super::registry::PrimitiveTopology;
use super::types::DrawList;
use crate::Size2D;
use crate::error::RendererError;
use crate::handle::{BufferHandle, MaterialHandle, MeshHandle, SkeletonHandle, TextureHandle};
use crate::render_graph::BufferDesc;
use crate::texture::TextureDescriptor;

// VulkanRenderer impl — delegates to existing methods.
// Feature-gated behind vulkan since VulkanRenderer is vulkan-only.

use crate::renderer::VulkanRenderer;

impl GpuRenderer for VulkanRenderer {
    fn capture_submission_snapshot(
        &self,
    ) -> Option<crate::render_graph::capture::CapturedSubmission> {
        use crate::render_graph::capture::{CapturedFeedback, CapturedSubmission};
        let (slot, generation, fence) = self.last_submission?;
        let feedback = match fence
            .map(|fence| unsafe { self.context.device.get_fence_status(fence) })
            .unwrap_or(Ok(true))
        {
            Ok(true) => CapturedFeedback::Completed,
            Ok(false) => CapturedFeedback::Pending,
            Err(_) => CapturedFeedback::Failed,
        };
        Some(CapturedSubmission {
            frame_slot: slot,
            generation,
            command_allocator: slot,
            feedback_identity: format!("frame_fence:{slot}:{generation}"),
            feedback,
        })
    }

    fn create_mesh<T, U>(
        &mut self,
        vertices: &[T],
        indices: &[U],
        topology: PrimitiveTopology,
    ) -> Result<MeshHandle, RendererError>
    where
        T: crate::vertex::Vertex,
        U: crate::renderer::registry::MeshIndexElement,
    {
        VulkanRenderer::create_mesh(self, vertices, indices, topology)
    }
    fn read_buffer_completed(
        &mut self,
        handle: BufferHandle,
        range: crate::render_graph::BufferByteRange,
    ) -> Result<Option<Vec<u8>>, RendererError> {
        VulkanRenderer::read_buffer_completed(self, handle, range)
    }
    fn acquire_frame(&mut self) -> Result<FrameAcquisition, RendererError> {
        super::frame_lifecycle::acquire_frame(self)
    }

    fn graph_texture_source(
        &self,
        resource: crate::render_graph::ResourceId,
    ) -> Option<super::texture_readback::GraphTextureSource> {
        VulkanRenderer::graph_texture_source(self, resource)
    }
    fn queue_texture_readback(
        &mut self,
        source: super::texture_readback::GraphTextureSource,
        region: super::texture_readback::TextureReadbackRegion,
    ) -> Result<super::texture_readback::TextureReadbackTicket, RendererError> {
        VulkanRenderer::queue_texture_readback(self, source, region)
    }
    fn poll_texture_readback(
        &mut self,
        ticket: super::texture_readback::TextureReadbackTicket,
    ) -> Result<Option<super::texture_readback::TextureReadbackData>, RendererError> {
        VulkanRenderer::poll_texture_readback(self, ticket)
    }
    fn frame_slot_count(&self) -> usize {
        super::FRAMES_IN_FLIGHT
    }
    fn buffer_descriptor(&self, handle: BufferHandle) -> Option<BufferDesc> {
        self.graph_buffers.get(handle).map(|buffer| buffer.desc)
    }
    fn create_buffer_with_data(
        &mut self,
        desc: BufferDesc,
        data: &[u8],
    ) -> Result<BufferHandle, RendererError> {
        VulkanRenderer::create_buffer_with_data(self, desc, data)
    }
    fn write_buffer(
        &mut self,
        frame: &FrameToken,
        handle: BufferHandle,
        offset: u64,
        data: &[u8],
    ) -> Result<(), RendererError> {
        VulkanRenderer::write_buffer(self, frame, handle, offset, data)
    }
    fn skeleton_buffer_handle(
        &mut self,
        frame: &FrameToken,
        skeleton: SkeletonHandle,
    ) -> Result<BufferHandle, RendererError> {
        self.frame_write_check(frame)?;
        self.skeleton_buffers
            .get(skeleton)
            .and_then(|buffers| buffers.get(frame.slot()))
            .copied()
            .ok_or_else(|| RendererError::InvalidOperation("Skeleton storage unavailable".into()))
    }

    fn create_buffer(&mut self, desc: BufferDesc) -> Result<BufferHandle, RendererError> {
        VulkanRenderer::create_buffer(self, desc)
    }

    fn destroy_buffer(&mut self, handle: BufferHandle) -> Result<(), RendererError> {
        VulkanRenderer::destroy_buffer(self, handle)
    }

    fn execute_draw_calls(
        &mut self,
        frame: &FrameToken,
        draw_list: &DrawList,
    ) -> Result<(), RendererError> {
        self.frame_write_check(frame)?;
        VulkanRenderer::execute_draw_calls(self, draw_list)
    }

    fn present(&mut self, frame: FrameToken) -> Result<PresentOutcome, RendererError> {
        VulkanRenderer::present_frame(self, frame)
    }

    fn abort(&mut self, frame: FrameToken) -> Result<(), RendererError> {
        super::frame_lifecycle::abort_frame(self, frame);
        Ok(())
    }

    fn swapchain_extent(&self) -> Size2D {
        VulkanRenderer::swapchain_extent(self)
    }

    fn current_frame(&self) -> usize {
        VulkanRenderer::current_frame(self)
    }

    fn num_images(&self) -> usize {
        VulkanRenderer::num_images(self)
    }

    fn wait_for_device(&self) {
        VulkanRenderer::wait_for_device(self);
    }

    fn destroy(&mut self) {
        VulkanRenderer::destroy(self);
    }

    fn capabilities(&self) -> &crate::renderer::types::GpuCapabilities {
        &self.capabilities
    }

    fn supports_feature(&self, feature: crate::renderer::features::RendererFeature) -> bool {
        use crate::renderer::features::RendererFeature;
        match feature {
            RendererFeature::TextureSubresourceUpload | RendererFeature::TimestampQueries => false,
            RendererFeature::TextureInPlaceUpdate => true,
        }
    }

    fn mesh_index_format(&self, mesh: MeshHandle) -> Option<crate::backend::command::IndexType> {
        VulkanRenderer::mesh_index_format(self, mesh)
    }

    fn mesh_vertex_count(&self, mesh: MeshHandle) -> Option<u32> {
        VulkanRenderer::mesh_vertex_count(self, mesh)
    }

    fn mesh_index_count(&self, mesh: MeshHandle) -> Option<u32> {
        VulkanRenderer::mesh_index_count(self, mesh)
    }

    fn create_mesh_dynamic(
        &mut self,
        descriptor: &crate::renderer::registry::MeshDescriptor,
        vertex_data: &[u8],
        indices: &[u32],
    ) -> Result<MeshHandle, RendererError> {
        VulkanRenderer::create_mesh_dynamic(self, descriptor, vertex_data, indices)
    }

    fn update_mesh_dynamic(
        &mut self,
        mesh: MeshHandle,
        vertex_data: &[u8],
        vertex_count: u32,
        indices: &[u32],
    ) -> Result<(), RendererError> {
        VulkanRenderer::update_mesh_dynamic(self, mesh, vertex_data, vertex_count, indices)
    }

    fn create_texture(
        &mut self,
        desc: &TextureDescriptor,
        data: &[u8],
    ) -> Result<TextureHandle, RendererError> {
        VulkanRenderer::create_texture(self, desc, data)
    }

    fn create_texture_solid(&mut self, color: [u8; 4]) -> Result<TextureHandle, RendererError> {
        VulkanRenderer::create_texture_solid(self, color)
    }

    fn update_texture(&mut self, handle: TextureHandle, data: &[u8]) -> Result<(), RendererError> {
        let texture =
            self.texture_manager
                .get_texture(handle)
                .ok_or_else(|| RendererError::StaleHandle {
                    resource: "texture".to_string(),
                    detail: format!("{handle:?} in update_texture"),
                })?;
        texture.update_data(data)
    }

    fn get_bindless_slot(&self, handle: TextureHandle) -> Option<u32> {
        VulkanRenderer::get_bindless_slot(self, handle)
    }

    fn get_texture_at_slot(&self, slot: u32) -> Option<TextureHandle> {
        VulkanRenderer::get_texture_at_slot(self, slot)
    }

    fn get_texture_bindless_index(&self, handle: TextureHandle) -> u32 {
        VulkanRenderer::get_texture_bindless_index(self, handle)
    }

    fn default_texture(&self) -> TextureHandle {
        VulkanRenderer::default_texture(self)
    }

    fn compile_material(
        &mut self,
        descriptor: &PipelineDescriptor,
    ) -> Result<MaterialHandle, RendererError> {
        use crate::renderer::pipeline_descriptor::PipelineStages;

        descriptor.validate()?;
        if !descriptor.specialization.is_empty() {
            return Err(RendererError::UnsupportedFeature(
                "specialization constants are not yet plumbed into the Vulkan material compiler"
                    .to_string(),
            ));
        }
        let PipelineStages::Graphics { .. } = &descriptor.stages else {
            return Err(RendererError::UnsupportedFeature(
                "compute pipelines are not yet supported by compile_material".to_string(),
            ));
        };

        VulkanRenderer::compile_material_descriptor(self, descriptor)
    }

    fn set_material_textures(
        &mut self,
        material: MaterialHandle,
        textures: crate::renderer::registry::MaterialTextures,
    ) {
        VulkanRenderer::set_material_textures(self, material, textures);
    }

    fn recompile_materials_for_shader(&mut self, shader_path: &std::path::Path) -> usize {
        VulkanRenderer::recompile_materials_for_shader(self, shader_path)
    }

    fn destroy_mesh(&mut self, handle: MeshHandle) {
        VulkanRenderer::destroy_mesh(self, handle);
    }

    fn destroy_material(&mut self, handle: MaterialHandle) {
        VulkanRenderer::destroy_material(self, handle);
    }

    fn destroy_texture(&mut self, handle: TextureHandle) {
        VulkanRenderer::destroy_texture(self, handle);
    }

    fn destroy_skeleton(&mut self, handle: SkeletonHandle) {
        VulkanRenderer::destroy_skeleton(self, handle);
    }

    fn resize(&mut self, width: u32, height: u32) -> Result<(), RendererError> {
        VulkanRenderer::recreate_swapchain(self, Size2D::new(width, height))
    }

    fn create_skeleton(&mut self, joint_count: usize) -> Result<SkeletonHandle, RendererError> {
        VulkanRenderer::create_skeleton(self, joint_count)
    }
}
