//! Global particle buffer with index list management and atomic counters.

use std::rc::Rc;

use ash::vk;
use bytemuck::{Pod, Zeroable};

use crate::error::RendererError;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc};
use std::mem::ManuallyDrop;

use log::info;

use crate::vulkan::context::VulkanContext;

/// Particle data structure (64 bytes).
///
/// Layout must match WGSL struct exactly. WGSL pads struct size to a multiple
/// of the largest member alignment (vec3f = 16 bytes), so 12 bytes of padding
/// are added after emitter_index.
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
    /// Index of the emitter that spawned this particle
    pub emitter_index: u32,
    /// Total lifetime assigned at emit (used to compute normalized age for color/size curves)
    pub max_lifetime: f32,
    /// Scale assigned at emit (used as base for size-over-lifetime curve)
    pub initial_scale: f32,
    /// Padding to match WGSL struct alignment (vec3f align = 16, struct size must be multiple of 16)
    pub _pad: f32,
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
    /// Size of the particle data region
    pub particles_size: u64,
    /// Size of the dead list region
    pub dead_list_size: u64,
    /// Size of a single alive list region
    pub alive_list_size: u64,
    /// Total particle buffer size
    pub total_size: u64,
    /// Byte offset where dead list begins
    pub dead_list_offset: u64,
    /// Byte offset where alive[0] begins
    pub alive_offset: u64,
    /// Byte offsets for alive ping-pong regions [0] and [1]
    pub alive_frame_offset: [u64; 2],
    /// Maximum particles
    pub max_particles: u64,
}

impl ParticleBufferLayout {
    /// Compute buffer layout for a given particle count and device alignment requirement.
    pub fn new(max_particles: u32, alignment: u64) -> Self {
        let max_particles = max_particles as u64;
        let particle_data_size = max_particles * std::mem::size_of::<ParticleData>() as u64;
        let particles_size = particle_data_size.next_multiple_of(alignment);

        let dead_list_raw = max_particles * std::mem::size_of::<u32>() as u64;
        let dead_list_size = dead_list_raw.next_multiple_of(alignment);

        let alive_list_raw = max_particles * std::mem::size_of::<u32>() as u64;
        let alive_list_size = alive_list_raw.next_multiple_of(alignment);

        let dead_list_offset = particles_size;
        let alive_offset = dead_list_offset + dead_list_size;
        let alive_frame_offset = [alive_offset, alive_offset + alive_list_size];

        let total_size = alive_offset + 2 * alive_list_size;

        Self {
            particles_size,
            dead_list_size,
            alive_list_size,
            total_size,
            dead_list_offset,
            alive_offset,
            alive_frame_offset,
            max_particles,
        }
    }
}

/// Global particle buffer with all particle data and management structures.
///
/// Memory layout:
/// - Particle data: 48 MB (1M × 48 bytes)
/// - Dead list: 4 MB (1M × 4 bytes)
/// - Alive list ping-pong: 8 MB (2 × 4 MB for A/B swap)
///   Total: ~60 MB
///
/// Counters, indirect draw, and emitter configs use separate per-frame buffers
struct StagingBuffer {
    buffer: vk::Buffer,
    allocation: Allocation,
}

/// (double-buffered to avoid races between frames-in-flight).
pub struct GlobalParticleBuffer {
    context: Rc<VulkanContext>,

    /// Main particle storage buffer
    particle_buffer: Option<vk::Buffer>,
    particle_allocation: ManuallyDrop<Allocation>,

    /// Per-frame atomic counters [frames_in_flight]
    counters_buffers: [Option<vk::Buffer>; 2],
    counters_allocations: [ManuallyDrop<Allocation>; 2],

    /// Per-frame indirect draw command buffers [frames_in_flight]
    indirect_draw_buffers: [Option<vk::Buffer>; 2],
    indirect_draw_allocations: [ManuallyDrop<Allocation>; 2],

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
    pub fn new(context: Rc<VulkanContext>, max_particles: u32) -> Result<Self, RendererError> {
        // Validate max_particles parameter to prevent allocation failures and shader overflow
        if max_particles == 0 {
            return Err(RendererError::InvalidOperation(
                "max_particles must be greater than 0".into(),
            ));
        }
        if max_particles > Self::SHADER_MAX_PARTICLES {
            return Err(RendererError::ResourceCreationFailed(format!(
                "max_particles ({}) exceeds shader limit ({}), please update shaders if more particles are needed",
                max_particles,
                Self::SHADER_MAX_PARTICLES
            )));
        }

        let alignment = unsafe {
            context
                .instance
                .get_physical_device_properties(context.physical_device)
        }
        .limits
        .min_storage_buffer_offset_alignment;

        let layout = ParticleBufferLayout::new(max_particles, alignment);

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
            .try_borrow_mut_string("global_particle_buffer")?
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

        // Create per-frame counters buffers (CPU-visible for readback, double-buffered)
        let counters_size = std::mem::size_of::<ParticleCounters>();
        let mut counters_buffers = [None; 2];
        let mut counters_allocations_build: [Option<Allocation>; 2] = [None, None];

        for frame_idx in 0..2 {
            let counters_buffer_info = vk::BufferCreateInfo::default()
                .size(counters_size as u64)
                .usage(
                    vk::BufferUsageFlags::STORAGE_BUFFER
                        | vk::BufferUsageFlags::UNIFORM_BUFFER
                        | vk::BufferUsageFlags::TRANSFER_SRC
                        | vk::BufferUsageFlags::TRANSFER_DST,
                )
                .sharing_mode(vk::SharingMode::EXCLUSIVE);

            counters_buffers[frame_idx] = Some(unsafe {
                context
                    .device
                    .create_buffer(&counters_buffer_info, None)
                    .map_err(|e| {
                        format!("Failed to create counters buffer[{}]: {:?}", frame_idx, e)
                    })?
            });

            let counters_requirements = unsafe {
                context
                    .device
                    .get_buffer_memory_requirements(counters_buffers[frame_idx].unwrap())
            };

            counters_allocations_build[frame_idx] = Some(
                context
                    .allocator
                    .try_borrow_mut_string("particle_counters")?
                    .allocate(&AllocationCreateDesc {
                        name: &format!("particle_counters[{}]", frame_idx),
                        requirements: counters_requirements,
                        location: gpu_allocator::MemoryLocation::CpuToGpu,
                        linear: true,
                        allocation_scheme:
                            gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
                    })
                    .map_err(|e| {
                        format!("Failed to allocate counters memory[{}]: {}", frame_idx, e)
                    })?,
            );

            unsafe {
                context
                    .device
                    .bind_buffer_memory(
                        counters_buffers[frame_idx].unwrap(),
                        counters_allocations_build[frame_idx]
                            .as_ref()
                            .unwrap()
                            .memory(),
                        counters_allocations_build[frame_idx]
                            .as_ref()
                            .unwrap()
                            .offset(),
                    )
                    .map_err(|e| {
                        format!("Failed to bind counters memory[{}]: {:?}", frame_idx, e)
                    })?;
            }

            // Initialize counters
            if let Some(mapped) = counters_allocations_build[frame_idx]
                .as_ref()
                .unwrap()
                .mapped_ptr()
            {
                let counters = ParticleCounters {
                    alive_count: 0,
                    dead_count: max_particles,
                    emit_count: 0,
                    workgroups_finished: 0,
                };
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        &counters as *const ParticleCounters as *const u8,
                        mapped.as_ptr() as *mut u8,
                        std::mem::size_of::<ParticleCounters>(),
                    );
                }
                let _ = context.flush_mapped_memory(
                    counters_allocations_build[frame_idx].as_ref().unwrap(),
                    0,
                    std::mem::size_of::<ParticleCounters>() as u64,
                );
            }
        }

        let counters_allocations = [
            ManuallyDrop::new(counters_allocations_build[0].take().unwrap()),
            ManuallyDrop::new(counters_allocations_build[1].take().unwrap()),
        ];

        // Create per-frame indirect draw command buffers (16 bytes each, double-buffered)
        let indirect_draw_size: u64 = 16;
        let mut indirect_draw_buffers = [None; 2];
        let mut indirect_draw_allocations_build: [Option<Allocation>; 2] = [None, None];

        for frame_idx in 0..2 {
            let indirect_draw_buffer_info = vk::BufferCreateInfo::default()
                .size(indirect_draw_size)
                .usage(
                    vk::BufferUsageFlags::STORAGE_BUFFER
                        | vk::BufferUsageFlags::INDIRECT_BUFFER
                        | vk::BufferUsageFlags::TRANSFER_SRC,
                )
                .sharing_mode(vk::SharingMode::EXCLUSIVE);

            indirect_draw_buffers[frame_idx] = Some(unsafe {
                context
                    .device
                    .create_buffer(&indirect_draw_buffer_info, None)
                    .map_err(|e| {
                        format!(
                            "Failed to create indirect draw buffer[{}]: {:?}",
                            frame_idx, e
                        )
                    })?
            });

            let indirect_draw_requirements = unsafe {
                context
                    .device
                    .get_buffer_memory_requirements(indirect_draw_buffers[frame_idx].unwrap())
            };

            indirect_draw_allocations_build[frame_idx] = Some(
                context
                    .allocator
                    .try_borrow_mut_string("particle_indirect_draw")?
                    .allocate(&AllocationCreateDesc {
                        name: &format!("particle_indirect_draw[{}]", frame_idx),
                        requirements: indirect_draw_requirements,
                        location: gpu_allocator::MemoryLocation::GpuOnly,
                        linear: true,
                        allocation_scheme:
                            gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
                    })
                    .map_err(|e| {
                        format!(
                            "Failed to allocate indirect draw memory[{}]: {}",
                            frame_idx, e
                        )
                    })?,
            );

            unsafe {
                context
                    .device
                    .bind_buffer_memory(
                        indirect_draw_buffers[frame_idx].unwrap(),
                        indirect_draw_allocations_build[frame_idx]
                            .as_ref()
                            .unwrap()
                            .memory(),
                        indirect_draw_allocations_build[frame_idx]
                            .as_ref()
                            .unwrap()
                            .offset(),
                    )
                    .map_err(|e| {
                        format!(
                            "Failed to bind indirect draw memory[{}]: {:?}",
                            frame_idx, e
                        )
                    })?;
            }
        }

        let indirect_draw_allocations = [
            ManuallyDrop::new(indirect_draw_allocations_build[0].take().unwrap()),
            ManuallyDrop::new(indirect_draw_allocations_build[1].take().unwrap()),
        ];

        info!(
            "Created global particle buffer: {} particles ({} MB)",
            max_particles,
            (layout.particles_size as usize
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
            ("alive[0]", layout.alive_offset),
            ("alive[1]", layout.alive_frame_offset[1]),
        ];

        for (name, offset) in offsets.iter() {
            if offset % min_storage_buffer_offset_alignment != 0 {
                return Err(RendererError::InvalidOperation(format!(
                    "Buffer offset for {} ({}) is not aligned to min_storage_buffer_offset_alignment ({})",
                    name, offset, min_storage_buffer_offset_alignment
                )));
            }
        }

        Ok(Self {
            context,
            particle_buffer: Some(particle_buffer),
            particle_allocation: ManuallyDrop::new(particle_allocation),
            counters_buffers,
            counters_allocations,
            indirect_draw_buffers,
            indirect_draw_allocations,
            max_particles,
            layout,
            destroyed: false,
        })
    }

    fn create_staging_buffer(&self, name: &str, size: u64) -> Result<StagingBuffer, RendererError> {
        let buffer_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buffer = unsafe {
            self.context
                .device
                .create_buffer(&buffer_info, None)
                .map_err(|e| format!("Failed to create staging buffer '{}': {:?}", name, e))?
        };
        let requirements = unsafe { self.context.device.get_buffer_memory_requirements(buffer) };
        let allocation = self
            .context
            .allocator
            .try_borrow_mut_string(name)?
            .allocate(&AllocationCreateDesc {
                name,
                requirements,
                location: gpu_allocator::MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| format!("Failed to allocate staging memory '{}': {}", name, e))?;
        unsafe {
            self.context
                .device
                .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
                .map_err(|e| format!("Failed to bind staging memory '{}': {:?}", name, e))?;
        }
        Ok(StagingBuffer { buffer, allocation })
    }

    /// Initialize all index lists (dead list starts full, alive lists start empty).
    pub fn initialize_index_lists(&self) -> Result<(), RendererError> {
        // Prepare dead list staging data
        let indices: Vec<u32> = (0..self.max_particles).collect();
        let dead_list_data: Vec<u8> = indices
            .iter()
            .flat_map(|i| i.to_le_bytes().to_vec())
            .collect();
        let dead_list_size = dead_list_data.len() as u64;

        // Prepare counters staging data
        let counters_data = ParticleCounters {
            alive_count: 0,
            dead_count: self.max_particles,
            emit_count: 0,
            workgroups_finished: 0,
        };
        let counters_bytes = bytemuck::bytes_of(&counters_data);
        let counters_size = counters_bytes.len() as u64;

        // Create dead list staging buffer
        let dead_staging =
            self.create_staging_buffer("particle_dead_list_staging", dead_list_size)?;
        if let Some(mapped) = dead_staging.allocation.mapped_ptr() {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    dead_list_data.as_ptr(),
                    mapped.as_ptr() as *mut u8,
                    dead_list_data.len(),
                );
            }
            let _ = self
                .context
                .flush_mapped_memory(&dead_staging.allocation, 0, dead_list_size);
        }

        // Create counters staging buffers for both frames
        let counters_staging_0 =
            self.create_staging_buffer("particle_counters_staging[0]", counters_size)?;
        let counters_staging_1 =
            self.create_staging_buffer("particle_counters_staging[1]", counters_size)?;
        for staging in [&counters_staging_0, &counters_staging_1] {
            if let Some(mapped) = staging.allocation.mapped_ptr() {
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        counters_bytes.as_ptr(),
                        mapped.as_ptr() as *mut u8,
                        counters_bytes.len(),
                    );
                }
                let _ = self
                    .context
                    .flush_mapped_memory(&staging.allocation, 0, counters_size);
            }
        }

        // Record all initialization commands into a single command buffer
        let cmd = self
            .context
            .begin_single_time_commands()
            .map_err(|e| format!("Failed to begin single-time commands: {}", e))?;

        unsafe {
            let vk_cmd = cmd.vk_command_buffer();

            // Zero-fill the particle data region
            self.context.device.cmd_fill_buffer(
                vk_cmd,
                self.particle_buffer(),
                0,
                self.layout.particles_size,
                0,
            );

            // Copy dead list indices from staging
            let dead_copy = vk::BufferCopy::default()
                .src_offset(0)
                .dst_offset(self.layout.dead_list_offset)
                .size(dead_list_size);
            self.context.device.cmd_copy_buffer(
                vk_cmd,
                dead_staging.buffer,
                self.particle_buffer(),
                std::slice::from_ref(&dead_copy),
            );

            // Zero-fill alive lists for both frames
            for frame_idx in 0..2 {
                self.context.device.cmd_fill_buffer(
                    vk_cmd,
                    self.particle_buffer(),
                    self.layout.alive_frame_offset[frame_idx],
                    self.layout.alive_list_size,
                    0,
                );
            }

            // Copy counters from staging for both frames
            for (frame_idx, staging) in [&counters_staging_0, &counters_staging_1]
                .into_iter()
                .enumerate()
            {
                let counters_copy = vk::BufferCopy::default()
                    .src_offset(0)
                    .dst_offset(0)
                    .size(counters_size);
                self.context.device.cmd_copy_buffer(
                    vk_cmd,
                    staging.buffer,
                    self.counters_buffer(frame_idx),
                    std::slice::from_ref(&counters_copy),
                );
            }
        }

        self.context
            .end_single_time_commands(cmd)
            .map_err(|e| format!("Failed to end single-time commands: {}", e))?;

        // Cleanup all staging buffers
        for staging in [dead_staging, counters_staging_0, counters_staging_1] {
            unsafe {
                self.context.device.destroy_buffer(staging.buffer, None);
            }
            self.context
                .allocator
                .free(staging.allocation, "particle init staging");
        }

        info!(
            "Initialized particle index lists: dead={}, alive[2]={} ({} MB total)",
            self.max_particles,
            0,
            (self.layout.alive_list_size * 2) / (1024 * 1024)
        );
        Ok(())
    }

    /// Get alive particle count for the given frame.
    ///
    /// Invalidates mapped memory before reading to ensure GPU writes are visible.
    /// Must be called after the GPU command buffer that wrote to counters has completed.
    pub fn get_alive_count(&self, frame_index: usize) -> Result<u32, RendererError> {
        let fi = frame_index % 2;
        let counters_allocation = &self.counters_allocations[fi];
        let _ = self.context.invalidate_mapped_memory(
            counters_allocation,
            0,
            std::mem::size_of::<ParticleCounters>() as u64,
        );
        if let Some(mapped) = counters_allocation.mapped_ptr() {
            let counters = unsafe { &*(mapped.as_ptr() as *const ParticleCounters) };
            return Ok(counters.alive_count);
        }
        Ok(0)
    }

    /// Get dead particle count for the given frame.
    ///
    /// Invalidates mapped memory before reading to ensure GPU writes are visible.
    /// Must be called after the GPU command buffer that wrote to counters has completed.
    pub fn get_dead_count(&self, frame_index: usize) -> Result<u32, RendererError> {
        let fi = frame_index % 2;
        let counters_allocation = &self.counters_allocations[fi];
        let _ = self.context.invalidate_mapped_memory(
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
    }

    /// Get the maximum particle count.
    pub fn max_particles(&self) -> u32 {
        self.max_particles
    }

    /// Get the pre-computed buffer layout.
    pub fn layout(&self) -> &ParticleBufferLayout {
        &self.layout
    }

    pub fn particle_buffer(&self) -> vk::Buffer {
        self.particle_buffer.unwrap_or_default()
    }

    pub fn counters_buffer(&self, frame_index: usize) -> vk::Buffer {
        self.counters_buffers[frame_index % 2].unwrap_or_default()
    }

    pub fn indirect_draw_buffer(&self, frame_index: usize) -> vk::Buffer {
        self.indirect_draw_buffers[frame_index % 2].unwrap_or_default()
    }

    /// Destroy all resources.
    pub fn destroy(&mut self) {
        if self.destroyed {
            return;
        }
        self.destroyed = true;

        log::info!("  buffer destroy: freeing particle allocation");
        unsafe {
            let alloc = ManuallyDrop::take(&mut self.particle_allocation);
            self.context.allocator.free(alloc, "particle buffer");
            for frame_idx in 0..2 {
                log::info!(
                    "  buffer destroy: freeing counters allocation[{}]",
                    frame_idx
                );
                let alloc = ManuallyDrop::take(&mut self.counters_allocations[frame_idx]);
                self.context.allocator.free(alloc, "particle counters");
                log::info!(
                    "  buffer destroy: freeing indirect draw allocation[{}]",
                    frame_idx
                );
                let alloc = ManuallyDrop::take(&mut self.indirect_draw_allocations[frame_idx]);
                self.context.allocator.free(alloc, "particle indirect draw");
            }
            log::info!("  buffer destroy: destroying particle buffer");
            if let Some(buffer) = self.particle_buffer.take() {
                self.context.device.destroy_buffer(buffer, None);
            }
            for frame_idx in 0..2 {
                log::info!(
                    "  buffer destroy: destroying counters buffer[{}]",
                    frame_idx
                );
                if let Some(buffer) = self.counters_buffers[frame_idx].take() {
                    self.context.device.destroy_buffer(buffer, None);
                }
                log::info!(
                    "  buffer destroy: destroying indirect draw buffer[{}]",
                    frame_idx
                );
                if let Some(buffer) = self.indirect_draw_buffers[frame_idx].take() {
                    self.context.device.destroy_buffer(buffer, None);
                }
            }
        }
        log::info!("  buffer destroy: done");
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
        assert_eq!(std::mem::size_of::<ParticleData>(), 64);
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
