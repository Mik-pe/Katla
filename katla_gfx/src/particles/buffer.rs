//! Global particle buffer with index list management and atomic counters.

use std::rc::Rc;

use ash::vk;
use bytemuck::{Pod, Zeroable};
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc};
use log::info;

use crate::vulkan::context::VulkanContext;

/// Particle data structure (48 bytes, tightly packed).
///
/// Layout must match WGSL struct exactly.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ParticleData {
    /// World position (x, y, z)
    pub position: [f32; 3],
    /// Scale factor
    pub scale: f32,
    /// Velocity (x, y, z)
    pub velocity: [f32; 3],
    /// Remaining lifetime in seconds
    pub lifetime: f32,
    /// RGBA color (0-1 range)
    pub color: [f32; 4],
}

/// Per-frame data for particle simulation (updated via push descriptors).
///
/// Must match WGSL `FrameData` exactly (32 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct FrameData {
    pub delta_time: f32,
    pub total_emit_count: u32,
    pub emitter_count: u32,
    pub random_seed: u32,
    pub total_simulate_count: u32,
    pub burst_count: u32,
    pub frame_index: u32,
    pub _pad: u32,
}

/// Atomic counters for particle management (16 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ParticleCounters {
    /// Number of alive particles (atomic)
    pub alive_count: u32,
    /// Number of dead particles (atomic, starts at MAX_PARTICLES)
    pub dead_count: u32,
    /// Number of newly emitted particles this frame (set by emit, read by simulate)
    pub emit_count: u32,
    /// Number of workgroups that completed simulate processing.
    /// Used by the last workgroup to write the indirect draw command.
    pub workgroups_finished: u32,
}

/// Pre-computed buffer layout offsets for the global particle buffer.
///
/// Eliminates duplicated offset arithmetic across buffer.rs, mod.rs, and debug_readback.rs.
/// Computed once in `GlobalParticleBuffer::new()` and stored for reuse.
#[derive(Clone, Copy, Debug)]
pub struct ParticleBufferLayout {
    /// Size of the particle data region (aligned to 64 bytes)
    pub particles_size_aligned: u64,
    /// Size of the dead list region (aligned to 64 bytes)
    pub dead_list_size_aligned: u64,
    /// Size of a single alive list region (unaligned)
    pub alive_list_size: u64,
    /// Total particle buffer size
    pub total_size: u64,
    /// Byte offset where dead list begins
    pub dead_list_offset: u64,
    /// Byte offset where alive_current[0] begins
    pub alive_current_offset: u64,
    /// Byte offsets for alive_current per frame (2 frames in flight)
    pub alive_current_frame_offset: [u64; 2],
    /// Byte offset where alive_next begins
    pub alive_next_offset: u64,
    /// Maximum particles
    pub max_particles: u64,
}

impl ParticleBufferLayout {
    /// Compute buffer layout for a given particle count.
    ///
    /// Aligns each region to 64 bytes for `min_storage_buffer_offset_alignment` compliance.
    /// Alive list regions are identical in size and don't need inter-region padding.
    pub fn new(max_particles: u32) -> Self {
        let max_particles = max_particles as u64;
        let particle_data_size = max_particles * std::mem::size_of::<ParticleData>() as u64;
        let particles_size_aligned = (particle_data_size + 63) & !63;

        let dead_list_size = max_particles * std::mem::size_of::<u32>() as u64;
        let dead_list_size_aligned = (dead_list_size + 63) & !63;

        let alive_list_size = max_particles * std::mem::size_of::<u32>() as u64;

        let dead_list_offset = particles_size_aligned;
        let alive_current_offset = dead_list_offset + dead_list_size_aligned;
        let alive_current_frame_offset =
            [alive_current_offset, alive_current_offset + alive_list_size];
        let alive_next_offset = alive_current_offset + 2 * alive_list_size;

        let total_size = alive_next_offset + alive_list_size;

        Self {
            particles_size_aligned,
            dead_list_size_aligned,
            alive_list_size,
            total_size,
            dead_list_offset,
            alive_current_offset,
            alive_current_frame_offset,
            alive_next_offset,
            max_particles,
        }
    }
}

/// Global particle buffer with all particle data and management structures.
///
/// Memory layout:
/// - Particle data: 48 MB (1M × 48 bytes)
/// - Dead list: 4 MB (1M × 4 bytes)
/// - Alive list current (per-frame): 8 MB (2 × 4 MB for 2 frames in flight)
/// - Alive list next: 4 MB
///   Total: ~64 MB
///
/// Counters, indirect draw, and emitter configs use separate buffers.
pub struct GlobalParticleBuffer {
    context: Rc<VulkanContext>,

    /// Main particle storage buffer
    particle_buffer: vk::Buffer,
    particle_allocation: Option<Allocation>,

    /// Atomic counters
    counters_buffer: vk::Buffer,
    counters_allocation: Option<Allocation>,

    /// Indirect draw command buffer (written by simulate shader, read by vkCmdDrawIndirect)
    indirect_draw_buffer: vk::Buffer,
    indirect_draw_allocation: Option<Allocation>,

    /// Maximum particles
    max_particles: u32,

    /// Pre-computed buffer layout offsets
    layout: ParticleBufferLayout,

    /// Flag to prevent double destruction
    destroyed: bool,
}

impl GlobalParticleBuffer {
    /// Maximum particles supported by shaders (must match MAX_PARTICLES in WGSL)
    const SHADER_MAX_PARTICLES: u32 = 1_048_576;

    /// Create a new global particle buffer.
    pub fn new(context: Rc<VulkanContext>, max_particles: u32) -> Result<Self, String> {
        // Validate max_particles parameter to prevent allocation failures and shader overflow
        if max_particles == 0 {
            return Err("max_particles must be greater than 0".to_string());
        }
        if max_particles > Self::SHADER_MAX_PARTICLES {
            return Err(format!(
                "max_particles ({}) exceeds shader limit ({}), please update shaders if more particles are needed",
                max_particles,
                Self::SHADER_MAX_PARTICLES
            ));
        }

        let layout = ParticleBufferLayout::new(max_particles);

        let particle_buffer_info = vk::BufferCreateInfo::default()
            .size(layout.total_size)
            .usage(
                vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_DST
                    | vk::BufferUsageFlags::TRANSFER_SRC
                    | vk::BufferUsageFlags::VERTEX_BUFFER,
            )
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let particle_buffer = unsafe {
            context
                .device
                .create_buffer(&particle_buffer_info, None)
                .map_err(|e| format!("Failed to create particle buffer: {:?}", e))?
        };

        let particle_requirements = unsafe {
            context
                .device
                .get_buffer_memory_requirements(particle_buffer)
        };

        let particle_allocation = context
            .allocator
            .borrow_mut()
            .allocate(&AllocationCreateDesc {
                name: "global_particle_buffer",
                requirements: particle_requirements,
                location: gpu_allocator::MemoryLocation::GpuOnly,
                linear: true,
                allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| format!("Failed to allocate particle memory: {}", e))?;

        unsafe {
            context
                .device
                .bind_buffer_memory(
                    particle_buffer,
                    particle_allocation.memory(),
                    particle_allocation.offset(),
                )
                .map_err(|e| format!("Failed to bind particle memory: {:?}", e))?
        }

        // Create counters buffer (CPU-visible for initialization, with transfer support for readback)
        let counters_size = std::mem::size_of::<ParticleCounters>();
        let counters_buffer_info = vk::BufferCreateInfo::default()
            .size(counters_size as u64)
            .usage(
                vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::UNIFORM_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_SRC
                    | vk::BufferUsageFlags::TRANSFER_DST,
            )
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let counters_buffer = unsafe {
            context
                .device
                .create_buffer(&counters_buffer_info, None)
                .map_err(|e| format!("Failed to create counters buffer: {:?}", e))?
        };

        let counters_requirements = unsafe {
            context
                .device
                .get_buffer_memory_requirements(counters_buffer)
        };

        let counters_allocation = context
            .allocator
            .borrow_mut()
            .allocate(&AllocationCreateDesc {
                name: "particle_counters",
                requirements: counters_requirements,
                location: gpu_allocator::MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| format!("Failed to allocate counters memory: {}", e))?;

        unsafe {
            context
                .device
                .bind_buffer_memory(
                    counters_buffer,
                    counters_allocation.memory(),
                    counters_allocation.offset(),
                )
                .map_err(|e| format!("Failed to bind counters memory: {:?}", e))?
        }

        // Initialize counters
        if let Some(mapped) = counters_allocation.mapped_ptr() {
            let counters = ParticleCounters {
                alive_count: 0,
                dead_count: max_particles,
                emit_count: 0,
                workgroups_finished: 0,
            };
            log::debug!(
                "Initialized counters: alive={}, dead={}",
                counters.alive_count,
                counters.dead_count
            );
            unsafe {
                std::ptr::copy_nonoverlapping(
                    &counters as *const ParticleCounters as *const u8,
                    mapped.as_ptr() as *mut u8,
                    std::mem::size_of::<ParticleCounters>(),
                );
            }
            context.flush_mapped_memory(
                &counters_allocation,
                0,
                std::mem::size_of::<ParticleCounters>() as u64,
            );
        }

        // Create indirect draw command buffer (16 bytes = one VkDrawIndirectCommand).
        // Written by simulate compute shader as STORAGE_BUFFER, read by render as INDIRECT_BUFFER.
        let indirect_draw_size: u64 = 16;
        let indirect_draw_buffer_info = vk::BufferCreateInfo::default()
            .size(indirect_draw_size)
            .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::INDIRECT_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let indirect_draw_buffer = unsafe {
            context
                .device
                .create_buffer(&indirect_draw_buffer_info, None)
                .map_err(|e| format!("Failed to create indirect draw buffer: {:?}", e))?
        };

        let indirect_draw_requirements = unsafe {
            context
                .device
                .get_buffer_memory_requirements(indirect_draw_buffer)
        };

        let indirect_draw_allocation = context
            .allocator
            .borrow_mut()
            .allocate(&AllocationCreateDesc {
                name: "particle_indirect_draw",
                requirements: indirect_draw_requirements,
                location: gpu_allocator::MemoryLocation::GpuOnly,
                linear: true,
                allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| format!("Failed to allocate indirect draw memory: {}", e))?;

        unsafe {
            context
                .device
                .bind_buffer_memory(
                    indirect_draw_buffer,
                    indirect_draw_allocation.memory(),
                    indirect_draw_allocation.offset(),
                )
                .map_err(|e| format!("Failed to bind indirect draw memory: {:?}", e))?
        }

        info!(
            "Created global particle buffer: {} particles ({} MB)",
            max_particles,
            (layout.particles_size_aligned as usize
                + max_particles as usize * std::mem::size_of::<u32>() * 4
                + counters_size)
                / (1024 * 1024)
        );
        // Validate buffer alignments for descriptor set offsets
        let device_properties = unsafe {
            context
                .instance
                .get_physical_device_properties(context.physical_device)
        };

        let min_storage_buffer_offset_alignment =
            device_properties.limits.min_storage_buffer_offset_alignment;

        // Validate descriptor buffer offsets are properly aligned (must use same aligned sizes as buffer creation)
        let offsets = [
            ("particle data", 0u64),
            ("dead list", layout.dead_list_offset),
            ("alive_current", layout.alive_current_offset),
            ("alive_next", layout.alive_next_offset),
        ];

        for (name, offset) in offsets.iter() {
            if offset % min_storage_buffer_offset_alignment != 0 {
                return Err(format!(
                    "Buffer offset for {} ({}) is not aligned to min_storage_buffer_offset_alignment ({})",
                    name, offset, min_storage_buffer_offset_alignment
                ));
            }
        }

        Ok(Self {
            context,
            particle_buffer,
            particle_allocation: Some(particle_allocation),
            counters_buffer,
            counters_allocation: Some(counters_allocation),
            indirect_draw_buffer,
            indirect_draw_allocation: Some(indirect_draw_allocation),
            max_particles,
            layout,
            destroyed: false,
        })
    }

    /// Initialize all index lists (dead list starts full, alive lists start empty).
    pub fn initialize_index_lists(&self) -> Result<(), String> {
        let cmd = self.context.begin_single_time_commands();

        // Zero-fill the particle data region
        unsafe {
            self.context.device.cmd_fill_buffer(
                cmd.vk_command_buffer(),
                self.particle_buffer,
                0,
                self.layout.particles_size_aligned,
                0,
            );
        }

        // Initialize dead list with indices 0..MAX_PARTICLES
        // All particles start in the dead list, ready to be allocated
        let indices: Vec<u32> = (0..self.max_particles).collect();
        let dead_list_data: Vec<u8> = indices
            .iter()
            .flat_map(|i| i.to_le_bytes().to_vec())
            .collect();

        let dead_list_size = dead_list_data.len() as u64;

        // Create staging buffer for dead list initialization
        let staging_buffer_info = vk::BufferCreateInfo::default()
            .size(dead_list_size)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let staging_buffer = unsafe {
            self.context
                .device
                .create_buffer(&staging_buffer_info, None)
                .map_err(|e| format!("Failed to create staging buffer: {:?}", e))?
        };

        let staging_requirements = unsafe {
            self.context
                .device
                .get_buffer_memory_requirements(staging_buffer)
        };

        let staging_allocation = self
            .context
            .allocator
            .borrow_mut()
            .allocate(&AllocationCreateDesc {
                name: "particle_dead_list_staging",
                requirements: staging_requirements,
                location: gpu_allocator::MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| format!("Failed to allocate staging memory: {}", e))?;

        unsafe {
            self.context
                .device
                .bind_buffer_memory(
                    staging_buffer,
                    staging_allocation.memory(),
                    staging_allocation.offset(),
                )
                .map_err(|e| format!("Failed to bind staging memory: {:?}", e))?
        }

        // Copy data to staging buffer
        if let Some(mapped) = staging_allocation.mapped_ptr() {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    dead_list_data.as_ptr(),
                    mapped.as_ptr() as *mut u8,
                    dead_list_data.len(),
                );
            }
            self.context
                .flush_mapped_memory(&staging_allocation, 0, dead_list_size);
        }

        // Copy from staging to dead list
        unsafe {
            let copy_region = vk::BufferCopy::default()
                .src_offset(0)
                .dst_offset(self.layout.dead_list_offset)
                .size(dead_list_size);

            self.context.device.cmd_copy_buffer(
                cmd.vk_command_buffer(),
                staging_buffer,
                self.particle_buffer,
                std::slice::from_ref(&copy_region),
            );
        }

        self.context.end_single_time_commands(cmd);

        // Cleanup staging buffer
        unsafe {
            self.context.device.destroy_buffer(staging_buffer, None);
        }
        if let Ok(mut allocator) = self.context.allocator.try_borrow_mut() {
            allocator.free(staging_allocation).ok();
        }

        // Initialize alive lists to zero (empty on startup)
        let cmd = self.context.begin_single_time_commands();

        unsafe {
            // Fill both alive_current regions (one per frame) with zeros
            for frame_idx in 0..2 {
                self.context.device.cmd_fill_buffer(
                    cmd.vk_command_buffer(),
                    self.particle_buffer,
                    self.layout.alive_current_frame_offset[frame_idx],
                    self.layout.alive_list_size,
                    0,
                );
            }

            // Fill alive_next with zeros
            self.context.device.cmd_fill_buffer(
                cmd.vk_command_buffer(),
                self.particle_buffer,
                self.layout.alive_next_offset,
                self.layout.alive_list_size,
                0,
            );
        }

        self.context.end_single_time_commands(cmd);

        // Initialize atomic counters
        let cmd = self.context.begin_single_time_commands();
        let counters_data = ParticleCounters {
            alive_count: 0,
            dead_count: self.max_particles,
            emit_count: 0,
            workgroups_finished: 0,
        };
        let counters_bytes: Vec<u8> = bytemuck::bytes_of(&counters_data).to_vec();

        // Create staging buffer for counters initialization
        let staging_buffer_info = vk::BufferCreateInfo::default()
            .size(counters_bytes.len() as u64)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let staging_buffer = unsafe {
            self.context
                .device
                .create_buffer(&staging_buffer_info, None)
                .map_err(|e| format!("Failed to create counters staging buffer: {:?}", e))?
        };
        let staging_requirements = unsafe {
            self.context
                .device
                .get_buffer_memory_requirements(staging_buffer)
        };
        let staging_allocation = self
            .context
            .allocator
            .borrow_mut()
            .allocate(&AllocationCreateDesc {
                name: "particle_counters_staging",
                requirements: staging_requirements,
                location: gpu_allocator::MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| format!("Failed to allocate counters staging memory: {}", e))?;

        unsafe {
            self.context
                .device
                .bind_buffer_memory(
                    staging_buffer,
                    staging_allocation.memory(),
                    staging_allocation.offset(),
                )
                .map_err(|e| format!("Failed to bind counters staging memory: {:?}", e))?
        }

        // Copy counters data to staging buffer
        if let Some(mapped) = staging_allocation.mapped_ptr() {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    counters_bytes.as_ptr(),
                    mapped.as_ptr() as *mut u8,
                    counters_bytes.len(),
                );
            }
            self.context
                .flush_mapped_memory(&staging_allocation, 0, counters_bytes.len() as u64);
        }

        // Copy from staging to counters buffer
        unsafe {
            let copy_region = vk::BufferCopy::default()
                .src_offset(0)
                .dst_offset(0)
                .size(counters_bytes.len() as u64);
            self.context.device.cmd_copy_buffer(
                cmd.vk_command_buffer(),
                staging_buffer,
                self.counters_buffer,
                std::slice::from_ref(&copy_region),
            );
        }

        self.context.end_single_time_commands(cmd);

        // Cleanup staging buffer
        unsafe {
            self.context.device.destroy_buffer(staging_buffer, None);
        }
        if let Ok(mut allocator) = self.context.allocator.try_borrow_mut() {
            allocator.free(staging_allocation).ok();
        }

        info!(
            "Initialized particle index lists: dead={}, alive_current[2]={}, alive_next={} ({} MB total)",
            self.max_particles,
            0,
            0,
            (self.layout.alive_list_size * 4) / (1024 * 1024)
        );
        Ok(())
    }

    /// Get current alive particle count.
    ///
    /// Invalidates mapped memory before reading to ensure GPU writes are visible.
    /// Must be called after the GPU command buffer that wrote to counters has completed.
    pub fn get_alive_count(&self) -> Result<u32, String> {
        if let Some(counters_allocation) = &self.counters_allocation {
            self.context.invalidate_mapped_memory(
                counters_allocation,
                0,
                std::mem::size_of::<ParticleCounters>() as u64,
            );
            if let Some(mapped) = counters_allocation.mapped_ptr() {
                let counters = unsafe { &*(mapped.as_ptr() as *const ParticleCounters) };
                return Ok(counters.alive_count);
            }
        }
        Ok(0)
    }

    /// Get current dead particle count.
    ///
    /// Invalidates mapped memory before reading to ensure GPU writes are visible.
    /// Must be called after the GPU command buffer that wrote to counters has completed.
    pub fn get_dead_count(&self) -> Result<u32, String> {
        if let Some(counters_allocation) = &self.counters_allocation {
            self.context.invalidate_mapped_memory(
                counters_allocation,
                0,
                std::mem::size_of::<ParticleCounters>() as u64,
            );
            if let Some(mapped) = counters_allocation.mapped_ptr() {
                let counters = unsafe { &*(mapped.as_ptr() as *const ParticleCounters) };
                Ok(counters.dead_count)
            } else {
                Ok(0)
            }
        } else {
            Ok(0)
        }
    }

    /// Get the maximum particle count.
    pub fn max_particles(&self) -> u32 {
        self.max_particles
    }

    /// Get the pre-computed buffer layout.
    pub fn layout(&self) -> &ParticleBufferLayout {
        &self.layout
    }

    /// Get the particle buffer handle (internal use only).
    pub(crate) fn particle_buffer(&self) -> vk::Buffer {
        self.particle_buffer
    }

    /// Get the counters buffer handle (internal use only).
    pub(crate) fn counters_buffer(&self) -> vk::Buffer {
        self.counters_buffer
    }

    /// Get the indirect draw buffer handle (for vkCmdDrawIndirect).
    pub(crate) fn indirect_draw_buffer(&self) -> vk::Buffer {
        self.indirect_draw_buffer
    }

    /// Swap alive_list_next to alive_list for next frame.
    ///
    /// This copies the content from alive_list_next (written by simulate shader)
    /// to alive_list[frame_idx] (read by emit/render shaders next frame).
    ///
    /// Uses vkCmdCopyBuffer for simplicity (Option A from design doc).
    /// A buffer barrier is inserted after the copy to ensure synchronization.
    ///
    /// # Arguments
    /// * `command_buffer` - Command buffer to record the copy into
    /// * `frame_idx` - Current frame index (for per-frame offsets to avoid race conditions)
    ///
    /// # Returns
    /// Ok(()) if swap succeeded, Err otherwise
    pub fn swap_alive_lists(
        &self,
        command_buffer: vk::CommandBuffer,
        frame_idx: usize,
    ) -> Result<(), String> {
        let device = &self.context.device;

        let alive_list_offset = self.layout.alive_current_frame_offset[frame_idx];
        let alive_next_offset = self.layout.alive_next_offset;
        let alive_list_size = self.layout.alive_list_size;

        // CRITICAL: Insert barrier BEFORE copy to ensure SIMULATE pass writes to alive_next are visible to TRANSFER read
        // This prevents READ_AFTER_WRITE hazards when copying from alive_next
        let pre_copy_barrier = vk::BufferMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(self.particle_buffer)
            .offset(alive_next_offset)
            .size(alive_list_size);

        unsafe {
            device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[pre_copy_barrier],
                &[],
            );
        }

        // Copy alive_next to alive_list (per-frame offset)
        let copy_region = vk::BufferCopy::default()
            .src_offset(alive_next_offset)
            .dst_offset(alive_list_offset)
            .size(alive_list_size);

        unsafe {
            device.cmd_copy_buffer(
                command_buffer,
                self.particle_buffer, // Same buffer, different regions
                self.particle_buffer,
                std::slice::from_ref(&copy_region),
            );
        }

        // Insert buffer barrier to ensure copy completes before next access
        // This prevents the emit shader from reading while copy is in progress
        // CRITICAL: Barrier must cover both source and destination regions
        let barriers = [
            // Barrier for source region (alive_next)
            vk::BufferMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .buffer(self.particle_buffer)
                .offset(alive_next_offset)
                .size(alive_list_size),
            // Barrier for destination region (alive_list)
            vk::BufferMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_WRITE | vk::AccessFlags::SHADER_READ)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .buffer(self.particle_buffer)
                .offset(alive_list_offset)
                .size(alive_list_size),
        ];

        unsafe {
            // Use legacy barrier for compatibility
            device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &barriers,
                &[],
            );
        }

        // CRITICAL FIX: We NO LONGER update alive_count here!
        // The simulate shader has already set it correctly with the survivor count.
        // Since we removed the reset from simulate, the count now persists through swap.
        // The swap just copies the alive_list data, the counter is already correct.

        log::debug!("Swap: alive_count preserved from simulate pass (no GPU update needed)");

        Ok(())
    }

    /// Destroy all resources.
    pub fn destroy(&mut self) {
        if self.destroyed {
            return;
        }
        self.destroyed = true;

        unsafe {
            // Free allocations first
            if let Some(alloc) = self.particle_allocation.take()
                && let Ok(mut allocator) = self.context.allocator.try_borrow_mut()
            {
                allocator.free(alloc).ok();
            }
            if let Some(alloc) = self.counters_allocation.take()
                && let Ok(mut allocator) = self.context.allocator.try_borrow_mut()
            {
                allocator.free(alloc).ok();
            }
            if let Some(alloc) = self.indirect_draw_allocation.take()
                && let Ok(mut allocator) = self.context.allocator.try_borrow_mut()
            {
                allocator.free(alloc).ok();
            }
            // Destroy buffers (only if not null)
            if self.particle_buffer != vk::Buffer::null() {
                self.context
                    .device
                    .destroy_buffer(self.particle_buffer, None);
                self.particle_buffer = vk::Buffer::null();
            }
            if self.counters_buffer != vk::Buffer::null() {
                self.context
                    .device
                    .destroy_buffer(self.counters_buffer, None);
                self.counters_buffer = vk::Buffer::null();
            }
            if self.indirect_draw_buffer != vk::Buffer::null() {
                self.context
                    .device
                    .destroy_buffer(self.indirect_draw_buffer, None);
                self.indirect_draw_buffer = vk::Buffer::null();
            }
        }
    }
}

impl Drop for GlobalParticleBuffer {
    fn drop(&mut self) {
        self.destroy();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_particle_data_size() {
        assert_eq!(std::mem::size_of::<ParticleData>(), 48);
    }

    #[test]
    fn test_counters_size() {
        assert_eq!(std::mem::size_of::<ParticleCounters>(), 16);
    }

    #[test]
    fn test_frame_data_size() {
        assert_eq!(std::mem::size_of::<FrameData>(), 32);
    }

    #[test]
    fn test_frame_data_offsets() {
        assert_eq!(std::mem::offset_of!(FrameData, delta_time), 0);
        assert_eq!(std::mem::offset_of!(FrameData, total_emit_count), 4);
        assert_eq!(std::mem::offset_of!(FrameData, emitter_count), 8);
        assert_eq!(std::mem::offset_of!(FrameData, random_seed), 12);
        assert_eq!(std::mem::offset_of!(FrameData, total_simulate_count), 16);
        assert_eq!(std::mem::offset_of!(FrameData, burst_count), 20);
        assert_eq!(std::mem::offset_of!(FrameData, frame_index), 24);
        assert_eq!(std::mem::offset_of!(FrameData, _pad), 28);
    }
}
