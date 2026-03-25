//! Forward+ tile-based light culling system.
//!
//! Divides the screen into tiles and computes per-tile lists of visible lights
//! using a compute shader, enabling efficient rendering of many dynamic lights
//! in a forward pipeline.
//!
//! Uses push descriptors (VK_KHR_push_descriptor) for both compute and fragment
//! descriptor sets, matching the particle system pattern. This avoids descriptor
//! pool management and enables per-dispatch binding.

use std::rc::Rc;

use ash::vk;
use bytemuck::{Pod, Zeroable};
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc};
use log::info;

use crate::vulkan::context::VulkanContext;

/// Maximum number of point lights supported.
pub const MAX_POINT_LIGHTS: u32 = 256;

/// Tile size in pixels (width and height).
pub const TILE_SIZE: u32 = 16;

/// Maximum number of lights per tile.
pub const MAX_LIGHTS_PER_TILE: u32 = 128;

/// GPU representation of a point light (32 bytes).
///
/// Must match WGSL `PointLightGPU` exactly.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct PointLightGPU {
    /// World-space position (x, y, z)
    pub position: [f32; 3],
    /// Packed: range (used as radius for culling)
    pub range: f32,
    /// RGB color (0-1)
    pub color: [f32; 3],
    /// Intensity multiplier
    pub intensity: f32,
}

/// Frame data for the light culling compute shader.
///
/// Must match WGSL `LightCullFrameData` exactly.
/// std140 rules: struct size must be a multiple of the largest member alignment (mat4x4f = 16).
/// Total: 64 + 64 + 7*4 + 4 pad = 160 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct LightCullFrameData {
    pub view_matrix: [f32; 16],
    pub proj_matrix: [f32; 16],
    pub light_count: u32,
    pub tiles_x: u32,
    pub tiles_y: u32,
    pub screen_width: u32,
    pub screen_height: u32,
    pub _pad0: u32,
    pub _pad1: u32,
    pub _pad2: u32,
}

/// GPU buffers for the light culling system.
///
/// Uses push descriptors for both compute (Set 0) and fragment (Set 3) binding.
/// No descriptor pools are allocated -- descriptors are pushed directly into
/// command buffers at record time via `cmd_push_descriptor_set_khr`.
pub struct LightCullingBuffers {
    context: Rc<VulkanContext>,

    /// Storage buffer: point light data array.
    light_buffer: vk::Buffer,
    light_allocation: Option<Allocation>,

    /// Storage buffer: per-tile visible light indices (u32 array).
    tile_index_buffer: vk::Buffer,
    tile_index_allocation: Option<Allocation>,

    /// Storage buffer: per-tile light counts (u32 array).
    tile_header_buffer: vk::Buffer,
    tile_header_allocation: Option<Allocation>,

    /// Uniform buffer for compute frame data (view/proj/tile params).
    frame_data_buffer: vk::Buffer,
    frame_data_allocation: Option<Allocation>,

    /// Number of tiles in X and Y.
    tiles_x: u32,
    tiles_y: u32,

    /// Screen dimensions.
    screen_width: u32,
    screen_height: u32,

    /// Number of lights currently uploaded.
    light_count: u32,

    /// Persistent mapping for light buffer (CPU writes).
    light_mapped_ptr: *mut u8,

    /// Push descriptor layout for compute pipeline (Set 0).
    compute_descriptor_layout: Option<vk::DescriptorSetLayout>,

    /// Push descriptor layout for fragment pipeline (Set 3).
    fragment_descriptor_layout: Option<vk::DescriptorSetLayout>,

    destroyed: bool,
}

unsafe impl Send for LightCullingBuffers {}
unsafe impl Sync for LightCullingBuffers {}

fn create_buffer(
    context: &Rc<VulkanContext>,
    name: &str,
    size: u64,
    usage: vk::BufferUsageFlags,
    location: gpu_allocator::MemoryLocation,
) -> Result<(vk::Buffer, Allocation), String> {
    let buffer_info = vk::BufferCreateInfo::default()
        .size(size)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);

    let buffer = unsafe {
        context
            .device
            .create_buffer(&buffer_info, None)
            .map_err(|e| format!("Failed to create buffer '{}': {:?}", name, e))?
    };

    let requirements = unsafe { context.device.get_buffer_memory_requirements(buffer) };

    let allocation = context
        .allocator
        .borrow_mut()
        .allocate(&AllocationCreateDesc {
            name,
            requirements,
            location,
            linear: true,
            allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
        })
        .map_err(|e| format!("Failed to allocate '{}': {}", name, e))?;

    unsafe {
        context
            .device
            .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
            .map_err(|e| format!("Failed to bind buffer '{}': {:?}", name, e))?;
    }

    Ok((buffer, allocation))
}

impl LightCullingBuffers {
    pub fn new(
        context: Rc<VulkanContext>,
        screen_width: u32,
        screen_height: u32,
    ) -> Result<Self, String> {
        let tiles_x = screen_width.div_ceil(TILE_SIZE);
        let tiles_y = screen_height.div_ceil(TILE_SIZE);
        let num_tiles = tiles_x * tiles_y;

        let light_buffer_size =
            (MAX_POINT_LIGHTS as u64) * (std::mem::size_of::<PointLightGPU>() as u64);
        let tile_index_size = (num_tiles as u64) * (MAX_LIGHTS_PER_TILE as u64) * 4;
        let tile_header_size = (num_tiles as u64) * 4;
        let frame_data_size = std::mem::size_of::<LightCullFrameData>() as u64;

        info!(
            "Creating light culling buffers: {}x{} screen, {}x{} tiles, \
             light={}KB, tile_idx={}KB, tile_hdr={}KB, frame_data={}KB",
            screen_width,
            screen_height,
            tiles_x,
            tiles_y,
            light_buffer_size / 1024,
            tile_index_size / 1024,
            tile_header_size / 1024,
            frame_data_size / 1024,
        );

        // Light buffer (CPU-writable, GPU-readable)
        let (light_buffer, light_allocation) = create_buffer(
            &context,
            "light_culling_light_buffer",
            light_buffer_size,
            vk::BufferUsageFlags::STORAGE_BUFFER,
            gpu_allocator::MemoryLocation::CpuToGpu,
        )?;

        let light_mapped_ptr = context.map_buffer(&light_allocation);
        unsafe {
            std::ptr::write_bytes(light_mapped_ptr, 0, light_buffer_size as usize);
        }

        // Tile index buffer (GPU-written, GPU-read)
        let (tile_index_buffer, tile_index_allocation) = create_buffer(
            &context,
            "light_culling_tile_index_buffer",
            tile_index_size,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            gpu_allocator::MemoryLocation::GpuOnly,
        )?;

        // Tile header buffer (GPU-cleared, GPU read/write)
        let (tile_header_buffer, tile_header_allocation) = create_buffer(
            &context,
            "light_culling_tile_header_buffer",
            tile_header_size,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            gpu_allocator::MemoryLocation::GpuOnly,
        )?;

        // Frame data buffer (CPU-writable, GPU-readable, uniform buffer)
        let (frame_data_buffer, frame_data_allocation) = create_buffer(
            &context,
            "light_culling_frame_data",
            frame_data_size,
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            gpu_allocator::MemoryLocation::CpuToGpu,
        )?;

        let device = &context.device;

        // Compute push descriptor layout (Set 0): light/tile/frame buffers
        // Uses PUSH_DESCRIPTOR_KHR -- no descriptor pool allocation needed
        let compute_bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(3)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];

        let compute_layout_info = vk::DescriptorSetLayoutCreateInfo::default()
            .bindings(&compute_bindings)
            .flags(vk::DescriptorSetLayoutCreateFlags::PUSH_DESCRIPTOR_KHR);

        let compute_descriptor_layout = unsafe {
            device
                .create_descriptor_set_layout(&compute_layout_info, None)
                .map_err(|e| format!("Failed to create compute push descriptor layout: {:?}", e))?
        };

        // Fragment push descriptor layout (Set 3): light/tile buffers only
        let fragment_bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        ];

        let fragment_layout_info = vk::DescriptorSetLayoutCreateInfo::default()
            .bindings(&fragment_bindings)
            .flags(vk::DescriptorSetLayoutCreateFlags::PUSH_DESCRIPTOR_KHR);

        let fragment_descriptor_layout = unsafe {
            device
                .create_descriptor_set_layout(&fragment_layout_info, None)
                .map_err(|e| format!("Failed to create fragment push descriptor layout: {:?}", e))?
        };

        Ok(Self {
            context,
            light_buffer,
            light_allocation: Some(light_allocation),
            tile_index_buffer,
            tile_index_allocation: Some(tile_index_allocation),
            tile_header_buffer,
            tile_header_allocation: Some(tile_header_allocation),
            frame_data_buffer,
            frame_data_allocation: Some(frame_data_allocation),
            tiles_x,
            tiles_y,
            screen_width,
            screen_height,
            light_count: 0,
            light_mapped_ptr,
            compute_descriptor_layout: Some(compute_descriptor_layout),
            fragment_descriptor_layout: Some(fragment_descriptor_layout),
            destroyed: false,
        })
    }

    /// Upload point light data to the GPU.
    pub fn upload_lights(&mut self, lights: &[PointLightGPU]) {
        self.light_count = lights.len().min(MAX_POINT_LIGHTS as usize) as u32;

        if !self.light_mapped_ptr.is_null() {
            let dst = unsafe {
                std::slice::from_raw_parts_mut(
                    self.light_mapped_ptr as *mut PointLightGPU,
                    MAX_POINT_LIGHTS as usize,
                )
            };
            for item in dst.iter_mut() {
                *item = PointLightGPU {
                    position: [0.0; 3],
                    range: 0.0,
                    color: [0.0; 3],
                    intensity: 0.0,
                };
            }
            dst[..self.light_count as usize].copy_from_slice(&lights[..self.light_count as usize]);
        }

        if let Some(ref alloc) = self.light_allocation {
            let flush_size =
                (MAX_POINT_LIGHTS as usize * std::mem::size_of::<PointLightGPU>()) as u64;
            self.context.flush_mapped_memory(alloc, 0, flush_size);
        }
    }

    /// Write frame data to the uniform buffer.
    pub fn write_frame_data(&mut self, frame_data: &LightCullFrameData) {
        if let Some(ref alloc) = self.frame_data_allocation
            && let Some(mapped) = alloc.mapped_ptr()
        {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    frame_data as *const LightCullFrameData as *const u8,
                    mapped.as_ptr() as *mut u8,
                    std::mem::size_of::<LightCullFrameData>(),
                );
            }
            self.context.flush_mapped_memory(
                alloc,
                0,
                std::mem::size_of::<LightCullFrameData>() as u64,
            );
        }
    }

    /// Clear tile light counts via GPU command (vkCmdFillBuffer).
    ///
    /// Call this before dispatching the light culling compute shader.
    /// Uses vkCmdFillBuffer to zero the tile header buffer on the GPU,
    /// avoiding CPU-GPU synchronization issues with mapped memory.
    pub fn record_clear_tile_headers(&self, cmd: vk::CommandBuffer) {
        let num_tiles = self.tiles_x * self.tiles_y;
        let fill_size = (num_tiles as u64) * (std::mem::size_of::<u32>() as u64);
        unsafe {
            self.context
                .device
                .cmd_fill_buffer(cmd, self.tile_header_buffer, 0, fill_size, 0);
        }
    }

    /// Push compute descriptors (Set 0) into a command buffer.
    ///
    /// Call this after binding the compute pipeline, before dispatching.
    /// Uses VK_KHR_push_descriptor -- no descriptor set allocation needed.
    pub fn push_compute_descriptors(
        &self,
        cmd: vk::CommandBuffer,
        pipeline_layout: vk::PipelineLayout,
    ) -> Result<(), String> {
        let push_descriptor = self
            .context
            .push_descriptor_khr
            .as_ref()
            .ok_or_else(|| "VK_KHR_push_descriptor not available".to_string())?;

        let light_info = [vk::DescriptorBufferInfo::default()
            .buffer(self.light_buffer)
            .offset(0)
            .range(std::mem::size_of::<PointLightGPU>() as u64 * MAX_POINT_LIGHTS as u64)];

        let tile_index_info = [vk::DescriptorBufferInfo::default()
            .buffer(self.tile_index_buffer)
            .offset(0)
            .range(vk::WHOLE_SIZE)];

        let tile_header_info = [vk::DescriptorBufferInfo::default()
            .buffer(self.tile_header_buffer)
            .offset(0)
            .range(vk::WHOLE_SIZE)];

        let frame_data_info = [vk::DescriptorBufferInfo::default()
            .buffer(self.frame_data_buffer)
            .offset(0)
            .range(std::mem::size_of::<LightCullFrameData>() as u64)];

        let writes = [
            vk::WriteDescriptorSet::default()
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&light_info),
            vk::WriteDescriptorSet::default()
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&tile_index_info),
            vk::WriteDescriptorSet::default()
                .dst_binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&tile_header_info),
            vk::WriteDescriptorSet::default()
                .dst_binding(3)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .buffer_info(&frame_data_info),
        ];

        unsafe {
            push_descriptor.cmd_push_descriptor_set(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                pipeline_layout,
                0, // Set 0
                &writes,
            );
        }

        Ok(())
    }

    /// Push fragment descriptors (Set 3) into a command buffer.
    ///
    /// Call this after binding the graphics pipeline, before drawing.
    /// Uses VK_KHR_push_descriptor -- no descriptor set allocation needed.
    pub fn push_fragment_descriptors(
        &self,
        cmd: vk::CommandBuffer,
        pipeline_layout: vk::PipelineLayout,
    ) -> Result<(), String> {
        let push_descriptor = self
            .context
            .push_descriptor_khr
            .as_ref()
            .ok_or_else(|| "VK_KHR_push_descriptor not available".to_string())?;

        let light_info = [vk::DescriptorBufferInfo::default()
            .buffer(self.light_buffer)
            .offset(0)
            .range(std::mem::size_of::<PointLightGPU>() as u64 * MAX_POINT_LIGHTS as u64)];

        let tile_index_info = [vk::DescriptorBufferInfo::default()
            .buffer(self.tile_index_buffer)
            .offset(0)
            .range(vk::WHOLE_SIZE)];

        let tile_header_info = [vk::DescriptorBufferInfo::default()
            .buffer(self.tile_header_buffer)
            .offset(0)
            .range(vk::WHOLE_SIZE)];

        let writes = [
            vk::WriteDescriptorSet::default()
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&light_info),
            vk::WriteDescriptorSet::default()
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&tile_index_info),
            vk::WriteDescriptorSet::default()
                .dst_binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&tile_header_info),
        ];

        unsafe {
            push_descriptor.cmd_push_descriptor_set(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline_layout,
                3, // Set 3
                &writes,
            );
        }

        Ok(())
    }

    pub fn compute_descriptor_layout(&self) -> Option<vk::DescriptorSetLayout> {
        self.compute_descriptor_layout
    }

    pub fn fragment_descriptor_layout(&self) -> Option<vk::DescriptorSetLayout> {
        self.fragment_descriptor_layout
    }

    pub fn frame_data_buffer(&self) -> vk::Buffer {
        self.frame_data_buffer
    }

    pub fn tiles_x(&self) -> u32 {
        self.tiles_x
    }

    pub fn tiles_y(&self) -> u32 {
        self.tiles_y
    }

    pub fn screen_width(&self) -> u32 {
        self.screen_width
    }

    pub fn screen_height(&self) -> u32 {
        self.screen_height
    }

    pub fn light_count(&self) -> u32 {
        self.light_count
    }

    pub fn destroy(&mut self) {
        if self.destroyed {
            return;
        }
        self.destroyed = true;

        unsafe {
            let device = &self.context.device;

            self.light_mapped_ptr = std::ptr::null_mut();

            if let Some(layout) = self.compute_descriptor_layout.take() {
                device.destroy_descriptor_set_layout(layout, None);
            }
            if let Some(layout) = self.fragment_descriptor_layout.take() {
                device.destroy_descriptor_set_layout(layout, None);
            }

            if let Some(alloc) = self.light_allocation.take() {
                device.destroy_buffer(self.light_buffer, None);
                let _ = self.context.allocator.borrow_mut().free(alloc);
            }
            if let Some(alloc) = self.tile_index_allocation.take() {
                device.destroy_buffer(self.tile_index_buffer, None);
                let _ = self.context.allocator.borrow_mut().free(alloc);
            }
            if let Some(alloc) = self.tile_header_allocation.take() {
                device.destroy_buffer(self.tile_header_buffer, None);
                let _ = self.context.allocator.borrow_mut().free(alloc);
            }
            if let Some(alloc) = self.frame_data_allocation.take() {
                device.destroy_buffer(self.frame_data_buffer, None);
                let _ = self.context.allocator.borrow_mut().free(alloc);
            }
        }
    }
}

impl Drop for LightCullingBuffers {
    fn drop(&mut self) {
        self.destroy();
    }
}
