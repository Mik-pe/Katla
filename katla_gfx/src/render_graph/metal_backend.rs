//! Metal backend for the render graph.
//!
//! Implements `RenderGraphBackend` for `MetalRenderer`, providing
//! concrete transient texture creation, bindless management, and
//! frame indexing using Metal GPU resources.

use objc2_metal::MTLBuffer;

use crate::metal::buffer::MetalGraphBuffer;
use crate::metal::metal_renderer::{FRAMES_IN_FLIGHT, MetalRenderer};
use crate::metal::metal_transient_texture::MetalTransientTexture;
use crate::render_graph::backend::{
    NativeTransientAllocation, RenderGraphBackend, TransientSlotPolicy,
};
use crate::render_graph::error::RenderGraphError;
use crate::render_graph::resource::{BufferDesc, BufferMemoryPolicy, GraphResourceDesc};
use crate::texture::ImageFormat;

impl RenderGraphBackend for MetalRenderer {
    fn graph_texture_upload_producers(
        &self,
    ) -> Vec<(
        crate::handle::TextureHandle,
        crate::texture::TextureUploadRegion,
    )> {
        crate::renderer::gpu_renderer::GpuRenderer::pending_texture_uploads(self)
    }

    type TransientTexture = MetalTransientTexture;
    type ImageView = crate::metal::texture::MetalTextureView;
    type TransientBuffer = MetalGraphBuffer;

    fn create_transient_slot(
        &self,
        members: &[GraphResourceDesc],
        policy: TransientSlotPolicy,
    ) -> Result<Vec<Self::TransientTexture>, RenderGraphError> {
        crate::metal::transient_heap::create_slot(&self.context.device, members, policy)
    }

    fn transient_allocation_info(
        texture: &Self::TransientTexture,
    ) -> Option<NativeTransientAllocation> {
        let allocation = &texture.allocation;
        log::debug!(
            "Inspect Metal allocation frame {} slot {} alias {}",
            allocation.frame_slot,
            allocation.slot,
            allocation.aliased
        );
        let identity = allocation
            .heap
            .as_ref()
            .map(|heap| objc2::rc::Retained::as_ptr(heap) as *const () as usize as u64)
            .unwrap_or_else(|| {
                objc2::rc::Retained::as_ptr(&texture.texture.inner) as *const () as usize as u64
            });
        Some(NativeTransientAllocation {
            identity,
            offset: allocation.offset,
            bytes: allocation.bytes,
            logical_bytes: allocation.logical_bytes,
            strategy: if allocation.memoryless {
                "memoryless"
            } else if allocation.heap.is_some() {
                "metal_placement_heap"
            } else {
                "private_standalone"
            },
        })
    }

    fn transient_buffer_allocation_info(
        buffer: &Self::TransientBuffer,
    ) -> Option<NativeTransientAllocation> {
        use crate::backend::resource::GpuBuffer;
        Some(NativeTransientAllocation {
            identity: buffer.buffer.inner.gpuAddress(),
            offset: buffer.offset,
            bytes: buffer.buffer.size(),
            logical_bytes: buffer.desc.size,
            strategy: if matches!(
                buffer.desc.memory,
                BufferMemoryPolicy::CpuVisible | BufferMemoryPolicy::Readback
            ) {
                "shared_buffer"
            } else {
                "private_buffer"
            },
        })
    }

    fn create_transient_buffer(
        &self,
        desc: BufferDesc,
    ) -> Result<Self::TransientBuffer, RenderGraphError> {
        let cpu_accessible = matches!(
            desc.memory,
            BufferMemoryPolicy::CpuVisible | BufferMemoryPolicy::Readback
        );
        let buffer = self
            .context
            .create_buffer(desc.size, cpu_accessible)
            .map_err(|error| RenderGraphError::BackendError(error.to_string()))?;
        Ok(MetalGraphBuffer::new(buffer, desc))
    }

    fn destroy_transient_texture(texture: Self::TransientTexture) {
        drop(texture);
    }

    fn destroy_transient_buffer(buffer: Self::TransientBuffer) {
        drop(buffer);
    }

    fn transient_buffer_size(buffer: &Self::TransientBuffer) -> u64 {
        buffer.size()
    }

    fn buffer_desc(buffer: &Self::TransientBuffer) -> BufferDesc {
        buffer.desc
    }

    fn buffer_by_handle(
        &self,
        handle: crate::handle::BufferHandle,
    ) -> Option<&Self::TransientBuffer> {
        self.graph_buffers.get(handle)
    }

    fn buffer_offset(buffer: &Self::TransientBuffer) -> u64 {
        buffer.offset
    }

    fn graph_buffer_previous_accesses(
        &self,
        buffer: &Self::TransientBuffer,
    ) -> Vec<crate::render_graph::BufferAccess> {
        use objc2_metal::MTLBuffer;
        self.buffer_history.borrow().previous(
            buffer.buffer.inner.gpuAddress(),
            buffer.offset,
            buffer.desc.size,
        )
    }
    fn record_graph_buffer_accesses(
        &self,
        buffer: &Self::TransientBuffer,
        accesses: &[crate::render_graph::BufferAccess],
    ) {
        self.pending_buffer_accesses.borrow_mut().push(
            crate::metal::frame_lifecycle::MetalBufferExecution {
                buffer: buffer.buffer.clone(),
                offset: buffer.offset,
                accesses: accesses.to_vec(),
            },
        );
    }

    fn prepare_compute_pipeline(
        &mut self,
        descriptor: &crate::render_graph::ComputePipelineDesc,
    ) -> Result<(), RenderGraphError> {
        if self.compute_pipelines.contains_key(descriptor) {
            return Ok(());
        }
        let interface = descriptor
            .interface()
            .map_err(RenderGraphError::BackendError)?;
        let shader = crate::metal::shader::compile_wgsl_to_metal(
            &self.context.device,
            &descriptor.wgsl,
            &[&descriptor.entry],
            crate::metal::shader::ShaderProfile::Graphics,
        )
        .map_err(|error| RenderGraphError::BackendError(error.to_string()))?;
        let function = shader
            .module
            .entry_points
            .get(&descriptor.entry)
            .ok_or_else(|| RenderGraphError::BackendError("compute entry point missing".into()))?;
        let mut pipeline = self
            .context
            .create_compute_pipeline(function, interface.workgroup_size)
            .map_err(|error| RenderGraphError::BackendError(error.to_string()))?;
        pipeline.uniform_bindings = interface
            .bindings
            .iter()
            .filter(|binding| binding.usage == crate::render_graph::BufferUsage::Uniform)
            .map(|binding| (binding.group, binding.binding))
            .collect();
        self.compute_pipelines.insert(descriptor.clone(), pipeline);
        Ok(())
    }

    fn current_frame(&self) -> usize {
        self.frame_index()
    }

    fn transient_texture_frames() -> usize {
        FRAMES_IN_FLIGHT
    }

    fn register_bindless_texture(
        &mut self,
        texture: &Self::TransientTexture,
    ) -> Result<u32, RenderGraphError> {
        if texture.allocation.memoryless {
            return Err(RenderGraphError::BackendError("Memoryless attachments cannot be registered for shader access; declare an exported resource before allocation".into()));
        }
        self.register_metal_bindless_texture(&texture.view.inner)
            .map_err(|e| RenderGraphError::BackendError(e.to_string()))
    }

    fn update_bindless_texture(
        &mut self,
        slot: u32,
        texture: &Self::TransientTexture,
    ) -> Result<(), RenderGraphError> {
        self.update_metal_bindless_texture(slot, &texture.view.inner)
            .map_err(|e| RenderGraphError::BackendError(e.to_string()))?;
        Ok(())
    }

    fn transient_texture_format(texture: &Self::TransientTexture) -> ImageFormat {
        texture.format
    }

    fn transient_texture_extent(texture: &Self::TransientTexture) -> (u32, u32) {
        (texture.width, texture.height)
    }

    fn transient_texture_is_depth(texture: &Self::TransientTexture) -> bool {
        matches!(
            texture.format,
            ImageFormat::D32Sfloat | ImageFormat::D32SfloatS8Uint | ImageFormat::D24UnormS8Uint
        )
    }

    fn transient_texture_bindless_slot(texture: &Self::TransientTexture) -> Option<u32> {
        texture.bindless_slot
    }

    fn set_transient_texture_bindless_slot(texture: &mut Self::TransientTexture, slot: u32) {
        texture.bindless_slot = Some(slot);
    }

    fn transient_texture_view(texture: &Self::TransientTexture) -> Self::ImageView {
        texture.view.clone()
    }

    fn swapchain_image_view(&self, _image_index: u32) -> Self::ImageView {
        self.drawable_texture_view
            .clone()
            .expect("No drawable texture view — render through an acquired frame")
    }
}
