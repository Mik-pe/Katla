//! Static-mesh buffer placement and staged uploads (issue #96).
//!
//! Immutable mesh data belongs in GPU-optimal memory, but device-local
//! buffers are usually not host-visible, so their contents arrive through a
//! staging copy instead of a direct mapping. [`StagedUploadBatch`] collects
//! every buffer of one mesh creation into a single staging allocation and a
//! single copy submission. The submission is ordered before everything the
//! application submits later on the graphics queue and each copy ends with
//! a barrier making its bytes visible to vertex/index reads, so any draw
//! after `create_mesh` returns sees the uploaded data — while the staging
//! allocation, fence, and command buffer stay alive until a frame-slot wait
//! observes completion (fail-loud creation semantics, no device round-trip
//! per mesh).
//!
//! Dynamic meshes never come through here: they stay in host-visible memory
//! for direct per-frame writes (`MeshUsage::Dynamic` →
//! [`BufferPlacement::HostVisible`]).
//!
//! Fallback: when the device-local allocation fails (no suitable heap, out
//! of memory), the batch retries in host-visible memory with a direct
//! write and logs the fallback; the selected placement is observable
//! through `VulkanRenderer::mesh_memory_report`.

use ash::vk;
use gpu_allocator::vulkan::Allocation;
use std::rc::Rc;

use super::context::VulkanContext;
use super::vertexbuffer::{IndexBuffer, IndexType, VertexBuffer};
use crate::RendererError;

/// Long-term memory placement for a mesh buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BufferPlacement {
    /// GPU-optimal memory populated through a staging copy.
    DeviceLocal,
    /// Host-visible memory for direct CPU writes.
    HostVisible,
}

impl BufferPlacement {
    /// Placement for a mesh upload policy.
    pub(crate) fn for_mesh_usage(usage: crate::renderer::registry::MeshUsage) -> Self {
        match usage {
            crate::renderer::registry::MeshUsage::Static => Self::DeviceLocal,
            crate::renderer::registry::MeshUsage::Dynamic => Self::HostVisible,
        }
    }
}

/// One staged `staging[src_offset..src_offset + size] → dst` copy.
struct CopyRecord {
    dst: vk::Buffer,
    src_offset: vk::DeviceSize,
    size: vk::DeviceSize,
}

/// Batches the vertex/index buffers of one mesh creation into one staging
/// allocation and one copy submission.
///
/// Push every buffer of the mesh, then call [`StagedUploadBatch::finish`];
/// keep the returned buffers alive until then (their `Drop` frees the
/// native objects, which the pending copies reference).
pub(crate) struct StagedUploadBatch {
    context: Rc<VulkanContext>,
    staged_bytes: Vec<u8>,
    copies: Vec<CopyRecord>,
}

impl StagedUploadBatch {
    pub(crate) fn new(context: Rc<VulkanContext>) -> Self {
        Self {
            context,
            staged_bytes: Vec::new(),
            copies: Vec::new(),
        }
    }

    /// Stage one vertex buffer: allocates the final buffer in device-local
    /// memory (falling back to host-visible with a direct write) and queues
    /// its bytes for the batch's copy submission.
    pub(crate) fn push_vertex(
        &mut self,
        bytes: &[u8],
        vertex_count: u32,
    ) -> Result<VertexBuffer, RendererError> {
        let usage = vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST;
        let (allocation_kind, buffer, allocation) =
            self.allocate_target(bytes.len() as vk::DeviceSize, usage, "mesh vertex buffer")?;
        let vertex_buffer = VertexBuffer::from_native(
            self.context.clone(),
            buffer,
            allocation,
            bytes.len() as vk::DeviceSize,
            vertex_count,
            usage,
            allocation_kind,
        );
        if allocation_kind == gpu_allocator::MemoryLocation::GpuOnly {
            self.stage(bytes, buffer);
        } else {
            vertex_buffer.write_host_visible(bytes)?;
        }
        Ok(vertex_buffer)
    }

    /// Stage one index buffer; see [`StagedUploadBatch::push_vertex`].
    pub(crate) fn push_index(
        &mut self,
        bytes: &[u8],
        index_type: IndexType,
        index_count: u32,
    ) -> Result<IndexBuffer, RendererError> {
        let usage = vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST;
        let (allocation_kind, buffer, allocation) =
            self.allocate_target(bytes.len() as vk::DeviceSize, usage, "mesh index buffer")?;
        let index_buffer = IndexBuffer::from_native(
            self.context.clone(),
            buffer,
            allocation,
            bytes.len() as vk::DeviceSize,
            index_type,
            index_count,
            usage,
            allocation_kind,
        );
        if allocation_kind == gpu_allocator::MemoryLocation::GpuOnly {
            self.stage(bytes, buffer);
        } else {
            index_buffer.write_host_visible(bytes)?;
        }
        Ok(index_buffer)
    }

    /// Submit every staged copy without blocking.
    ///
    /// The copies are ordered before all later submissions on the graphics
    /// queue, and each copy ends with a barrier making its bytes visible to
    /// vertex/index reads, so any draw submitted after this returns sees the
    /// uploaded data. The staging allocation and the submission's fence and
    /// command buffer stay alive in the context's pending list until a
    /// frame-slot wait observes completion — creation stays fail-loud and
    /// synchronous on the CPU without paying a device round-trip per mesh.
    pub(crate) fn finish(self) -> Result<(), RendererError> {
        if self.copies.is_empty() {
            return Ok(());
        }
        log::debug!(
            "staging {} mesh buffers ({} bytes) through one submission",
            self.copies.len(),
            self.staged_bytes.len()
        );

        let staging_size = self.staged_bytes.len() as vk::DeviceSize;
        let staging_info = vk::BufferCreateInfo::default()
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .size(staging_size);
        let (staging_buffer, staging_allocation) = self.context.allocate_buffer_named(
            &staging_info,
            gpu_allocator::MemoryLocation::CpuToGpu,
            "mesh upload staging",
        )?;

        // The staging buffer and every target buffer are freed by RAII on
        // any failure below, so a failed finish leaks nothing.
        let (fence, command_buffer) =
            self.record_and_submit(staging_buffer, &staging_allocation, staging_size)?;
        self.context
            .defer_staged_upload(fence, command_buffer, staging_buffer, staging_allocation);
        Ok(())
    }

    /// Allocate the final buffer, preferring device-local memory and
    /// falling back to host-visible (with the bytes written directly) when
    /// the preferred placement is unavailable.
    fn allocate_target(
        &self,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
        name: &str,
    ) -> Result<(gpu_allocator::MemoryLocation, vk::Buffer, Allocation), RendererError> {
        let create_info = vk::BufferCreateInfo::default()
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .usage(usage)
            .size(size);
        match self.context.allocate_buffer_named(
            &create_info,
            gpu_allocator::MemoryLocation::GpuOnly,
            name,
        ) {
            Ok((buffer, allocation)) => {
                Ok((gpu_allocator::MemoryLocation::GpuOnly, buffer, allocation))
            }
            Err(device_local_error) => {
                log::warn!(
                    "device-local allocation for {name} failed ({device_local_error}); \
                     falling back to host-visible mesh memory"
                );
                let (buffer, allocation) = self.context.allocate_buffer_named(
                    &create_info,
                    gpu_allocator::MemoryLocation::CpuToGpu,
                    name,
                )?;
                Ok((gpu_allocator::MemoryLocation::CpuToGpu, buffer, allocation))
            }
        }
    }

    /// Queue bytes for a device-local target and write them into the
    /// staging accumulator.
    fn stage(&mut self, bytes: &[u8], dst: vk::Buffer) {
        let src_offset = self.staged_bytes.len() as vk::DeviceSize;
        self.staged_bytes.extend_from_slice(bytes);
        self.copies.push(CopyRecord {
            dst,
            src_offset,
            size: bytes.len() as vk::DeviceSize,
        });
    }

    /// Record the copies, submit them under a fresh fence, and return the
    /// fence plus the recorded command buffer so their completion can be
    /// observed later.
    fn record_and_submit(
        &self,
        staging_buffer: vk::Buffer,
        staging_allocation: &Allocation,
        staging_size: vk::DeviceSize,
    ) -> Result<(vk::Fence, super::CommandBuffer), RendererError> {
        let mapped = staging_allocation
            .mapped_ptr()
            .map(|ptr| ptr.cast().as_ptr())
            .ok_or_else(|| {
                RendererError::InvalidOperation(
                    "mesh upload staging buffer is not mapped".to_string(),
                )
            })?;
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.staged_bytes.as_ptr(),
                mapped,
                self.staged_bytes.len(),
            );
        }
        self.context
            .flush_mapped_memory(staging_allocation, 0, staging_size)?;

        let cmd = self.context.begin_single_time_commands()?;
        unsafe {
            let vk_cmd = cmd.vk_command_buffer();
            for record in &self.copies {
                let region = vk::BufferCopy::default()
                    .src_offset(record.src_offset)
                    .dst_offset(0)
                    .size(record.size);
                self.context
                    .device
                    .cmd_copy_buffer(vk_cmd, staging_buffer, record.dst, &[region]);

                // Make the copied bytes visible to later vertex/index reads.
                let barrier = vk::BufferMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(
                        vk::AccessFlags::VERTEX_ATTRIBUTE_READ | vk::AccessFlags::INDEX_READ,
                    )
                    .buffer(record.dst)
                    .size(record.size);
                self.context.device.cmd_pipeline_barrier(
                    vk_cmd,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::VERTEX_INPUT,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[barrier],
                    &[],
                );
            }
        }
        cmd.end_single_time_command()?;

        let fence = unsafe {
            self.context
                .device
                .create_fence(&vk::FenceCreateInfo::default(), None)
                .map_err(|e| {
                    RendererError::VulkanError("Failed to create upload fence".into(), e)
                })?
        };
        self.context.gfx_queue.submit(&[&cmd], &[], &[], fence);
        Ok((fence, cmd))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::registry::{MeshUsage, PrimitiveTopology};

    #[test]
    fn test_placement_follows_mesh_usage() {
        assert_eq!(
            BufferPlacement::for_mesh_usage(MeshUsage::Static),
            BufferPlacement::DeviceLocal
        );
        assert_eq!(
            BufferPlacement::for_mesh_usage(MeshUsage::Dynamic),
            BufferPlacement::HostVisible
        );
    }

    /// Device-local allocation failure falls back to host-visible memory
    /// with a direct write; creation still succeeds (issue #96).
    #[test]
    #[ignore = "requires a Vulkan device"]
    fn test_device_local_failure_falls_back_to_host_visible() {
        let mut renderer = crate::renderer::VulkanRenderer::init_headless(
            64,
            48,
            crate::ValidationMode::Disabled,
            std::ffi::CString::new("staged upload fallback test").unwrap(),
            std::ffi::CString::new("Katla").unwrap(),
        )
        .unwrap();

        let (vertices, indices) = test_triangle();
        // Fail exactly the first device-local allocation: the position
        // buffer takes the fallback, the rest stay device-local.
        renderer.context.allocator.inject_allocation_failures(1);
        let mesh = renderer
            .create_mesh(&vertices, &indices, PrimitiveTopology::TriangleList)
            .expect("creation must succeed through the host-visible fallback");
        let report = renderer
            .mesh_memory_report(mesh)
            .expect("mesh must be live");
        assert!(
            report.host_visible_buffers >= 1,
            "the first buffer must report the fallback placement, got {report:?}"
        );
        assert!(report.device_local_buffers >= 1, "{report:?}");
        assert_eq!(renderer.mesh_vertex_count(mesh), Some(3));
        assert_eq!(renderer.mesh_index_count(mesh), Some(3));

        renderer.destroy_mesh(mesh);
        renderer.destroy();
    }

    /// Exhausting every allocation attempt fails creation typed without
    /// registering anything (issue #96).
    #[test]
    #[ignore = "requires a Vulkan device"]
    fn test_staged_creation_fails_typed_when_allocations_exhaust() {
        let mut renderer = crate::renderer::VulkanRenderer::init_headless(
            64,
            48,
            crate::ValidationMode::Disabled,
            std::ffi::CString::new("staged upload failure test").unwrap(),
            std::ffi::CString::new("Katla").unwrap(),
        )
        .unwrap();

        let (vertices, indices) = test_triangle();
        let meshes_before = renderer.asset_registry.mesh_count();

        // Enough injected failures to cover the device-local attempt and
        // the host-visible retry for the very first buffer.
        renderer.context.allocator.inject_allocation_failures(16);
        let error = renderer
            .create_mesh(&vertices, &indices, PrimitiveTopology::TriangleList)
            .unwrap_err();
        assert!(
            matches!(error, crate::RendererError::AllocationFailed { .. }),
            "got {error:?}"
        );
        assert_eq!(renderer.asset_registry.mesh_count(), meshes_before);

        renderer.destroy();
    }

    fn test_triangle() -> (Vec<crate::vertex::VertexPBR>, Vec<u32>) {
        use crate::vertex::VertexPBR;
        let v = |position: [f32; 3]| VertexPBR {
            position,
            normal: [0.0, 0.0, 1.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            tex_coord0: [0.0, 0.0],
        };
        (
            vec![
                v([-0.5, -0.5, 0.5]),
                v([0.5, -0.5, 0.5]),
                v([0.0, 0.5, 0.5]),
            ],
            vec![0, 1, 2],
        )
    }
}
