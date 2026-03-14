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

/// Maximum particles per buffer
const MAX_PARTICLES: u32 = 1_048_576; // 1M particles

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
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct FrameData {
    /// Delta time for this frame (seconds)
    pub delta_time: f32,
    /// Total particles to emit this frame
    pub total_emit_count: u32,
    /// Random seed for particle initialization
    pub random_seed: u32,
    pub _pad: u32,
}

/// Atomic counters for particle management (8 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ParticleCounters {
    /// Number of alive particles (atomic)
    pub alive_count: u32,
    /// Number of dead particles (atomic, starts at MAX_PARTICLES)
    pub dead_count: u32,
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
/// - Alive list current: 4 MB
/// - Alive list next: 4 MB
/// - Counters: 32 bytes
/// - Emitter configs: 80 KB (1024 × 80 bytes)
/// - Indirect draw: 16 bytes
/// Total: ~60 MB
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

    /// Descriptor set for compute pipeline
    compute_descriptor_set: Option<vk::DescriptorSet>,

    /// Descriptor set for render pipeline
    render_descriptor_set: Option<vk::DescriptorSet>,

    /// Descriptor pool
    descriptor_pool: Option<vk::DescriptorPool>,

    /// Descriptor set layouts
    compute_layout: Option<vk::DescriptorSetLayout>,
    render_layout: Option<vk::DescriptorSetLayout>,
}

impl GlobalParticleBuffer {
    /// Create a new global particle buffer.
    pub fn new(context: Rc<VulkanContext>, max_particles: u32) -> Result<Self, String> {
        let particle_size = (max_particles as usize) * std::mem::size_of::<ParticleData>();

        // Create particle storage buffer
        let particle_buffer_info = vk::BufferCreateInfo::default()
            .size((particle_size * 3) as u64) // particles + 2 alive lists
            .usage(
                vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_DST
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
            };
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
            (particle_size * 3 + emitter_size + counters_size) / (1024 * 1024)
        );

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
            compute_descriptor_set: None,
            render_descriptor_set: None,
            descriptor_pool: None,
            compute_layout: None,
            render_layout: None,
        })
    }

    /// Initialize dead list (all particles start dead).
    pub fn initialize_dead_list(&self) -> Result<(), String> {
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

        self.context.end_single_time_commands(cmd);

        // TODO: Initialize dead list with indices 0..MAX_PARTICLES
        // This requires a staging buffer and copy

        info!("Initialized particle dead list");
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
                (configs.len() * std::mem::size_of::<EmitterConfig>()) as u64,
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

    /// Dispatch compute shader for particle update.
    pub fn dispatch_compute(
        &self,
        emit_count: u32,
        delta_time: f32,
        frame_count: u32,
    ) -> Result<(), String> {
        // Create frame data for push descriptor update
        let _frame_data = FrameData {
            delta_time,
            total_emit_count: emit_count,
            random_seed: frame_count,
            _pad: 0,
        };

        // TODO: Use push descriptors to update frame_data
        // This will be done when recording the compute pass

        // TODO: Record compute dispatch
        // This requires command buffer and pipeline binding

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

    /// Destroy all resources.
    pub fn destroy(&mut self) {
        unsafe {
            if let Some(layout) = self.compute_layout.take() {
                self.context
                    .device
                    .destroy_descriptor_set_layout(layout, None);
            }
            if let Some(layout) = self.render_layout.take() {
                self.context
                    .device
                    .destroy_descriptor_set_layout(layout, None);
            }
            if let Some(pool) = self.descriptor_pool.take() {
                self.context.device.destroy_descriptor_pool(pool, None);
            }
            if let Some(alloc) = self.particle_allocation.take() {
                self.context.allocator.borrow_mut().free(alloc).ok();
            }
            if let Some(alloc) = self.counters_allocation.take() {
                self.context.allocator.borrow_mut().free(alloc).ok();
            }
            if let Some(alloc) = self.emitter_allocation.take() {
                self.context.allocator.borrow_mut().free(alloc).ok();
            }
            if let Some(alloc) = self.indirect_allocation.take() {
                self.context.allocator.borrow_mut().free(alloc).ok();
            }
            self.context
                .device
                .destroy_buffer(self.particle_buffer, None);
            self.context
                .device
                .destroy_buffer(self.counters_buffer, None);
            self.context
                .device
                .destroy_buffer(self.emitter_buffer, None);
            self.context
                .device
                .destroy_buffer(self.indirect_buffer, None);
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
        assert_eq!(std::mem::size_of::<ParticleCounters>(), 8);
    }

    #[test]
    fn test_frame_data_size() {
        assert_eq!(std::mem::size_of::<FrameData>(), 16);
    }
}
