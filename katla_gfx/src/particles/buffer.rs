//! Global particle buffer with index list management.
//!
//! Implements single-buffer particle storage with:
//! - Particle data storage
//! - Alive/dead index lists
//! - Atomic counters for GPU-driven lifecycle
//! - Indirect drawing support

use std::rc::Rc;

use ash::vk;
use bytemuck::{Pod, Zeroable};
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc};
use log::info;

use crate::vulkan::context::VulkanContext;

use super::EmitterConfig;

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
/// std140 layout rules apply (WGSL uniform buffers).
/// Struct is padded to 64-byte alignment to satisfy min_storage_buffer_offset_alignment.
/// 7 fields × 4 bytes = 28 bytes → padded to 64 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct FrameData {
    /// Delta time for this frame (seconds)
    pub delta_time: f32,
    /// Total particles to emit this frame
    pub total_emit_count: u32,
    /// Number of active emitters this frame
    pub emitter_count: u32,
    /// Random seed for particle initialization
    pub random_seed: u32,
    /// Total particles to simulate (newly emitted + previously alive)
    pub total_simulate_count: u32,
    /// Burst particles to emit immediately (overrides emit_rate for this frame)
    pub burst_count: u32,
    /// Frame index (for per-frame buffer offsets to avoid race conditions)
    pub frame_index: u32,
    /// Padding to match 64-byte alignment (28 → 64 bytes)
    pub _pad: [u32; 9],
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
    /// Padding
    pub _pad: u32,
}

/// Indirect draw arguments for particle rendering.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct DrawIndirectCommand {
    /// Number of vertices to draw (6 per particle)
    pub vertex_count: u32,
    /// Number of instances (always 1)
    pub instance_count: u32,
    /// First vertex (always 0)
    pub first_vertex: u32,
    /// First instance (always 0)
    pub first_instance: u32,
}

/// Global particle buffer with all particle data and management structures.
///
/// Memory layout:
/// - Particle data: 48 MB (1M × 48 bytes)
/// - Dead list: 4 MB (1M × 4 bytes)
/// - Alive list current (per-frame): 8 MB (2 × 4 MB for 2 frames in flight)
/// - Alive list next: 4 MB
/// - Counters: 32 bytes
/// - Emitter configs: 80 KB (1024 × 80 bytes)
/// - Indirect draw: 16 bytes
///   Total: ~64 MB
pub struct GlobalParticleBuffer {
    context: Rc<VulkanContext>,

    /// Main particle storage buffer
    particle_buffer: vk::Buffer,
    particle_allocation: Option<Allocation>,

    /// Atomic counters
    counters_buffer: vk::Buffer,
    counters_allocation: Option<Allocation>,

    /// Emitter configuration buffer
    emitter_buffer: vk::Buffer,
    emitter_allocation: Option<Allocation>,

    /// Indirect draw arguments buffer
    indirect_buffer: vk::Buffer,
    indirect_allocation: Option<Allocation>,

    /// Maximum particles
    max_particles: u32,

    /// Flag to prevent double destruction
    destroyed: bool,
}

impl GlobalParticleBuffer {
    /// Create a new global particle buffer.
    pub fn new(context: Rc<VulkanContext>, max_particles: u32) -> Result<Self, String> {
        let particle_size = (max_particles as usize) * std::mem::size_of::<ParticleData>();

        // Create particle storage buffer
        let particle_buffer_info = vk::BufferCreateInfo::default()
            .size(
                (particle_size * 3 + max_particles as usize * std::mem::size_of::<u32>() * 2)
                    as u64,
            ) // particles + dead + alive_current[2] + alive_next
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

        // Create counters buffer (CPU-visible for initialization)
        let counters_size = std::mem::size_of::<ParticleCounters>();
        let counters_buffer_info = vk::BufferCreateInfo::default()
            .size(counters_size as u64)
            .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::UNIFORM_BUFFER)
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
                _pad: 0,
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

        // Create emitter config buffer (CPU-visible)
        let emitter_size = (1024usize) * std::mem::size_of::<EmitterConfig>();
        let emitter_buffer_info = vk::BufferCreateInfo::default()
            .size(emitter_size as u64)
            .usage(vk::BufferUsageFlags::STORAGE_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let emitter_buffer = unsafe {
            context
                .device
                .create_buffer(&emitter_buffer_info, None)
                .map_err(|e| format!("Failed to create emitter buffer: {:?}", e))?
        };

        let emitter_requirements = unsafe {
            context
                .device
                .get_buffer_memory_requirements(emitter_buffer)
        };

        let emitter_allocation = context
            .allocator
            .borrow_mut()
            .allocate(&AllocationCreateDesc {
                name: "emitter_configs",
                requirements: emitter_requirements,
                location: gpu_allocator::MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| format!("Failed to allocate emitter memory: {}", e))?;

        unsafe {
            context
                .device
                .bind_buffer_memory(
                    emitter_buffer,
                    emitter_allocation.memory(),
                    emitter_allocation.offset(),
                )
                .map_err(|e| format!("Failed to bind emitter memory: {:?}", e))?
        }

        // Create indirect draw buffer
        let indirect_buffer_info = vk::BufferCreateInfo::default()
            .size(std::mem::size_of::<DrawIndirectCommand>() as u64)
            .usage(
                vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::INDIRECT_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_DST,
            )
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let indirect_buffer = unsafe {
            context
                .device
                .create_buffer(&indirect_buffer_info, None)
                .map_err(|e| format!("Failed to create indirect buffer: {:?}", e))?
        };

        let indirect_requirements = unsafe {
            context
                .device
                .get_buffer_memory_requirements(indirect_buffer)
        };

        let indirect_allocation = context
            .allocator
            .borrow_mut()
            .allocate(&AllocationCreateDesc {
                name: "particle_indirect",
                requirements: indirect_requirements,
                location: gpu_allocator::MemoryLocation::GpuOnly,
                linear: true,
                allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| format!("Failed to allocate indirect memory: {}", e))?;

        unsafe {
            context
                .device
                .bind_buffer_memory(
                    indirect_buffer,
                    indirect_allocation.memory(),
                    indirect_allocation.offset(),
                )
                .map_err(|e| format!("Failed to bind indirect memory: {:?}", e))?
        }

        info!(
            "Created global particle buffer: {} particles ({} MB)",
            max_particles,
            (particle_size * 3
                + max_particles as usize * std::mem::size_of::<u32>() * 2
                + emitter_size
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

        // Validate descriptor buffer offsets are properly aligned
        let particle_data_size =
            (max_particles as u64) * (std::mem::size_of::<ParticleData>() as u64);
        let dead_list_size = (max_particles as u64) * (std::mem::size_of::<u32>() as u64);

        // Check that all descriptor offsets meet alignment requirements
        let offsets = [
            ("particle data", 0u64),
            ("dead list", particle_data_size),
            ("alive_current", particle_data_size + dead_list_size),
            ("alive_next", particle_data_size + 2 * dead_list_size),
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
            emitter_buffer,
            emitter_allocation: Some(emitter_allocation),
            indirect_buffer,
            indirect_allocation: Some(indirect_allocation),
            max_particles,
            destroyed: false,
        })
    }

    /// Initialize all index lists (dead list starts full, alive lists start empty).
    pub fn initialize_index_lists(&self) -> Result<(), String> {
        // Fill particle data with zeros (all particles start dead)
        let cmd = self.context.begin_single_time_commands();

        unsafe {
            self.context.device.cmd_fill_buffer(
                cmd.vk_command_buffer(),
                self.particle_buffer,
                0,
                (self.max_particles as usize * std::mem::size_of::<ParticleData>()) as u64,
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
        // Fill particle data with zeros (all dead)
        let cmd = self.context.begin_single_time_commands();

        unsafe {
            self.context.device.cmd_fill_buffer(
                cmd.vk_command_buffer(),
                self.particle_buffer,
                0,
                (self.max_particles as usize * std::mem::size_of::<ParticleData>()) as u64,
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

        let _dead_list_size = dead_list_data.len() as u64;

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

        // Calculate dead list offset in particle buffer
        // Layout: particles (48 bytes each) -> dead list (4 bytes each)
        let dead_list_offset =
            (self.max_particles as u64) * (std::mem::size_of::<ParticleData>() as u64);

        // Copy from staging to dead list
        unsafe {
            let copy_region = vk::BufferCopy::default()
                .src_offset(0)
                .dst_offset(dead_list_offset)
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
        // Layout: particles -> dead list -> alive_current[2] -> alive_next
        // With 2 frames in flight, we have 2 separate alive_current regions
        let frames_in_flight = 2;
        let particle_data_size =
            (self.max_particles as u64) * (std::mem::size_of::<ParticleData>() as u64);
        let dead_list_size = (self.max_particles as u64) * (std::mem::size_of::<u32>() as u64);
        let base_alive_current_offset = particle_data_size + dead_list_size;
        let alive_list_size = dead_list_size;

        let cmd = self.context.begin_single_time_commands();

        unsafe {
            // Fill both alive_current regions (one per frame) with zeros
            for frame_idx in 0..frames_in_flight {
                let alive_current_offset =
                    base_alive_current_offset + (frame_idx as u64 * alive_list_size);
                self.context.device.cmd_fill_buffer(
                    cmd.vk_command_buffer(),
                    self.particle_buffer,
                    alive_current_offset,
                    alive_list_size,
                    0,
                );
            }

            // Fill alive_next with zeros
            let alive_next_offset =
                base_alive_current_offset + (frames_in_flight as u64 * alive_list_size);
            self.context.device.cmd_fill_buffer(
                cmd.vk_command_buffer(),
                self.particle_buffer,
                alive_next_offset,
                alive_list_size,
                0,
            );
        }

        self.context.end_single_time_commands(cmd);

        info!(
            "Initialized particle index lists: dead={}, alive_current[2]={}, alive_next={} ({} MB total)",
            self.max_particles,
            0,
            0,
            (dead_list_size + 3 * alive_list_size) / (1024 * 1024)
        );
        Ok(())
    }

    /// Upload emitter configurations to GPU.
    pub fn upload_emitter_configs(&self, configs: &[EmitterConfig]) -> Result<(), String> {
        if let Some(mapped) = self
            .emitter_allocation
            .as_ref()
            .and_then(|a| a.mapped_ptr())
        {
            let dst = mapped.as_ptr() as *mut EmitterConfig;
            unsafe {
                std::ptr::copy_nonoverlapping(configs.as_ptr(), dst, configs.len());
            }
            self.context.flush_mapped_memory(
                self.emitter_allocation.as_ref().unwrap(),
                0,
                std::mem::size_of_val(configs) as u64,
            );
        }
        Ok(())
    }

    /// Get current alive particle count.
    pub fn get_alive_count(&self) -> Result<u32, String> {
        if let Some(mapped) = self
            .counters_allocation
            .as_ref()
            .and_then(|a| a.mapped_ptr())
        {
            let counters = unsafe { &*(mapped.as_ptr() as *const ParticleCounters) };
            Ok(counters.alive_count)
        } else {
            Ok(0)
        }
    }

    /// Get current dead particle count.
    pub fn get_dead_count(&self) -> Result<u32, String> {
        if let Some(mapped) = self
            .counters_allocation
            .as_ref()
            .and_then(|a| a.mapped_ptr())
        {
            let counters = unsafe { &*(mapped.as_ptr() as *const ParticleCounters) };
            Ok(counters.dead_count)
        } else {
            Ok(0)
        }
    }

    /// Dispatch compute shader for particle update.
    pub fn dispatch_compute(
        &self,
        _command_buffer: vk::CommandBuffer,
        _pipeline: vk::Pipeline,
        _layout: vk::PipelineLayout,
        _emit_count: u32,
        _delta_time: f32,
        _frame_count: u32,
    ) -> Result<(), String> {
        // TODO: Implement actual compute dispatch
        // This requires:
        // 1. Binding compute pipeline
        // 2. Binding descriptor sets (particle buffers, emitter configs)
        // 3. Updating push descriptors
        // 4. Dispatching compute shader
        Ok(())
    }

    /// Dispatch indirect draw call for particle rendering.
    pub fn dispatch_draw_indirect(
        &self,
        _command_buffer: vk::CommandBuffer,
        _render_pass: vk::RenderPass,
    ) -> Result<(), String> {
        // TODO: Record indirect draw call
        // This requires pipeline binding and draw command

        Ok(())
    }

    /// Get the maximum particle count.
    pub fn max_particles(&self) -> u32 {
        self.max_particles
    }

    /// Get the particle buffer handle (internal use only).
    pub(crate) fn particle_buffer(&self) -> vk::Buffer {
        self.particle_buffer
    }

    /// Get the counters buffer handle (internal use only).
    pub(crate) fn counters_buffer(&self) -> vk::Buffer {
        self.counters_buffer
    }

    /// Swap alive_list_2 to alive_list_1 after simulate pass.
    ///
    /// This copies the content from alive_next (written by simulate shader)
    /// to alive_current (read by emit shader next frame).
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

        // Calculate buffer offsets
        // Layout: particles (48 bytes each) -> dead list (4 bytes each) -> alive_current -> alive_next
        let particle_data_size =
            (self.max_particles as u64) * (std::mem::size_of::<ParticleData>() as u64);
        let dead_list_size = (self.max_particles as u64) * (std::mem::size_of::<u32>() as u64);

        // CRITICAL: Use per-frame offsets for alive_current to avoid WRITE_AFTER_WRITE hazards
        // With 2 frames in flight, we need separate alive_current regions for each frame
        // Frame 0 uses alive_current at offset 0
        // Frame 1 uses alive_current at offset +alive_list_size
        let frames_in_flight = 2;
        let alive_list_size = dead_list_size;
        let base_alive_current_offset = particle_data_size + dead_list_size;
        let alive_current_offset = base_alive_current_offset + (frame_idx as u64 * alive_list_size);
        let alive_next_offset =
            base_alive_current_offset + (frames_in_flight as u64 * alive_list_size);

        // Copy alive_next to alive_current (per-frame offset)
        let copy_region = vk::BufferCopy::default()
            .src_offset(alive_next_offset)
            .dst_offset(alive_current_offset)
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
                .src_access_mask(vk::AccessFlags::TRANSFER_READ)
                .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .buffer(self.particle_buffer)
                .offset(alive_next_offset)
                .size(alive_list_size),
            // Barrier for destination region (alive_current)
            vk::BufferMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .buffer(self.particle_buffer)
                .offset(alive_current_offset)
                .size(alive_list_size),
        ];

        unsafe {
            // Use legacy barrier for compatibility
            device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &barriers,
                &[],
            );
        }

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
            if let Some(alloc) = self.emitter_allocation.take()
                && let Ok(mut allocator) = self.context.allocator.try_borrow_mut()
            {
                allocator.free(alloc).ok();
            }
            if let Some(alloc) = self.indirect_allocation.take()
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
            if self.emitter_buffer != vk::Buffer::null() {
                self.context
                    .device
                    .destroy_buffer(self.emitter_buffer, None);
                self.emitter_buffer = vk::Buffer::null();
            }
            if self.indirect_buffer != vk::Buffer::null() {
                self.context
                    .device
                    .destroy_buffer(self.indirect_buffer, None);
                self.indirect_buffer = vk::Buffer::null();
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
        assert_eq!(std::mem::size_of::<FrameData>(), 64);
    }
}
