//! GPU particle buffer system for compute-based particle simulation.
//!
//! This module provides types for GPU-based particle effects using compute shaders.
//! The design follows modern AAA approaches (Unreal Niagara, Unity VFX Graph):
//! - Single buffer with `read_write` access (modern GPUs handle this well)
//! - 64K max particles per emitter (fits cache, 4MB per emitter)
//! - DeviceAddressBuffer for persistent mapping and BDA support
//!
//! # Example
//!
//! ```ignore
//! use katla_vulkan::vulkan::particle_buffer::{ParticleBuffer, ParticleData, MAX_PARTICLES};
//! use katla_vulkan::DescriptorSetBuilder;
//!
//! // Create a particle buffer for 64K particles
//! let particle_buffer = ParticleBuffer::new(context.clone(), MAX_PARTICLES)?;
//!
//! // Get device address for compute shader access
//! let address = particle_buffer.device_address();
//!
//! // Create descriptor set using DescriptorSetBuilder
//! let descriptor_set = DescriptorSetBuilder::new(&context)
//!     .storage_buffer(0, &particle_buffer)
//!     .build(layout)?;
//! ```

use ash::vk;
use std::rc::Rc;

use super::context::VulkanContext;
use crate::vulkan::bda::DeviceAddressBuffer;
use crate::vulkan::material::buffer_descriptor::BufferDescriptorSource;

/// Maximum number of particles per emitter.
/// 64K particles = 4MB per emitter (64 bytes * 65536)
/// Fits well in cache and matches typical workgroup dispatch sizes.
pub const MAX_PARTICLES: usize = 65536;

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

impl ParticleData {
    /// Create a new particle at a position with initial velocity.
    pub fn new(position: [f32; 3], velocity: [f32; 3], lifetime: f32) -> Self {
        Self {
            position,
            _pad1: 0.0,
            velocity,
            lifetime,
            color: [1.0, 1.0, 1.0, 1.0],
            scale: 1.0,
            _pad2: [0.0; 3],
        }
    }

    /// Create a particle with full parameters.
    pub fn with_color_and_scale(
        position: [f32; 3],
        velocity: [f32; 3],
        lifetime: f32,
        color: [f32; 4],
        scale: f32,
    ) -> Self {
        Self {
            position,
            _pad1: 0.0,
            velocity,
            lifetime,
            color,
            scale,
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

/// GPU particle buffer using DeviceAddressBuffer.
///
/// Wraps DeviceAddressBuffer for particle storage with:
/// - Persistent mapping for CPU-side initialization
/// - BDA support for compute shader access
/// - BufferDescriptorSource for easy descriptor binding
///
/// # Memory Layout
/// - Uses DeviceAddressBuffer with STORAGE_BUFFER usage
/// - CpuToGpu memory location for persistent mapping
/// - Size = capacity * 64 bytes (ParticleData size)
pub struct ParticleBuffer {
    buffer: DeviceAddressBuffer,
    capacity: usize,
}

impl ParticleBuffer {
    /// Create a new particle buffer.
    ///
    /// # Arguments
    /// * `context` - Vulkan context
    /// * `capacity` - Maximum number of particles
    ///
    /// # Returns
    /// A new ParticleBuffer, or an error if creation fails.
    pub fn new(context: Rc<VulkanContext>, capacity: usize) -> Result<Self, vk::Result> {
        let size = capacity * std::mem::size_of::<ParticleData>();
        let buffer = DeviceAddressBuffer::new_persistent(context, size as u64)?;
        Ok(Self { buffer, capacity })
    }

    /// Create a particle buffer with default max capacity (64K particles).
    pub fn with_max_capacity(context: Rc<VulkanContext>) -> Result<Self, vk::Result> {
        Self::new(context, MAX_PARTICLES)
    }

    /// Get the Vulkan buffer handle.
    pub fn buffer(&self) -> vk::Buffer {
        self.buffer.buffer
    }

    /// Get the maximum number of particles.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get the buffer size in bytes.
    pub fn size(&self) -> u64 {
        self.buffer.size
    }

    /// Get the GPU device address for compute shader access.
    pub fn device_address(&self) -> u64 {
        self.buffer.device_address()
    }

    /// Write particle data to the buffer.
    ///
    /// This is useful for initial seeding of particles.
    pub fn write_particles(&mut self, particles: &[ParticleData]) {
        self.buffer.write(particles);
    }

    /// Get a reference to the underlying DeviceAddressBuffer.
    pub fn inner(&self) -> &DeviceAddressBuffer {
        &self.buffer
    }
}

// Implement BufferDescriptorSource for easy descriptor binding
impl BufferDescriptorSource for ParticleBuffer {
    fn buffer(&self) -> crate::sync::VkBuffer {
        crate::sync::VkBuffer::new(self.buffer.buffer)
    }
}

/// Configuration buffer for emitter settings.
///
/// Stores EmitterConfig in a uniform/storage buffer for shader access.
pub struct EmitterConfigBuffer {
    buffer: DeviceAddressBuffer,
}

impl EmitterConfigBuffer {
    /// Create a new emitter config buffer.
    pub fn new(context: Rc<VulkanContext>) -> Result<Self, vk::Result> {
        let size = std::mem::size_of::<EmitterConfig>();
        let buffer = DeviceAddressBuffer::new_persistent(context, size as u64)?;
        Ok(Self { buffer })
    }

    /// Get the Vulkan buffer handle.
    pub fn buffer(&self) -> vk::Buffer {
        self.buffer.buffer
    }

    /// Get the GPU device address.
    pub fn device_address(&self) -> u64 {
        self.buffer.device_address()
    }

    /// Update the emitter configuration.
    pub fn update(&mut self, config: &EmitterConfig) {
        self.buffer.write(std::slice::from_ref(config));
    }
}

impl BufferDescriptorSource for EmitterConfigBuffer {
    fn buffer(&self) -> crate::sync::VkBuffer {
        crate::sync::VkBuffer::new(self.buffer.buffer)
    }
}

/// Helper function to calculate workgroup count for particle dispatch.
///
/// # Arguments
/// * `particle_count` - Number of particles to process
/// * `workgroup_size` - Workgroup size from shader (typically 256)
///
/// # Returns
/// Number of workgroups needed to process all particles.
#[inline]
pub fn calculate_workgroup_count(particle_count: u32, workgroup_size: u32) -> u32 {
    particle_count.div_ceil(workgroup_size)
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
    fn test_particle_data_default() {
        let particle = ParticleData::default();
        assert_eq!(particle.position, [0.0; 3]);
        assert_eq!(particle.velocity, [0.0; 3]);
        assert_eq!(particle.lifetime, 0.0);
        assert_eq!(particle.color, [1.0; 4]);
        assert_eq!(particle.scale, 1.0);
    }

    #[test]
    fn test_particle_data_new() {
        let particle = ParticleData::new([1.0, 2.0, 3.0], [0.1, 0.2, 0.3], 5.0);
        assert_eq!(particle.position, [1.0, 2.0, 3.0]);
        assert_eq!(particle.velocity, [0.1, 0.2, 0.3]);
        assert_eq!(particle.lifetime, 5.0);
    }

    #[test]
    fn test_workgroup_count_calculation() {
        // 64K particles with workgroup size 256 = 256 workgroups
        assert_eq!(calculate_workgroup_count(65536, 256), 256);

        // Partial workgroup
        assert_eq!(calculate_workgroup_count(257, 256), 2);

        // Exactly one workgroup
        assert_eq!(calculate_workgroup_count(256, 256), 1);

        // Empty
        assert_eq!(calculate_workgroup_count(0, 256), 0);
    }

    #[test]
    fn test_max_particles_memory() {
        // 64K particles * 64 bytes = 4MB
        let memory_mb =
            (MAX_PARTICLES * std::mem::size_of::<ParticleData>()) as f64 / (1024.0 * 1024.0);
        assert!((memory_mb - 4.0).abs() < 0.01);
    }

    #[test]
    fn test_emitter_config_default() {
        let config = EmitterConfig::default();
        assert_eq!(config.position, [0.0; 3]);
        assert_eq!(config.emit_count, 0);
        assert_eq!(config.velocity_direction, [0.0, 1.0, 0.0]);
        assert_eq!(config.base_lifetime, 5.0);
    }
}
