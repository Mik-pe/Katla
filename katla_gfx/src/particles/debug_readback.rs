//! Debug readback for particle system — CPU-side access to GPU particle data via staging buffers.

use std::rc::Rc;

use ash::vk;
use log::{info, warn};

use crate::sync::VkBuffer;
use crate::vulkan::context::VulkanContext;

use super::buffer::{GlobalParticleBuffer, ParticleCounters, ParticleData};

/// Indirect draw command data read back from GPU.
///
/// Mirrors the VkDrawIndirectCommand struct written by the simulate shader.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct IndirectDrawCommandData {
    pub vertex_count: u32,
    pub instance_count: u32,
    pub first_vertex: u32,
    pub first_instance: u32,
}

unsafe impl bytemuck::Pod for IndirectDrawCommandData {}
unsafe impl bytemuck::Zeroable for IndirectDrawCommandData {}

/// Debug readback data for particle system.
#[derive(Debug, Clone)]
pub struct ParticleDebugData {
    /// Particle data read back from GPU
    pub particles: Vec<ParticleData>,
    /// Alive particle index list
    pub alive_list: Vec<u32>,
    /// Dead particle index list
    pub dead_list: Vec<u32>,
    /// Atomic counters
    pub counters: ParticleCounters,
    /// Indirect draw command from the last readback frame
    pub indirect_draw: Option<IndirectDrawCommandData>,
}

impl ParticleDebugData {
    /// Create empty debug data
    pub fn new() -> Self {
        Self {
            particles: Vec::new(),
            alive_list: Vec::new(),
            dead_list: Vec::new(),
            counters: ParticleCounters {
                alive_count: 0,
                dead_count: 0,
                emit_count: 0,
                workgroups_finished: 0,
            },
            indirect_draw: None,
        }
    }

    /// Get summary statistics
    pub fn summary(&self) -> String {
        format!(
            "Particles: {} alive, {} dead, {} total capacity | Lists: {} alive indices, {} dead indices",
            self.counters.alive_count,
            self.counters.dead_count,
            self.particles.len(),
            self.alive_list.len(),
            self.dead_list.len()
        )
    }

    /// Print first N particles for debugging
    pub fn print_particles(&self, count: usize) {
        let n = count.min(self.particles.len());
        info!("=== First {} particles ===", n);
        for (i, p) in self.particles.iter().take(n).enumerate() {
            info!(
                "Particle {}: pos=({:.2},{:.2},{:.2}) vel=({:.2},{:.2},{:.2}) lifetime={:.2} scale={:.2} color=({:.2},{:.2},{:.2},{:.2})",
                i,
                p.position[0],
                p.position[1],
                p.position[2],
                p.velocity[0],
                p.velocity[1],
                p.velocity[2],
                p.lifetime,
                p.scale,
                p.color[0],
                p.color[1],
                p.color[2],
                p.color[3]
            );
        }
    }

    /// Print alive particle indices with detailed diagnostics
    pub fn print_alive_indices(&self, count: usize) {
        let n = count.min(self.alive_list.len());
        info!("=== First {} alive particle indices ===", n);

        // Check for bounds violations
        let max_particles = self.particles.len();
        let mut out_of_bounds = 0;
        let mut duplicates = std::collections::HashSet::new();
        let mut duplicate_count = 0;

        for (i, &idx) in self.alive_list.iter().take(n).enumerate() {
            let is_ob = idx as usize >= max_particles;
            if is_ob {
                out_of_bounds += 1;
            }

            if duplicates.contains(&idx) {
                duplicate_count += 1;
            } else {
                duplicates.insert(idx);
            }

            let marker = if is_ob { "[OUT OF BOUNDS]" } else { "" };
            info!("  alive_list[{}] = {} {}", i, idx, marker);
        }

        if out_of_bounds > 0 {
            warn!(
                "WARNING: {} alive particle indices are out of bounds (max={})",
                out_of_bounds, max_particles
            );
        }

        if duplicate_count > 0 {
            warn!(
                "WARNING: {} duplicate indices found in first {} alive list entries",
                duplicate_count, n
            );
        }
    }

    /// Print particles at specific indices from alive_list
    pub fn print_alive_particles(&self, count: usize) {
        let n = count.min(self.alive_list.len());
        info!("=== First {} alive particles (by index) ===", n);

        let mut unique_positions = std::collections::HashSet::new();
        let mut position_counts: std::collections::HashMap<(i32, i32, i32), usize> =
            std::collections::HashMap::new();

        for (i, &idx) in self.alive_list.iter().take(n).enumerate() {
            let idx = idx as usize;
            if idx >= self.particles.len() {
                warn!(
                    "  [{}] Index {} out of bounds (max={})",
                    i,
                    idx,
                    self.particles.len()
                );
                continue;
            }

            let p = &self.particles[idx];

            // Quantize position for grouping (round to 2 decimal places)
            let pos_key = (
                (p.position[0] * 100.0) as i32,
                (p.position[1] * 100.0) as i32,
                (p.position[2] * 100.0) as i32,
            );
            *position_counts.entry(pos_key).or_insert(0) += 1;

            info!(
                "  [{}] Particle idx={}: pos=({:.2},{:.2},{:.2}) vel=({:.2},{:.2},{:.2}) lifetime={:.2} scale={:.2} color=({:.2},{:.2},{:.2},{:.2})",
                i,
                idx,
                p.position[0],
                p.position[1],
                p.position[2],
                p.velocity[0],
                p.velocity[1],
                p.velocity[2],
                p.lifetime,
                p.scale,
                p.color[0],
                p.color[1],
                p.color[2],
                p.color[3]
            );

            unique_positions.insert(pos_key);
        }

        info!("Position distribution among first {} alive particles:", n);
        for (pos, count) in position_counts.iter() {
            info!(
                "  ({:.2}, {:.2}, {:.2}): {} particles",
                pos.0 as f32 / 100.0,
                pos.1 as f32 / 100.0,
                pos.2 as f32 / 100.0,
                count
            );
        }

        info!(
            "Unique positions: {} / {} alive particles",
            unique_positions.len(),
            n
        );
        if unique_positions.len() <= 3 {
            warn!(
                "WARNING: Only {} unique positions among {} alive particles - possible index corruption!",
                unique_positions.len(),
                n
            );
            warn!(
                "This check only validates alive particles from alive_list, not dead particle slots."
            );
        }
    }

    /// Print first few dead list indices to verify initialization
    pub fn print_dead_indices(&self, count: usize) {
        let n = count.min(self.dead_list.len());
        info!("=== First {} dead particle indices ===", n);
        info!("{:?}", &self.dead_list[..n]);
    }
}

impl Default for ParticleDebugData {
    fn default() -> Self {
        Self::new()
    }
}

/// Staging buffer for GPU-to-CPU readback.
struct ReadbackStagingBuffer {
    buffer: VkBuffer,
    allocation: gpu_allocator::vulkan::Allocation,
}

impl ReadbackStagingBuffer {
    /// Create a new staging buffer for readback.
    fn new(context: &Rc<VulkanContext>, size: u64, name: &str) -> Result<Self, String> {
        let buffer_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe {
            context
                .device
                .create_buffer(&buffer_info, None)
                .map_err(|e| format!("Failed to create readback buffer: {:?}", e))?
        };

        let requirements = unsafe { context.device.get_buffer_memory_requirements(buffer) };

        let allocation = context
            .allocator
            .try_borrow_mut_string(name)?
            .allocate(&gpu_allocator::vulkan::AllocationCreateDesc {
                name,
                requirements,
                location: gpu_allocator::MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| format!("Failed to allocate readback memory: {}", e))?;

        unsafe {
            context
                .device
                .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
                .map_err(|e| format!("Failed to bind readback memory: {:?}", e))?
        }

        Ok(Self {
            buffer: VkBuffer::new(buffer),
            allocation,
        })
    }

    /// Read data from staging buffer to CPU vector.
    fn read<T: bytemuck::Pod>(&self, count: usize) -> Vec<T> {
        if let Some(mapped) = self.allocation.mapped_ptr() {
            let src = unsafe { std::slice::from_raw_parts(mapped.as_ptr() as *const T, count) };
            src.to_vec()
        } else {
            warn!("Readback buffer is not mapped");
            Vec::new()
        }
    }

    /// Destroy the staging buffer.
    fn destroy(self, context: &Rc<VulkanContext>) {
        unsafe {
            context
                .allocator
                .free(self.allocation, "debug readback staging");
            context.device.destroy_buffer(self.buffer.vk(), None);
        }
    }
}

/// Debug readback helper for particle system.
pub struct ParticleDebugReadback {
    particle_staging: Option<ReadbackStagingBuffer>,
    alive_list_staging: Option<ReadbackStagingBuffer>,
    dead_list_staging: Option<ReadbackStagingBuffer>,
    counters_staging: Option<ReadbackStagingBuffer>,
    indirect_draw_staging: Option<ReadbackStagingBuffer>,
    context: Rc<VulkanContext>,
}

impl ParticleDebugReadback {
    /// Create a new debug readback helper.
    pub fn new(context: &Rc<VulkanContext>, max_particles: u32) -> Result<Self, String> {
        info!("Creating particle debug readback helper");

        // Particle data staging buffer (48 bytes per particle)
        let particle_data_size =
            (max_particles as u64) * std::mem::size_of::<ParticleData>() as u64;
        let particle_staging =
            ReadbackStagingBuffer::new(context, particle_data_size, "particle_readback_particles")?;

        // Alive list staging buffer (4 bytes per index)
        // Only need to read the simulate output region, not all three buffers
        let alive_list_size = (max_particles as u64) * std::mem::size_of::<u32>() as u64;
        let alive_list_staging =
            ReadbackStagingBuffer::new(context, alive_list_size, "particle_readback_alive_list")?;

        // Dead list staging buffer (4 bytes per index)
        let dead_list_size = (max_particles as u64) * std::mem::size_of::<u32>() as u64;
        let dead_list_staging =
            ReadbackStagingBuffer::new(context, dead_list_size, "particle_readback_dead_list")?;

        // Counters staging buffer
        let counters_size = std::mem::size_of::<ParticleCounters>() as u64;
        let counters_staging =
            ReadbackStagingBuffer::new(context, counters_size, "particle_readback_counters")?;

        // Indirect draw command staging buffer (16 bytes = sizeof(VkDrawIndirectCommand))
        let indirect_draw_staging = ReadbackStagingBuffer::new(
            context,
            std::mem::size_of::<IndirectDrawCommandData>() as u64,
            "particle_readback_indirect_draw",
        )?;

        Ok(Self {
            particle_staging: Some(particle_staging),
            alive_list_staging: Some(alive_list_staging),
            dead_list_staging: Some(dead_list_staging),
            counters_staging: Some(counters_staging),
            indirect_draw_staging: Some(indirect_draw_staging),
            context: context.clone(),
        })
    }

    /// Record copy commands to staging buffers.
    ///
    /// This must be called before reading data to ensure GPU->CPU copy happens.
    /// The command buffer must be submitted and waited on before calling read().
    ///
    /// IMPORTANT: This function assumes it's being called after compute shader
    /// work that writes to these buffers. A barrier is inserted to ensure
    /// compute shader writes complete before the transfer reads begin.
    pub fn record_copy(
        &mut self,
        command_buffer: vk::CommandBuffer,
        particle_buffer: &GlobalParticleBuffer,
        frame_index: usize,
    ) -> Result<(), String> {
        let device = &self.context.device;
        let layout = particle_buffer.layout();
        let fi = frame_index % 2;

        // Insert barrier to ensure compute shader writes complete before transfer reads
        // This prevents READ_AFTER_WRITE hazards

        let barriers = [
            // Barrier for particle buffer (particles + dead + alive regions)
            vk::BufferMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_WRITE | vk::AccessFlags::SHADER_READ)
                .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .buffer(particle_buffer.particle_buffer())
                .offset(0)
                .size(layout.total_size),
            // Barrier for counters buffer
            vk::BufferMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_WRITE | vk::AccessFlags::SHADER_READ)
                .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .buffer(particle_buffer.counters_buffer(fi))
                .offset(0)
                .size(std::mem::size_of::<ParticleCounters>() as u64),
            // Barrier for indirect draw buffer
            vk::BufferMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .buffer(particle_buffer.indirect_draw_buffer(fi))
                .offset(0)
                .size(16),
        ];

        unsafe {
            device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &barriers,
                &[],
            );
        }

        // Copy particle data
        if let Some(staging) = &self.particle_staging {
            let particle_size = layout.max_particles * std::mem::size_of::<ParticleData>() as u64;

            let copy_region = vk::BufferCopy {
                src_offset: 0,
                dst_offset: 0,
                size: particle_size,
            };

            unsafe {
                device.cmd_copy_buffer(
                    command_buffer,
                    particle_buffer.particle_buffer(),
                    staging.buffer.vk(),
                    &[copy_region],
                );
            }

            // Barrier: ensure particle data copy completes before next transfer read from particle_buffer
            // This prevents WRITE_AFTER_WRITE hazards when copying different regions of the same buffer
            let barrier = vk::BufferMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .buffer(staging.buffer.vk())
                .offset(0)
                .size(particle_size);

            unsafe {
                device.cmd_pipeline_barrier(
                    command_buffer,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[barrier],
                    &[],
                );
            }
        }

        // Copy alive list (simulate output region)
        // Simulate writes survivors to alive[(frame_index+1)%2]
        if let Some(staging) = &self.alive_list_staging {
            let next_frame = (frame_index + 1) % 2;
            let alive_output_offset = layout.alive_frame_offset[next_frame];
            let alive_list_size = layout.alive_list_size;

            let copy_region = vk::BufferCopy {
                src_offset: alive_output_offset,
                dst_offset: 0,
                size: alive_list_size,
            };

            log::debug!(
                "record_copy: copying alive_list from offset={}, size={}",
                alive_output_offset,
                alive_list_size
            );

            unsafe {
                device.cmd_copy_buffer(
                    command_buffer,
                    particle_buffer.particle_buffer(),
                    staging.buffer.vk(),
                    &[copy_region],
                );
            }

            // Barrier: ensure alive list copy completes before next transfer read from particle_buffer
            let barrier = vk::BufferMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .buffer(staging.buffer.vk())
                .offset(0)
                .size(alive_list_size);

            unsafe {
                device.cmd_pipeline_barrier(
                    command_buffer,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[barrier],
                    &[],
                );
            }
        }

        // Copy dead list
        if let Some(staging) = &self.dead_list_staging {
            let dead_list_offset = layout.dead_list_offset;
            let dead_list_size = layout.alive_list_size;

            let copy_region = vk::BufferCopy {
                src_offset: dead_list_offset,
                dst_offset: 0,
                size: dead_list_size,
            };

            unsafe {
                device.cmd_copy_buffer(
                    command_buffer,
                    particle_buffer.particle_buffer(),
                    staging.buffer.vk(),
                    &[copy_region],
                );
            }

            // Barrier: ensure dead list copy completes before next transfer read from particle_buffer
            let barrier = vk::BufferMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .buffer(staging.buffer.vk())
                .offset(0)
                .size(dead_list_size);

            unsafe {
                device.cmd_pipeline_barrier(
                    command_buffer,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[barrier],
                    &[],
                );
            }
        }

        // Copy counters
        if let Some(staging) = &self.counters_staging {
            let counters_buffer = particle_buffer.counters_buffer(fi);
            let counters_size = std::mem::size_of::<ParticleCounters>() as u64;

            let copy_region = vk::BufferCopy {
                src_offset: 0,
                dst_offset: 0,
                size: counters_size,
            };

            unsafe {
                device.cmd_copy_buffer(
                    command_buffer,
                    counters_buffer,
                    staging.buffer.vk(),
                    &[copy_region],
                );
            }
        }

        // Copy indirect draw command
        if let Some(staging) = &self.indirect_draw_staging {
            let indirect_draw_buffer = particle_buffer.indirect_draw_buffer(fi);
            let indirect_draw_size = std::mem::size_of::<IndirectDrawCommandData>() as u64;

            let copy_region = vk::BufferCopy {
                src_offset: 0,
                dst_offset: 0,
                size: indirect_draw_size,
            };

            unsafe {
                device.cmd_copy_buffer(
                    command_buffer,
                    indirect_draw_buffer,
                    staging.buffer.vk(),
                    &[copy_region],
                );
            }
        }

        log::debug!("Recorded particle debug readback copies");
        Ok(())
    }

    /// Read data from staging buffers to CPU.
    ///
    /// This must be called AFTER the command buffer with record_copy() has been
    /// submitted and fully executed (GPU fence wait).
    pub fn read(
        &self,
        particle_buffer: &GlobalParticleBuffer,
    ) -> Result<ParticleDebugData, String> {
        let layout = particle_buffer.layout();
        let max_particles = particle_buffer.max_particles() as usize;

        // Invalidate mapped memory for all staging buffers to ensure GPU writes (cmd_copy_buffer)
        // are visible to CPU reads. Without this, the CPU may read stale cached data.
        let particle_data_size = layout.max_particles * std::mem::size_of::<ParticleData>() as u64;
        let index_list_size = layout.alive_list_size;
        let counters_size = std::mem::size_of::<ParticleCounters>() as u64;

        if let Some(ref staging) = self.particle_staging {
            let _ =
                self.context
                    .invalidate_mapped_memory(&staging.allocation, 0, particle_data_size);
        }
        if let Some(ref staging) = self.alive_list_staging {
            let _ = self
                .context
                .invalidate_mapped_memory(&staging.allocation, 0, index_list_size);
        }
        if let Some(ref staging) = self.dead_list_staging {
            let _ = self
                .context
                .invalidate_mapped_memory(&staging.allocation, 0, index_list_size);
        }
        if let Some(ref staging) = self.counters_staging {
            let _ = self
                .context
                .invalidate_mapped_memory(&staging.allocation, 0, counters_size);
        }

        // Read particle data
        let particles = if let Some(ref staging) = self.particle_staging {
            staging.read::<ParticleData>(max_particles)
        } else {
            Vec::new()
        };

        // Read alive list (simulate output region)
        let alive_list = if let Some(ref staging) = self.alive_list_staging {
            staging.read::<u32>(max_particles)
        } else {
            Vec::new()
        };

        // Read dead list
        let dead_list = if let Some(ref staging) = self.dead_list_staging {
            staging.read::<u32>(max_particles)
        } else {
            Vec::new()
        };

        // Read counters
        let counters = if let Some(ref staging) = self.counters_staging {
            let data = staging.read::<ParticleCounters>(1);
            data.into_iter().next().unwrap_or(ParticleCounters {
                alive_count: 0,
                dead_count: 0,
                emit_count: 0,
                workgroups_finished: 0,
            })
        } else {
            ParticleCounters {
                alive_count: 0,
                dead_count: 0,
                emit_count: 0,
                workgroups_finished: 0,
            }
        };

        // Read indirect draw command
        let indirect_draw = if let Some(ref staging) = self.indirect_draw_staging {
            let _ = self.context.invalidate_mapped_memory(
                &staging.allocation,
                0,
                std::mem::size_of::<IndirectDrawCommandData>() as u64,
            );
            let data = staging.read::<IndirectDrawCommandData>(1);
            data.into_iter().next()
        } else {
            None
        };

        Ok(ParticleDebugData {
            particles,
            alive_list,
            dead_list,
            counters,
            indirect_draw,
        })
    }

    /// Destroy all staging buffers.
    pub fn destroy(&mut self) {
        if let Some(staging) = self.particle_staging.take() {
            staging.destroy(&self.context);
        }
        if let Some(staging) = self.alive_list_staging.take() {
            staging.destroy(&self.context);
        }
        if let Some(staging) = self.dead_list_staging.take() {
            staging.destroy(&self.context);
        }
        if let Some(staging) = self.counters_staging.take() {
            staging.destroy(&self.context);
        }
        if let Some(staging) = self.indirect_draw_staging.take() {
            staging.destroy(&self.context);
        }
    }
}

impl Drop for ParticleDebugReadback {
    fn drop(&mut self) {
        self.destroy();
    }
}
