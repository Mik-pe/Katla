//! GPU particle emitter component.
//!
//! This component manages GPU-based particle effects using compute shaders.
//! It holds all the resources needed for particle simulation and rendering.

use ash::vk;
use katla_ecs::Component;
use katla_vulkan::{
    BufferDescriptorSet, ComputePipeline, DeviceAddressBuffer, EmitterConfig, MaterialPipeline,
    ParticleBuffer,
};

/// Frame data for compute shader (must match shader struct)
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct FrameData {
    pub delta_time: f32,
    pub emit_count: u32,
    pub max_particles: u32,
    pub random_seed: u32,
}

/// GPU particle emitter component.
///
/// This component manages a particle system that runs entirely on the GPU.
/// It holds:
/// - ParticleBuffer: Storage for particle data (positions, velocities, lifetimes, etc.)
/// - FrameDataBuffer: Uniform buffer for per-frame simulation data
/// - ComputePipeline: Compute shader for particle simulation
/// - RenderPipeline: Graphics pipeline for billboard rendering
/// - DescriptorSets: For binding buffers to shaders
/// - Config: Emitter settings (position, velocity, spawn rate, etc.)
#[derive(Component)]
pub struct ParticleEmitter {
    /// GPU buffer containing particle data
    pub particle_buffer: ParticleBuffer,
    /// Uniform buffer for frame data (delta_time, emit_count, etc.)
    pub frame_data_buffer: DeviceAddressBuffer,
    /// Compute pipeline for particle simulation
    pub compute_pipeline: ComputePipeline,
    /// Compute descriptor set for buffers
    pub compute_descriptor_set: BufferDescriptorSet,
    /// Graphics pipeline for particle rendering
    pub render_pipeline: MaterialPipeline,
    /// Render descriptor set for particle buffer (set 1)
    pub render_particle_descriptor: BufferDescriptorSet,
    /// Emitter configuration
    pub config: EmitterConfig,
    /// Accumulated time for particle emission
    pub emit_accumulator: f32,
    /// Particles per second to emit
    pub emit_rate: f32,
    /// Whether the emitter is actively spawning particles
    pub is_active: bool,
    /// Current alive particle count (tracked on CPU for dispatch)
    pub alive_count: u32,
    /// Random seed for this frame
    pub random_seed: u32,
}

impl ParticleEmitter {
    /// Create a new particle emitter.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        particle_buffer: ParticleBuffer,
        frame_data_buffer: DeviceAddressBuffer,
        compute_pipeline: ComputePipeline,
        compute_descriptor_set: BufferDescriptorSet,
        render_pipeline: MaterialPipeline,
        render_particle_descriptor: BufferDescriptorSet,
        config: EmitterConfig,
        emit_rate: f32,
    ) -> Self {
        Self {
            particle_buffer,
            frame_data_buffer,
            compute_pipeline,
            compute_descriptor_set,
            render_pipeline,
            render_particle_descriptor,
            config,
            emit_accumulator: 0.0,
            emit_rate,
            is_active: true,
            alive_count: 0,
            random_seed: 0,
        }
    }

    /// Update the emitter for a frame.
    ///
    /// This calculates how many particles to emit based on delta time
    /// and the emit rate, and updates the frame data buffer.
    pub fn update(&mut self, delta_time: f32) {
        // Accumulate time for emission
        if self.is_active {
            self.emit_accumulator += delta_time * self.emit_rate;
        }

        // Calculate particles to emit (integer part of accumulator)
        let emit_count = self.emit_accumulator.floor() as u32;
        self.emit_accumulator -= emit_count as f32;

        // Update random seed (simple increment for now)
        self.random_seed = self.random_seed.wrapping_add(1);

        // Clamp alive count to max capacity
        let max_particles = self.particle_buffer.capacity() as u32;
        self.alive_count = self.alive_count.min(max_particles);

        // Update frame data buffer
        let frame_data = FrameData {
            delta_time,
            emit_count,
            max_particles,
            random_seed: self.random_seed,
        };
        self.frame_data_buffer.write(std::slice::from_ref(&frame_data));
    }

    /// Get the workgroup count for compute dispatch.
    pub fn workgroup_count(&self) -> u32 {
        katla_vulkan::calculate_workgroup_count(self.particle_buffer.capacity() as u32, 256)
    }

    /// Get the max particle count for rendering.
    pub fn particle_count(&self) -> u32 {
        self.particle_buffer.capacity() as u32
    }

    /// Set the emitter position.
    pub fn set_position(&mut self, position: [f32; 3]) {
        self.config.position = position;
    }

    /// Set the emit rate (particles per second).
    pub fn set_emit_rate(&mut self, rate: f32) {
        self.emit_rate = rate;
    }

    /// Set whether the emitter is active.
    pub fn set_active(&mut self, active: bool) {
        self.is_active = active;
    }

    // Compute shader bindings
    pub fn compute_pipeline(&self) -> vk::Pipeline {
        self.compute_pipeline.vk_pipeline()
    }

    pub fn compute_layout(&self) -> vk::PipelineLayout {
        self.compute_pipeline.vk_layout()
    }

    pub fn compute_descriptor(&self) -> vk::DescriptorSet {
        self.compute_descriptor_set.set()
    }

    // Render shader bindings
    pub fn render_pipeline(&self) -> vk::Pipeline {
        self.render_pipeline.vk_pipeline().handle
    }

    pub fn render_layout(&self) -> vk::PipelineLayout {
        self.render_pipeline.vk_layout()
    }

    pub fn render_particle_descriptor(&self) -> vk::DescriptorSet {
        self.render_particle_descriptor.set()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use katla_vulkan::MAX_PARTICLES;

    #[test]
    fn test_workgroup_calculation() {
        let count = katla_vulkan::calculate_workgroup_count(MAX_PARTICLES as u32, 256);
        assert_eq!(count, 256);
    }

    #[test]
    fn test_emit_accumulator() {
        let emit_rate = 100.0;
        let delta_time = 0.016;

        let mut accumulator = 0.0_f32;
        let mut total_emitted = 0u32;

        for _ in 0..60 {
            accumulator += delta_time * emit_rate;
            let emit_count = accumulator.floor() as u32;
            accumulator -= emit_count as f32;
            total_emitted += emit_count;
        }

        assert!(total_emitted >= 95 && total_emitted <= 105);
    }

    #[test]
    fn test_frame_data_size() {
        use std::mem::size_of;
        assert_eq!(size_of::<FrameData>(), 16);
    }
}
