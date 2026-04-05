//! Compute pipeline and buffer management for GPU pose evaluation.
//!
//! Provides `PoseComputePipeline` (pipeline, descriptor set layout, descriptor pool)
//! and `PoseComputeBuffers` (GPU storage buffers for skeleton animation data).

use std::rc::Rc;

use ash::vk;
use gpu_allocator::{
    MemoryLocation,
    vulkan::{Allocation, AllocationCreateDesc, AllocationScheme},
};
use log::{info, warn};

use crate::error::RendererError;
use crate::handle::PipelineHandle;
use crate::renderer::registry::AssetRegistry;
use crate::sync::{VkDescriptorSetLayout, VkShaderModule};
use crate::vulkan::context::VulkanContext;
use crate::vulkan::material::compute_pipeline::ComputePipelineBuilder;

use super::types::{AnimChannelInfo, AnimClipHeader, JointInfo, SkeletonAnimParams};

/// Workgroup size for pose compute shader (must match @workgroup_size in WGSL).
const POSE_COMPUTE_WORKGROUP_SIZE: u32 = 64;

/// Number of storage buffer bindings in the pose compute descriptor set.
const BINDING_COUNT: u32 = 8;

fn allocate_upload_buffer(
    context: &VulkanContext,
    size: u64,
    name: &str,
    buf_slot: &mut Option<vk::Buffer>,
    alloc_slot: &mut Option<Allocation>,
) -> Result<(), RendererError> {
    if size == 0 {
        return Ok(());
    }

    // Tear down previous allocation
    if let (Some(buf), Some(alloc)) = (buf_slot.take(), alloc_slot.take()) {
        context.free_buffer(buf, alloc);
    }

    let buffer_info = vk::BufferCreateInfo::default()
        .size(size)
        .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);

    let buffer = unsafe {
        context
            .device
            .create_buffer(&buffer_info, None)
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to create {} buffer: {:?}",
                    name, e
                ))
            })?
    };

    let requirements = unsafe { context.device.get_buffer_memory_requirements(buffer) };

    let allocation = context
        .allocator
        .borrow_mut()
        .allocate(&AllocationCreateDesc {
            name,
            requirements,
            location: MemoryLocation::CpuToGpu,
            linear: true,
            allocation_scheme: AllocationScheme::GpuAllocatorManaged,
        })
        .map_err(|e| {
            RendererError::InitializationFailed(format!(
                "Failed to allocate {} memory: {}",
                name, e
            ))
        })?;

    unsafe {
        context
            .device
            .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to bind {} memory: {:?}",
                    name, e
                ))
            })?;
    }

    *buf_slot = Some(buffer);
    *alloc_slot = Some(allocation);
    Ok(())
}

// ---------------------------------------------------------------------------
// PoseComputeBuffers
// ---------------------------------------------------------------------------

/// GPU buffers for the pose compute dispatch.
///
/// Layout (binding index → data):
///
/// | Binding | Name              | Direction | Description                     |
/// |---------|-------------------|-----------|---------------------------------|
/// | 0       | params            | CPU→GPU   | per-frame `SkeletonAnimParams`  |
/// | 1       | clip_headers      | GPU-only  | `AnimClipHeader` array          |
/// | 2       | channel_infos     | GPU-only  | `AnimChannelInfo` array         |
/// | 3       | keyframe_times    | GPU-only  | f32 keyframe timestamps         |
/// | 4       | keyframe_values   | GPU-only  | f32 keyframe values (Vec4)      |
/// | 5       | joints            | GPU-only  | `JointInfo` array               |
/// | 6       | world_matrices    | GPU       | scratch space for world transforms during hierarchy propagation |
/// | 7       | output            | GPU       | final joint matrices (compute writes, vertex reads) |
pub struct PoseComputeBuffers {
    context: Rc<VulkanContext>,

    // Binding 0 – CPU→GPU per-frame params
    params_buffer: Option<vk::Buffer>,
    params_allocation: Option<Allocation>,
    params_size: u64,

    // Binding 1 – static clip headers
    clip_headers_buffer: Option<vk::Buffer>,
    clip_headers_allocation: Option<Allocation>,

    // Binding 2 – static channel infos
    channel_infos_buffer: Option<vk::Buffer>,
    channel_infos_allocation: Option<Allocation>,

    // Binding 3 – static keyframe times
    keyframe_times_buffer: Option<vk::Buffer>,
    keyframe_times_allocation: Option<Allocation>,

    // Binding 4 – static keyframe values
    keyframe_values_buffer: Option<vk::Buffer>,
    keyframe_values_allocation: Option<Allocation>,

    // Binding 5 – static joints
    joints_buffer: Option<vk::Buffer>,
    joints_allocation: Option<Allocation>,

    // Binding 6 – world transform scratch space
    world_buffer: Option<vk::Buffer>,
    world_allocation: Option<Allocation>,

    // Binding 7 – GPU output: joint matrices
    output_buffer: Option<vk::Buffer>,
    output_allocation: Option<Allocation>,
    output_size: u64,

    destroyed: bool,
}

impl PoseComputeBuffers {
    pub fn new(context: Rc<VulkanContext>) -> Self {
        Self {
            context,
            params_buffer: None,
            params_allocation: None,
            params_size: 0,
            clip_headers_buffer: None,
            clip_headers_allocation: None,
            channel_infos_buffer: None,
            channel_infos_allocation: None,
            keyframe_times_buffer: None,
            keyframe_times_allocation: None,
            keyframe_values_buffer: None,
            keyframe_values_allocation: None,
            joints_buffer: None,
            joints_allocation: None,
            world_buffer: None,
            world_allocation: None,
            output_buffer: None,
            output_allocation: None,
            output_size: 0,
            destroyed: false,
        }
    }

    // -- Allocation helpers --------------------------------------------------

    /// Allocate (or reallocate) the CPU→GPU params buffer for `max_skeletons`.
    pub fn allocate_params(&mut self, max_skeletons: usize) -> Result<(), RendererError> {
        let size = (max_skeletons * std::mem::size_of::<SkeletonAnimParams>()) as u64;
        if size == 0 {
            return Ok(());
        }

        // Tear down previous allocation if any
        if let (Some(buf), Some(alloc)) = (self.params_buffer.take(), self.params_allocation.take())
        {
            self.context.free_buffer(buf, alloc);
        }

        let buffer_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(vk::BufferUsageFlags::STORAGE_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let (buffer, allocation) = self
            .context
            .allocate_buffer(&buffer_info, MemoryLocation::CpuToGpu)
            .expect("Failed to allocate pose compute params buffer");

        self.params_buffer = Some(buffer);
        self.params_allocation = Some(allocation);
        self.params_size = size;

        info!(
            "Allocated pose compute params buffer: {} bytes ({} skeletons)",
            size, max_skeletons
        );
        Ok(())
    }

    /// Allocate GPU-only buffers for static clip data.
    pub fn allocate_clip_data(
        &mut self,
        headers_size: u64,
        channels_size: u64,
        times_size: u64,
        values_size: u64,
    ) -> Result<(), RendererError> {
        allocate_upload_buffer(
            &self.context,
            headers_size,
            "pose_clip_headers",
            &mut self.clip_headers_buffer,
            &mut self.clip_headers_allocation,
        )?;
        allocate_upload_buffer(
            &self.context,
            channels_size,
            "pose_channel_infos",
            &mut self.channel_infos_buffer,
            &mut self.channel_infos_allocation,
        )?;
        allocate_upload_buffer(
            &self.context,
            times_size,
            "pose_keyframe_times",
            &mut self.keyframe_times_buffer,
            &mut self.keyframe_times_allocation,
        )?;
        allocate_upload_buffer(
            &self.context,
            values_size,
            "pose_keyframe_values",
            &mut self.keyframe_values_buffer,
            &mut self.keyframe_values_allocation,
        )?;

        info!(
            "Allocated pose compute clip data buffers: headers={} channels={} times={} values={}",
            headers_size, channels_size, times_size, values_size
        );
        Ok(())
    }

    /// Allocate GPU-only buffer for joint data.
    pub fn allocate_joints(&mut self, max_joints: usize) -> Result<(), RendererError> {
        let size = (max_joints * std::mem::size_of::<JointInfo>()) as u64;
        allocate_upload_buffer(
            &self.context,
            size,
            "pose_joints",
            &mut self.joints_buffer,
            &mut self.joints_allocation,
        )?;

        info!(
            "Allocated pose compute joints buffer: {} bytes ({} joints)",
            size, max_joints
        );
        Ok(())
    }

    /// Allocate GPU-only scratch buffer for world transforms (hierarchy propagation).
    pub fn allocate_world(&mut self, max_joints: usize) -> Result<(), RendererError> {
        let size = (max_joints * 64) as u64; // mat4x4<f32> = 64 bytes
        allocate_upload_buffer(
            &self.context,
            size,
            "pose_world",
            &mut self.world_buffer,
            &mut self.world_allocation,
        )?;

        info!(
            "Allocated pose compute world buffer: {} bytes ({} joints)",
            size, max_joints
        );
        Ok(())
    }

    /// Allocate GPU-only output buffer for final joint matrices.
    pub fn allocate_output(&mut self, max_joints: usize) -> Result<(), RendererError> {
        let size = (max_joints * 64) as u64; // mat4x4<f32> = 64 bytes
        if size == 0 {
            return Ok(());
        }

        if let (Some(buf), Some(alloc)) = (self.output_buffer.take(), self.output_allocation.take())
        {
            self.context.free_buffer(buf, alloc);
        }

        let buffer_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let (buffer, allocation) = self
            .context
            .allocate_buffer(&buffer_info, MemoryLocation::GpuOnly)
            .expect("Failed to allocate pose compute output buffer");

        self.output_buffer = Some(buffer);
        self.output_allocation = Some(allocation);
        self.output_size = size;

        info!(
            "Allocated pose compute output buffer: {} bytes ({} joints)",
            size, max_joints
        );
        Ok(())
    }

    // -- Upload helpers ------------------------------------------------------

    /// Write per-frame animation params to mapped memory.
    pub fn update_params(&self, params: &[SkeletonAnimParams]) {
        let alloc = match &self.params_allocation {
            Some(a) => a,
            None => return,
        };
        let byte_len = std::mem::size_of_val(params) as u64;
        if byte_len > self.params_size {
            warn!(
                "Params write exceeds buffer size ({} > {})",
                byte_len, self.params_size
            );
            return;
        }

        if let Some(mapped) = alloc.mapped_ptr() {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    params.as_ptr() as *const u8,
                    mapped.as_ptr() as *mut u8,
                    byte_len as usize,
                );
            }
            let _ = self.context.flush_mapped_memory(alloc, 0, byte_len);
        }
    }

    /// Upload clip headers, channel infos, keyframe times and values (once per model load).
    ///
    /// These buffers must have been allocated via `allocate_clip_data` with CpuToGpu memory
    /// so the CPU can write them before being used as GPU-only read-only resources.
    pub fn upload_clip_data(
        &self,
        headers: &[AnimClipHeader],
        channels: &[AnimChannelInfo],
        times: &[f32],
        values: &[f32],
    ) {
        self.write_to_buffer(
            &self.clip_headers_allocation,
            headers.as_ptr() as *const u8,
            std::mem::size_of_val(headers) as u64,
            "clip_headers",
        );
        self.write_to_buffer(
            &self.channel_infos_allocation,
            channels.as_ptr() as *const u8,
            std::mem::size_of_val(channels) as u64,
            "channel_infos",
        );
        self.write_to_buffer(
            &self.keyframe_times_allocation,
            times.as_ptr() as *const u8,
            std::mem::size_of_val(times) as u64,
            "keyframe_times",
        );
        self.write_to_buffer(
            &self.keyframe_values_allocation,
            values.as_ptr() as *const u8,
            std::mem::size_of_val(values) as u64,
            "keyframe_values",
        );
    }

    /// Upload joint data (once per model load).
    pub fn upload_joints(&self, joints: &[JointInfo]) {
        self.write_to_buffer(
            &self.joints_allocation,
            joints.as_ptr() as *const u8,
            std::mem::size_of_val(joints) as u64,
            "joints",
        );
    }

    // -- Buffer accessors ----------------------------------------------------

    pub fn params_buffer(&self) -> vk::Buffer {
        self.params_buffer.unwrap_or(vk::Buffer::null())
    }

    pub fn params_size(&self) -> u64 {
        self.params_size
    }

    pub fn clip_headers_buffer(&self) -> vk::Buffer {
        self.clip_headers_buffer.unwrap_or(vk::Buffer::null())
    }

    pub fn channel_infos_buffer(&self) -> vk::Buffer {
        self.channel_infos_buffer.unwrap_or(vk::Buffer::null())
    }

    pub fn keyframe_times_buffer(&self) -> vk::Buffer {
        self.keyframe_times_buffer.unwrap_or(vk::Buffer::null())
    }

    pub fn keyframe_values_buffer(&self) -> vk::Buffer {
        self.keyframe_values_buffer.unwrap_or(vk::Buffer::null())
    }

    pub fn joints_buffer(&self) -> vk::Buffer {
        self.joints_buffer.unwrap_or(vk::Buffer::null())
    }

    pub fn world_buffer(&self) -> vk::Buffer {
        self.world_buffer.unwrap_or(vk::Buffer::null())
    }

    pub fn output_buffer(&self) -> vk::Buffer {
        self.output_buffer.unwrap_or(vk::Buffer::null())
    }

    pub fn output_size(&self) -> u64 {
        self.output_size
    }

    // -- Internal helpers ----------------------------------------------------

    fn write_to_buffer(
        &self,
        allocation: &Option<Allocation>,
        src: *const u8,
        byte_len: u64,
        label: &str,
    ) {
        let alloc = match allocation {
            Some(a) => a,
            None => {
                warn!(
                    "Attempted to write {} data but buffer is not allocated",
                    label
                );
                return;
            }
        };

        if let Some(mapped) = alloc.mapped_ptr() {
            unsafe {
                std::ptr::copy_nonoverlapping(src, mapped.as_ptr() as *mut u8, byte_len as usize);
            }
            let _ = self.context.flush_mapped_memory(alloc, 0, byte_len);
        }
    }

    pub fn destroy(&mut self) {
        if self.destroyed {
            return;
        }
        self.destroyed = true;

        if let (Some(buf), Some(alloc)) = (self.params_buffer.take(), self.params_allocation.take())
        {
            self.context.free_buffer(buf, alloc);
        }
        if let (Some(buf), Some(alloc)) = (
            self.clip_headers_buffer.take(),
            self.clip_headers_allocation.take(),
        ) {
            self.context.free_buffer(buf, alloc);
        }
        if let (Some(buf), Some(alloc)) = (
            self.channel_infos_buffer.take(),
            self.channel_infos_allocation.take(),
        ) {
            self.context.free_buffer(buf, alloc);
        }
        if let (Some(buf), Some(alloc)) = (
            self.keyframe_times_buffer.take(),
            self.keyframe_times_allocation.take(),
        ) {
            self.context.free_buffer(buf, alloc);
        }
        if let (Some(buf), Some(alloc)) = (
            self.keyframe_values_buffer.take(),
            self.keyframe_values_allocation.take(),
        ) {
            self.context.free_buffer(buf, alloc);
        }
        if let (Some(buf), Some(alloc)) = (self.joints_buffer.take(), self.joints_allocation.take())
        {
            self.context.free_buffer(buf, alloc);
        }
        if let (Some(buf), Some(alloc)) = (self.world_buffer.take(), self.world_allocation.take()) {
            self.context.free_buffer(buf, alloc);
        }
        if let (Some(buf), Some(alloc)) = (self.output_buffer.take(), self.output_allocation.take())
        {
            self.context.free_buffer(buf, alloc);
        }
    }
}

impl Drop for PoseComputeBuffers {
    fn drop(&mut self) {
        self.destroy();
    }
}

// ---------------------------------------------------------------------------
// PoseComputePipeline
// ---------------------------------------------------------------------------

/// Manages the compute pipeline, descriptor set layout, pool, and set
/// for the GPU pose evaluation pass.
pub struct PoseComputePipeline {
    context: Rc<VulkanContext>,

    pipeline_handle: Option<PipelineHandle>,

    descriptor_layout: Option<vk::DescriptorSetLayout>,

    descriptor_pool: Option<vk::DescriptorPool>,
    descriptor_set: Option<vk::DescriptorSet>,

    destroyed: bool,
}

impl PoseComputePipeline {
    pub fn new(context: Rc<VulkanContext>) -> Self {
        Self {
            context,
            pipeline_handle: None,
            descriptor_layout: None,
            descriptor_pool: None,
            descriptor_set: None,
            destroyed: false,
        }
    }

    /// Create descriptor set layout, build the compute pipeline, register it,
    /// and allocate the descriptor pool + set.
    pub fn initialize(
        &mut self,
        asset_registry: &mut AssetRegistry,
        shader_module: VkShaderModule,
    ) -> Result<(), RendererError> {
        self.create_descriptor_layout()?;
        self.build_pipeline(asset_registry, shader_module)?;
        self.allocate_descriptor_set()?;
        info!("Pose compute pipeline initialized");
        Ok(())
    }

    /// Update the descriptor set with current buffer handles.
    pub fn update_bindings(&mut self, buffers: &PoseComputeBuffers) {
        let descriptor_set = match self.descriptor_set {
            Some(ds) => ds,
            None => return,
        };

        let buf_params = [vk::DescriptorBufferInfo {
            buffer: buffers.params_buffer(),
            offset: 0,
            range: buffers.params_size(),
        }];
        let buf_clip_headers = [vk::DescriptorBufferInfo {
            buffer: buffers.clip_headers_buffer(),
            offset: 0,
            range: vk::WHOLE_SIZE,
        }];
        let buf_channel_infos = [vk::DescriptorBufferInfo {
            buffer: buffers.channel_infos_buffer(),
            offset: 0,
            range: vk::WHOLE_SIZE,
        }];
        let buf_keyframe_times = [vk::DescriptorBufferInfo {
            buffer: buffers.keyframe_times_buffer(),
            offset: 0,
            range: vk::WHOLE_SIZE,
        }];
        let buf_keyframe_values = [vk::DescriptorBufferInfo {
            buffer: buffers.keyframe_values_buffer(),
            offset: 0,
            range: vk::WHOLE_SIZE,
        }];
        let buf_joints = [vk::DescriptorBufferInfo {
            buffer: buffers.joints_buffer(),
            offset: 0,
            range: vk::WHOLE_SIZE,
        }];
        let buf_world = [vk::DescriptorBufferInfo {
            buffer: buffers.world_buffer(),
            offset: 0,
            range: vk::WHOLE_SIZE,
        }];
        let buf_output = [vk::DescriptorBufferInfo {
            buffer: buffers.output_buffer(),
            offset: 0,
            range: buffers.output_size(),
        }];

        let descriptor_writes = [
            // Binding 0: params (CpuToGpu)
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&buf_params),
            // Binding 1: clip_headers
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&buf_clip_headers),
            // Binding 2: channel_infos
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&buf_channel_infos),
            // Binding 3: keyframe_times
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(3)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&buf_keyframe_times),
            // Binding 4: keyframe_values
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(4)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&buf_keyframe_values),
            // Binding 5: joints
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(5)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&buf_joints),
            // Binding 6: world_matrices (scratch)
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(6)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&buf_world),
            // Binding 7: output (joint matrices)
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(7)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&buf_output),
        ];

        unsafe {
            self.context
                .device
                .update_descriptor_sets(&descriptor_writes, &[]);
        }
    }

    /// Record a compute dispatch for pose evaluation.
    ///
    /// Binds the pipeline and descriptor set, then dispatches
    /// `(skeleton_count + 63) / 64` workgroups.
    pub fn record_dispatch(
        &self,
        cmd: vk::CommandBuffer,
        asset_registry: &AssetRegistry,
        skeleton_count: u32,
    ) {
        let handle = match self.pipeline_handle {
            Some(h) => h,
            None => return,
        };

        let pipeline = match asset_registry.get_pipeline(handle) {
            Some(p) => p,
            None => return,
        };

        let vk_pipeline = pipeline.vk_pipeline();
        let vk_layout = pipeline.vk_layout();

        unsafe {
            self.context
                .device
                .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, vk_pipeline);
        }

        if let Some(descriptor_set) = self.descriptor_set {
            unsafe {
                self.context.device.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::COMPUTE,
                    vk_layout,
                    0,
                    std::slice::from_ref(&descriptor_set),
                    &[],
                );
            }
        }

        let workgroups = skeleton_count.div_ceil(POSE_COMPUTE_WORKGROUP_SIZE);
        unsafe {
            self.context.device.cmd_dispatch(cmd, workgroups, 1, 1);
        }
    }

    /// Insert a buffer memory barrier on the output matrices buffer so that
    /// the data written by the compute shader is visible to `dst_stage`.
    ///
    /// The `dst_access` should match how the buffer will be used next:
    /// - `SHADER_READ` for vertex shader consumption
    /// - `TRANSFER_READ` for buffer copy operations
    pub fn add_output_barrier(
        &self,
        cmd: vk::CommandBuffer,
        buffers: &PoseComputeBuffers,
        dst_stage: vk::PipelineStageFlags2,
        dst_access: vk::AccessFlags2,
    ) {
        let output_buf = buffers.output_buffer();
        if output_buf == vk::Buffer::null() {
            return;
        }

        let barrier = vk::BufferMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .dst_stage_mask(dst_stage)
            .src_access_mask(vk::AccessFlags2::SHADER_WRITE)
            .dst_access_mask(dst_access)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(output_buf)
            .offset(0)
            .size(vk::WHOLE_SIZE);

        let barriers = [barrier];
        let dep_info = vk::DependencyInfo::default().buffer_memory_barriers(&barriers);

        unsafe {
            self.context.device.cmd_pipeline_barrier2(cmd, &dep_info);
        }
    }

    /// Get the pipeline handle registered in the asset registry.
    pub fn pipeline_handle(&self) -> Option<PipelineHandle> {
        self.pipeline_handle
    }

    pub fn destroy(&mut self) {
        if self.destroyed {
            return;
        }
        self.destroyed = true;

        if let Some(pool) = self.descriptor_pool.take() {
            unsafe {
                self.context.device.destroy_descriptor_pool(pool, None);
            }
        }
        if let Some(layout) = self.descriptor_layout.take() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(layout, None);
            }
        }
    }

    // -- Private helpers -----------------------------------------------------

    fn create_descriptor_layout(&mut self) -> Result<(), RendererError> {
        let bindings: [vk::DescriptorSetLayoutBinding; BINDING_COUNT as usize] =
            std::array::from_fn(|i| {
                vk::DescriptorSetLayoutBinding::default()
                    .binding(i as u32)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
            });

        let binding_flags: [vk::DescriptorBindingFlags; BINDING_COUNT as usize] =
            std::array::from_fn(|_| vk::DescriptorBindingFlags::UPDATE_AFTER_BIND);

        let mut binding_flags_info =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::default().binding_flags(&binding_flags);

        let layout_info = vk::DescriptorSetLayoutCreateInfo::default()
            .bindings(&bindings)
            .flags(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL)
            .push_next(&mut binding_flags_info);

        let layout = unsafe {
            self.context
                .device
                .create_descriptor_set_layout(&layout_info, None)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to create pose compute descriptor layout: {:?}",
                        e
                    ))
                })?
        };

        self.descriptor_layout = Some(layout);
        Ok(())
    }

    fn build_pipeline(
        &mut self,
        asset_registry: &mut AssetRegistry,
        shader_module: VkShaderModule,
    ) -> Result<(), RendererError> {
        let layout = self.descriptor_layout.ok_or_else(|| {
            RendererError::InitializationFailed("Pose compute descriptor layout not created".into())
        })?;

        let compute_pipeline = ComputePipelineBuilder::new(self.context.clone())
            .with_shader(shader_module)
            .with_descriptor_layouts(vec![VkDescriptorSetLayout(layout)])
            .build()
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to build pose compute pipeline: {}",
                    e
                ))
            })?;

        let handle = asset_registry.register_compute_pipeline(compute_pipeline);
        self.pipeline_handle = Some(handle);
        Ok(())
    }

    fn allocate_descriptor_set(&mut self) -> Result<(), RendererError> {
        let layout = self.descriptor_layout.ok_or_else(|| {
            RendererError::InitializationFailed("Pose compute descriptor layout not created".into())
        })?;

        let pool_sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(BINDING_COUNT)];

        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(1)
            .flags(
                vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET
                    | vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND,
            );

        let pool = unsafe {
            self.context
                .device
                .create_descriptor_pool(&pool_info, None)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to create pose compute descriptor pool: {:?}",
                        e
                    ))
                })?
        };

        let set_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(pool)
            .set_layouts(std::slice::from_ref(&layout));

        let sets = unsafe {
            self.context
                .device
                .allocate_descriptor_sets(&set_info)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to allocate pose compute descriptor set: {:?}",
                        e
                    ))
                })?
        };

        self.descriptor_pool = Some(pool);
        self.descriptor_set = Some(sets[0]);
        Ok(())
    }
}

impl Drop for PoseComputePipeline {
    fn drop(&mut self) {
        self.destroy();
    }
}
