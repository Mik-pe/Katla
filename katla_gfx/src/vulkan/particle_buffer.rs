use std::rc::Rc;

use ash::vk;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc};
use log::info;

use super::context::VulkanContext;

/// Per-particle data structure for GPU simulation.
///
/// Size: 64 bytes (cache-line aligned for optimal GPU access).
/// Total memory for 64K particles: 4MB.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ParticleData {
    /// World position (x, y, z)
    pub position: [f32; 3],
    /// Padding for 16-byte alignment
    pub _pad1: f32,
    /// Velocity (x, y, z)
    pub velocity: [f32; 3],
    /// Remaining lifetime in seconds
    pub lifetime: f32,
    /// RGBA color (0-1 range)
    pub color: [f32; 4],
    /// Scale factor
    pub scale: f32,
    /// Padding for 16-byte alignment
    pub _pad2: [f32; 3],
}

/// GPU particle buffer for simulation and rendering.
///
/// Manages a storage buffer containing particle data that can be:
/// - Written by compute shaders (simulation)
/// - Read by vertex shaders (rendering)
pub struct ParticleBuffer {
    /// Vulkan context for resource creation.
    context: Rc<VulkanContext>,
    /// Storage buffer for particle data (read_write for compute).
    buffer: vk::Buffer,
    /// Memory allocation for the buffer.
    allocation: Option<Allocation>,
    /// Number of particles in the buffer.
    particle_count: u32,
}

impl ParticleBuffer {
    /// Create a new particle buffer.
    ///
    /// # Arguments
    /// * `context` - Vulkan context
    /// * `max_particles` - Maximum number of particles (default: MAX_PARTICLES)
    ///
    /// # Returns
    /// A new particle buffer with allocated GPU storage.
    pub fn new(context: Rc<VulkanContext>, max_particles: u32) -> Self {
        let buffer_size = (max_particles as usize) * std::mem::size_of::<ParticleData>();

        // Create storage buffer (read_write access for compute shaders)
        let buffer_info = vk::BufferCreateInfo::default()
            .size(buffer_size as u64)
            .usage(
                vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::VERTEX_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_DST,
            )
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe {
            context
                .device
                .create_buffer(&buffer_info, None)
                .expect("Failed to create particle buffer")
        };

        // Get memory requirements
        let requirements = unsafe { context.device.get_buffer_memory_requirements(buffer) };

        // Find appropriate memory type
        let memory_type_index = {
            let memory_properties = unsafe {
                context
                    .instance
                    .get_physical_device_memory_properties(context.physical_device)
            };

            // Find a device-local memory type
            let mut found_index = None;
            for i in 0..32 {
                if (requirements.memory_type_bits & (1 << i)) != 0 {
                    let props = memory_properties.memory_types[i as usize];
                    if props
                        .property_flags
                        .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
                    {
                        found_index = Some(i);
                        break;
                    }
                }
            }
            found_index.expect("Failed to find suitable memory type for particle buffer")
        };

        // Allocate memory
        let allocation = context
            .allocator
            .borrow_mut()
            .allocate(&AllocationCreateDesc {
                name: "particle_buffer",
                requirements,
                location: gpu_allocator::MemoryLocation::GpuOnly,
                linear: true, // Buffer allocations are linear
                allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .expect("Failed to allocate memory for particle buffer");

        // Bind memory to buffer
        let allocation_memory = unsafe { allocation.memory() };
        let allocation_offset = allocation.offset();
        unsafe {
            context
                .device
                .bind_buffer_memory(buffer, allocation_memory, allocation_offset)
                .expect("Failed to bind particle buffer memory")
        };

        let _ = memory_type_index; // Used to avoid unused warning

        info!(
            "Created particle buffer: {} particles ({} MB)",
            max_particles,
            buffer_size / (1024 * 1024)
        );

        Self {
            context,
            buffer,
            allocation: Some(allocation),
            particle_count: max_particles,
        }
    }

    /// Get the Vulkan buffer handle.
    pub fn buffer(&self) -> vk::Buffer {
        self.buffer
    }

    /// Get the buffer size in bytes.
    pub fn size(&self) -> usize {
        self.particle_count as usize * std::mem::size_of::<ParticleData>()
    }

    /// Get the maximum particle count.
    pub fn max_particles(&self) -> u32 {
        self.particle_count
    }

    /// Destroy the particle buffer and release GPU resources.
    pub fn destroy(&mut self) {
        unsafe {
            if let Some(allocation) = self.allocation.take() {
                self.context.allocator.borrow_mut().free(allocation).ok();
            }
            self.context.device.destroy_buffer(self.buffer, None);
        }
    }
}

impl Drop for ParticleBuffer {
    fn drop(&mut self) {
        self.destroy();
    }
}

impl Default for ParticleData {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            _pad1: 0.0,
            velocity: [0.0; 3],
            lifetime: 0.0,
            color: [1.0; 4],
            scale: 1.0,
            _pad2: [0.0; 3],
        }
    }
}

/// Emitter configuration passed to compute shader via push constants.
///
/// This structure is small enough to fit in push constants (<= 128 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct EmitterConfig {
    /// World position of the emitter
    pub position: [f32; 3],
    /// Particles to emit this frame (calculated from emit_rate * delta_time)
    pub emit_count: u32,
    /// Initial velocity direction
    pub velocity_direction: [f32; 3],
    /// Base lifetime for new particles
    pub base_lifetime: f32,
    /// Velocity magnitude (random within cone)
    pub velocity_magnitude: f32,
    /// Random velocity cone angle (0 = straight, PI/2 = hemisphere)
    pub velocity_cone_angle: f32,
    /// Base scale for new particles
    pub base_scale: f32,
    /// Color for new particles (RGBA)
    pub color: [f32; 4],
}

/// Per-frame data for particle simulation.
///
/// Passed to compute shader via uniform buffer.
/// Matches FrameData in particle_sim.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct FrameData {
    /// Delta time for this frame (seconds)
    pub delta_time: f32,
    /// Number of particles to emit this frame
    pub emit_count: u32,
    /// Maximum particle capacity
    pub max_particles: u32,
    /// Random seed for particle initialization
    pub random_seed: u32,
}

impl Default for EmitterConfig {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            emit_count: 0,
            velocity_direction: [0.0, 1.0, 0.0],
            base_lifetime: 5.0,
            velocity_magnitude: 1.0,
            velocity_cone_angle: 0.5,
            base_scale: 0.1,
            color: [1.0, 1.0, 1.0, 1.0],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_particle_data_size() {
        // Verify 64-byte size for cache alignment
        assert_eq!(std::mem::size_of::<ParticleData>(), 64);
    }

    #[test]
    fn test_emitter_config_size() {
        // Should fit in push constants (<= 128 bytes)
        assert!(std::mem::size_of::<EmitterConfig>() <= 128);
    }

    #[test]
    fn test_frame_data_size() {
        // FrameData should be small for uniform buffer
        assert_eq!(std::mem::size_of::<FrameData>(), 16);
    }

    #[test]
    fn test_particle_data_default() {
        let particle = ParticleData::default();
        assert_eq!(particle.position, [0.0; 3]);
        assert_eq!(particle.velocity, [0.0; 3]);
        assert_eq!(particle.lifetime, 0.0);
        assert_eq!(particle.color, [1.0; 4]);
        assert_eq!(particle.scale, 1.0);
    }

    #[test]
    fn test_emitter_config_default() {
        let config = EmitterConfig::default();
        assert_eq!(config.position, [0.0; 3]);
        assert_eq!(config.emit_count, 0);
        assert_eq!(config.velocity_direction, [0.0, 1.0, 0.0]);
        assert_eq!(config.base_lifetime, 5.0);
    }

    #[test]
    fn test_frame_data_default() {
        let data = FrameData {
            delta_time: 0.016,
            emit_count: 10,
            max_particles: 65536,
            random_seed: 42,
        };
        assert_eq!(data.delta_time, 0.016);
        assert_eq!(data.emit_count, 10);
        assert_eq!(data.max_particles, 65536);
        assert_eq!(data.random_seed, 42);
    }
}
