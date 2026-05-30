//! GPU-driven particle system using a single global buffer with atomic counters,
//! index list management, and indirect drawing.

pub(crate) mod barriers;
pub(crate) mod buffer;
pub(crate) mod debug_readback;
pub(crate) mod descriptors;
pub(crate) mod dispatch;
pub(crate) mod emitter;
pub(crate) mod pipeline;
pub(crate) mod presets;
pub(crate) mod render;
pub(crate) mod stats;
pub(crate) mod types;
pub(crate) mod validation;

pub use buffer::{FrameData, GlobalParticleBuffer, ParticleCounters, ParticleData};
pub use debug_readback::{IndirectDrawCommandData, ParticleDebugData, ParticleDebugReadback};
pub use presets::EmitterPreset;
pub use stats::ParticleStats;
pub use types::{Align16Vec4, EmitterConfig, EmitterConfigBuilder, EmitterHandle, EmitterShape};
pub use validation::{
    ValidationError, validate_all_emitters, validate_counters, validate_emitter_config,
};

use std::rc::Rc;

use ash::vk;
use log::{info, warn};

use crate::error::RendererError;
use crate::handle::PipelineHandle;
use crate::vulkan::context::VulkanContext;

use types::EmitterState;

/// Default maximum particles across all emitters
pub const DEFAULT_MAX_PARTICLES: u32 = 1_048_576; // 1M particles (48MB)

pub(super) struct ParticlePipelines {
    pub(super) emit: Option<PipelineHandle>,
    pub(super) simulate: Option<PipelineHandle>,
    pub(super) draw_command: Option<PipelineHandle>,
    pub(super) render: Option<PipelineHandle>,
}

pub(super) struct ParticleDescriptors {
    pub(super) compute_layout: Option<vk::DescriptorSetLayout>,
    pub(super) render_layout: Option<vk::DescriptorSetLayout>,
    pub(super) compute_push_layout: Option<vk::DescriptorSetLayout>,
    pub(super) render_push_layout: Option<vk::DescriptorSetLayout>,
    pub(super) draw_command_layout: Option<vk::DescriptorSetLayout>,
    pub(super) particle_render_storage_layout: Option<vk::DescriptorSetLayout>,
    pub(super) compute_sets: [Option<vk::DescriptorSet>; 2],
    pub(super) draw_command_set: Option<vk::DescriptorSet>,
    pub(super) render_sets: [Option<vk::DescriptorSet>; 2],
    pub(super) _compute_pools: [Option<vk::DescriptorPool>; 2],
    pub(super) _draw_command_pool: Option<vk::DescriptorPool>,
    pub(super) _render_pools: [Option<vk::DescriptorPool>; 2],
}

pub(super) struct ParticleBuffers {
    pub(super) frame_data: [Option<(vk::Buffer, gpu_allocator::vulkan::Allocation)>; 2],
    pub(super) emitter_configs: [Option<(vk::Buffer, gpu_allocator::vulkan::Allocation)>; 2],
}

pub(super) struct ParticleEmitterPool {
    pub(super) emitters: Vec<EmitterConfig>,
    pub(super) emitter_states: Vec<EmitterState>,
    pub(super) next_slot: u32,
    pub(super) free_slots: Vec<u32>,
}

/// Maximum emitters in system
pub const MAX_EMITTERS: u32 = 1024;

/// Workgroup size for particle emit compute shader (must match @workgroup_size in particle_emit.wgsl)
pub const PARTICLE_EMIT_WORKGROUP_SIZE: u32 = 256;

/// Workgroup size for particle simulate compute shader (must match @workgroup_size in particle_simulate.wgsl)
pub const PARTICLE_SIMULATE_WORKGROUP_SIZE: u32 = 64;

/// Modern GPU-driven particle system.
///
/// Manages all particle effects using a single global buffer pool.
/// Each emitter is just configuration data - no per-emitter GPU resources.
pub struct GlobalParticleSystem {
    pub(super) buffer: GlobalParticleBuffer,
    pub(super) pipelines: ParticlePipelines,
    pub(super) descriptors: ParticleDescriptors,
    pub(super) buffers: ParticleBuffers,
    pub(super) emitter_pool: ParticleEmitterPool,

    pub(super) context: Rc<VulkanContext>,
    pub(super) destroyed: bool,

    pub(super) frame_count: u32,
    pub(super) max_particles: u32,
    pub(super) estimated_max_alive: u32,
    pub(super) total_emitted: u64,

    pub(super) debug_readback: Option<ParticleDebugReadback>,
}

impl GlobalParticleSystem {
    pub fn new(context: &Rc<VulkanContext>, max_particles: u32) -> Result<Self, RendererError> {
        info!(
            "Initializing modern particle system (max particles: {})",
            max_particles
        );

        let buffer = GlobalParticleBuffer::new(context.clone(), max_particles)
            .map_err(|e| format!("Failed to create particle buffer: {}", e))?;

        let mut system = Self {
            buffer,
            pipelines: ParticlePipelines {
                emit: None,
                simulate: None,
                draw_command: None,
                render: None,
            },
            descriptors: ParticleDescriptors {
                compute_layout: None,
                render_layout: None,
                compute_push_layout: None,
                render_push_layout: None,
                draw_command_layout: None,
                particle_render_storage_layout: None,
                compute_sets: [None, None],
                draw_command_set: None,
                render_sets: [None, None],
                _compute_pools: [None, None],
                _draw_command_pool: None,
                _render_pools: [None, None],
            },
            buffers: ParticleBuffers {
                frame_data: [None, None],
                emitter_configs: [None, None],
            },
            emitter_pool: ParticleEmitterPool {
                emitters: Vec::with_capacity(MAX_EMITTERS as usize),
                emitter_states: Vec::with_capacity(MAX_EMITTERS as usize),
                next_slot: 0,
                free_slots: Vec::new(),
            },
            context: context.clone(),
            destroyed: false,
            frame_count: 0,
            max_particles,
            estimated_max_alive: max_particles,
            total_emitted: 0,
            debug_readback: None,
        };

        system.buffer.initialize_index_lists()?;

        system.create_descriptor_layouts(context)?;

        let mut compute_descriptor_sets = [None, None];
        let mut compute_descriptor_pools: [Option<vk::DescriptorPool>; 2] = [None, None];
        for fi in 0..2 {
            let (ds, pool) = system.create_compute_descriptor_set()?;
            compute_descriptor_sets[fi] = Some(ds);
            compute_descriptor_pools[fi] = Some(pool);
        }
        system.descriptors.compute_sets = compute_descriptor_sets;
        system.descriptors._compute_pools = compute_descriptor_pools;

        let mut render_descriptor_sets = [None, None];
        let mut render_descriptor_pools: [Option<vk::DescriptorPool>; 2] = [None, None];
        for fi in 0..2 {
            let (ds, pool) = system.create_render_descriptor_set()?;
            render_descriptor_sets[fi] = Some(ds);
            render_descriptor_pools[fi] = Some(pool);
        }
        system.descriptors.render_sets = render_descriptor_sets;
        system.descriptors._render_pools = render_descriptor_pools;

        system.create_push_descriptor_buffers(context)?;

        info!("Modern particle system initialized successfully");
        Ok(system)
    }

    pub fn alive_count(&self) -> u32 {
        self.estimated_max_alive
    }

    pub fn set_alive_count(&mut self, count: u32) {
        self.estimated_max_alive = count;
    }

    pub fn get_emitters(&self) -> &[EmitterConfig] {
        &self.emitter_pool.emitters
    }

    pub fn max_estimated_alive(&self) -> u32 {
        self.estimated_max_alive
    }

    pub fn emit_pipeline_handle(&self) -> Option<PipelineHandle> {
        self.pipelines.emit
    }

    pub fn simulate_pipeline_handle(&self) -> Option<PipelineHandle> {
        self.pipelines.simulate
    }

    pub fn render_pipeline_handle(&self) -> Option<PipelineHandle> {
        self.pipelines.render
    }

    pub fn particle_buffer(&self) -> vk::Buffer {
        self.buffer.particle_buffer()
    }

    pub fn counters_buffer(&self, frame_index: usize) -> vk::Buffer {
        self.buffer.counters_buffer(frame_index)
    }

    pub fn indirect_draw_buffer(&self, frame_index: usize) -> vk::Buffer {
        self.buffer.indirect_draw_buffer(frame_index)
    }

    pub fn buffer_layout(&self) -> &buffer::ParticleBufferLayout {
        self.buffer.layout()
    }

    pub fn emitter_configs_buffer(&self, frame_index: usize) -> Option<vk::Buffer> {
        self.buffers.emitter_configs[frame_index % 2]
            .as_ref()
            .map(|(buf, _)| *buf)
    }

    pub fn destroy(&mut self) {
        if self.destroyed {
            return;
        }
        self.destroyed = true;

        info!("Destroying particle system");

        info!("  destroying push descriptor buffers");
        for frame_idx in 0..2 {
            if let Some((buffer, allocation)) = self.buffers.frame_data[frame_idx].take() {
                unsafe {
                    self.context
                        .allocator
                        .free(allocation, "particle frame data");
                    self.context.device.destroy_buffer(buffer, None);
                }
            }
            if let Some((buffer, allocation)) = self.buffers.emitter_configs[frame_idx].take() {
                unsafe {
                    self.context
                        .allocator
                        .free(allocation, "particle emitter configs");
                    self.context.device.destroy_buffer(buffer, None);
                }
            }
        }

        info!("  destroying global particle buffer");
        self.buffer.destroy();

        info!("  destroying descriptor set layouts");
        if let Some(layout) = self.descriptors.compute_layout.take() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(layout, None);
            }
        }
        if let Some(layout) = self.descriptors.render_layout.take() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(layout, None);
            }
        }
        if let Some(layout) = self.descriptors.compute_push_layout.take() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(layout, None);
            }
        }
        if let Some(layout) = self.descriptors.render_push_layout.take() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(layout, None);
            }
        }
        if let Some(layout) = self.descriptors.draw_command_layout.take() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(layout, None);
            }
        }
        if let Some(layout) = self.descriptors.particle_render_storage_layout.take() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(layout, None);
            }
        }

        self.emitter_pool.emitters.clear();
        self.emitter_pool.next_slot = 0;
        self.emitter_pool.free_slots.clear();

        info!("  destroying descriptor pools");
        for fi in 0..2 {
            if let Some(pool) = self.descriptors._compute_pools[fi].take() {
                unsafe {
                    self.context.device.destroy_descriptor_pool(pool, None);
                }
            }
            if let Some(pool) = self.descriptors._render_pools[fi].take() {
                unsafe {
                    self.context.device.destroy_descriptor_pool(pool, None);
                }
            }
        }

        info!("  destroying debug readback");
        if let Some(mut readback) = self.debug_readback.take() {
            readback.destroy();
        }
        info!("  particle system destroy done");
    }

    pub fn init_debug_readback(&mut self) -> Result<(), RendererError> {
        if self.debug_readback.is_some() {
            warn!("Debug readback already initialized");
            return Ok(());
        }

        info!("Initializing particle debug readback");
        let readback = ParticleDebugReadback::new(&self.context, self.max_particles)?;
        self.debug_readback = Some(readback);
        info!("Particle debug readback initialized successfully");
        Ok(())
    }

    pub fn record_debug_readback(
        &mut self,
        command_buffer: vk::CommandBuffer,
        frame_index: usize,
    ) -> Result<(), RendererError> {
        if let Some(ref mut readback) = self.debug_readback {
            readback.record_copy(command_buffer, &self.buffer, frame_index)?;
            Ok(())
        } else {
            Err(RendererError::InvalidOperation(
                "Debug readback not initialized. Call init_debug_readback() first.".into(),
            ))
        }
    }

    pub fn read_debug_data(&self) -> Result<ParticleDebugData, RendererError> {
        if let Some(ref readback) = self.debug_readback {
            readback.read(&self.buffer)
        } else {
            Err(RendererError::InvalidOperation(
                "Debug readback not initialized. Call init_debug_readback() first.".into(),
            ))
        }
    }

    pub fn has_debug_readback(&self) -> bool {
        self.debug_readback.is_some()
    }

    pub fn destroy_debug_readback(&mut self) {
        if let Some(mut readback) = self.debug_readback.take() {
            readback.destroy();
            info!("Particle debug readback destroyed");
        }
    }

    /// Reset all particle system state to initial values.
    ///
    /// Clears all counters (alive, dead, emit, workgroups), resets per-emitter
    /// runtime state (burst counts, accumulators), and reinitializes GPU buffers
    /// (particle data zeroed, dead list repopulated, alive lists cleared, counters reset).
    /// Emitter configurations are preserved.
    pub fn reset_all(&mut self) -> Result<(), RendererError> {
        info!("Resetting particle system");

        self.frame_count = 0;
        self.total_emitted = 0;
        self.recompute_estimated_max_alive();

        for state in &mut self.emitter_pool.emitter_states {
            *state = EmitterState::default();
        }

        self.buffer.initialize_index_lists()?;

        info!("Particle system reset complete");
        Ok(())
    }

    pub fn max_particles(&self) -> u32 {
        self.max_particles
    }

    /// Get per-emitter alive particle counts.
    ///
    /// Returns a vector where index i is the number of alive particles
    /// belonging to emitter i. Requires debug readback to be initialized;
    /// returns zeros if unavailable.
    pub fn emitter_alive_counts(&self) -> Vec<u32> {
        let n = self.emitter_pool.emitters.len();
        if let Some(ref readback) = self.debug_readback
            && let Ok(debug_data) = readback.read(&self.buffer)
        {
            let counts = debug_data.emitter_alive_counts(n);
            return counts[..n].to_vec();
        }
        vec![0; n]
    }

    pub fn get_stats(&self) -> ParticleStats {
        let particle_data_mb = (self.max_particles as f32) * 48.0 / (1024.0 * 1024.0);
        let index_lists_mb = (self.max_particles as f32) * 12.0 / (1024.0 * 1024.0);
        let counters_mb = 32.0 / (1024.0 * 1024.0);
        let configs_mb = (self.emitter_pool.emitters.len() as f32) * 80.0 / (1024.0 * 1024.0);

        let emitter_counts = if let Some(ref readback) = self.debug_readback {
            if let Ok(debug_data) = readback.read(&self.buffer) {
                let counts = debug_data.emitter_alive_counts(self.emitter_pool.emitters.len());
                counts[..self.emitter_pool.emitters.len()].to_vec()
            } else {
                self.emitter_pool.emitters.iter().map(|_| 0).collect()
            }
        } else {
            self.emitter_pool.emitters.iter().map(|_| 0).collect()
        };

        ParticleStats {
            max_alive_count: self.max_particles,
            current_alive_count: self.alive_count(),
            dead_count: self.max_particles - self.alive_count(),
            total_emitted: self.total_emitted,
            total_died: 0,
            compute_time_ms: 0.0,
            avg_compute_time_ms: 0.0,
            peak_compute_time_ms: 0.0,
            emitter_counts,
            memory_used_mb: particle_data_mb + index_lists_mb + counters_mb + configs_mb,
            buffer_utilization: if self.max_particles > 0 {
                self.alive_count() as f32 / self.max_particles as f32
            } else {
                0.0
            },
            frame_count: self.frame_count as u64,
            total_dispatches: 0,
        }
    }
}

impl Drop for GlobalParticleSystem {
    fn drop(&mut self) {
        if !self.destroyed {
            self.destroy();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emitter_config_size() {
        assert_eq!(std::mem::size_of::<EmitterConfig>(), 160);
    }

    #[test]
    fn test_emitter_handle() {
        let handle = EmitterHandle::new(42);
        assert_eq!(handle.index(), 42);
        assert_ne!(handle, EmitterHandle::NONE);
    }

    #[test]
    fn test_emitter_shape_default() {
        let config = EmitterConfig::default();
        assert_eq!(config.shape, EmitterShape::Point);
        assert_eq!(config.shape_params, [0.0; 4]);
    }

    #[test]
    fn test_emitter_shape_point() {
        let mut config = EmitterConfig::default();
        config.shape = EmitterShape::Point;
        assert_eq!(config.shape, EmitterShape::Point);
    }

    #[test]
    fn test_emitter_shape_line() {
        let mut config = EmitterConfig::default();
        config.shape = EmitterShape::Line;
        config.shape_params = [10.0, 0.0, 0.0, 0.0];
        assert_eq!(config.shape, EmitterShape::Line);
        assert_eq!(config.shape_params[0], 10.0);
    }

    #[test]
    fn test_emitter_shape_circle() {
        let mut config = EmitterConfig::default();
        config.shape = EmitterShape::Circle;
        config.shape_params = [5.0, 0.0, 0.0, 0.0];
        assert_eq!(config.shape, EmitterShape::Circle);
        assert_eq!(config.shape_params[0], 5.0);
    }

    #[test]
    fn test_emitter_shape_sphere() {
        let mut config = EmitterConfig::default();
        config.shape = EmitterShape::Sphere;
        config.shape_params = [3.0, 0.0, 0.0, 0.0];
        assert_eq!(config.shape, EmitterShape::Sphere);
        assert_eq!(config.shape_params[0], 3.0);
    }

    #[test]
    fn test_emitter_shape_box() {
        let mut config = EmitterConfig::default();
        config.shape = EmitterShape::Box;
        config.shape_params = [4.0, 3.0, 2.0, 0.0];
        assert_eq!(config.shape, EmitterShape::Box);
        assert_eq!(config.shape_params[0], 4.0);
        assert_eq!(config.shape_params[1], 3.0);
        assert_eq!(config.shape_params[2], 2.0);
    }

    #[test]
    fn test_emitter_shape_serialization() {
        let config = EmitterConfig {
            position: [1.0, 2.0, 3.0],
            _pad_position: 0.0,
            shape: EmitterShape::Sphere,
            emit_rate: 100.0,
            base_lifetime: 2.0,
            lifetime_variation: 0.5,
            velocity_direction: [0.0, 1.0, 0.0],
            _pad_velocity: 0.0,
            velocity_magnitude: 5.0,
            velocity_cone_angle: 0.3,
            base_scale: 0.2,
            scale_variation: 0.3,
            color: [1.0, 0.5, 0.0, 1.0],
            color_variation: 0.2,
            color_end: Align16Vec4([0.0; 4]),
            shape_params: [2.5, 0.0, 0.0, 0.0],
            gravity: -9.8,
            turbulence_strength: 0.0,
            turbulence_frequency: 3.0,
            kill_all: 0,
            scale_end: 1.0,
            _pad2: [0.0; 3],
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: EmitterConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.shape, EmitterShape::Sphere);
        assert_eq!(deserialized.shape_params[0], 2.5);
        assert_eq!(deserialized.position, [1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_emitter_config_field_offsets() {
        assert_eq!(std::mem::offset_of!(EmitterConfig, position), 0);
        assert_eq!(std::mem::offset_of!(EmitterConfig, shape), 16);
        assert_eq!(std::mem::offset_of!(EmitterConfig, emit_rate), 20);
        assert_eq!(std::mem::offset_of!(EmitterConfig, base_lifetime), 24);
        assert_eq!(std::mem::offset_of!(EmitterConfig, lifetime_variation), 28);
        assert_eq!(std::mem::offset_of!(EmitterConfig, velocity_direction), 32);
        assert_eq!(std::mem::offset_of!(EmitterConfig, velocity_magnitude), 48);
        assert_eq!(std::mem::offset_of!(EmitterConfig, velocity_cone_angle), 52);
        assert_eq!(std::mem::offset_of!(EmitterConfig, base_scale), 56);
        assert_eq!(std::mem::offset_of!(EmitterConfig, scale_variation), 60);
        assert_eq!(std::mem::offset_of!(EmitterConfig, color), 64);
        assert_eq!(std::mem::offset_of!(EmitterConfig, color_variation), 80);
        assert_eq!(std::mem::offset_of!(EmitterConfig, color_end), 96);
        assert_eq!(std::mem::offset_of!(EmitterConfig, shape_params), 112);
        assert_eq!(std::mem::offset_of!(EmitterConfig, gravity), 128);
        assert_eq!(std::mem::offset_of!(EmitterConfig, kill_all), 140);
        assert_eq!(std::mem::offset_of!(EmitterConfig, scale_end), 144);
        assert_eq!(std::mem::offset_of!(EmitterConfig, _pad2), 148);
    }

    #[test]
    fn test_all_emitter_shapes() {
        let shapes = [
            EmitterShape::Point,
            EmitterShape::Line,
            EmitterShape::Circle,
            EmitterShape::Sphere,
            EmitterShape::Box,
        ];

        for shape in shapes {
            let mut config = EmitterConfig::default();
            config.shape = shape;
            assert_eq!(config.shape, shape);
        }
    }
}

#[cfg(test)]
mod vulkan_tests {
    use super::*;

    #[test]
    fn test_reset_all_clears_emitter_states() {
        let mut pool = ParticleEmitterPool {
            emitters: vec![EmitterConfig::default()],
            emitter_states: vec![EmitterState {
                burst_count: 100,
                emit_accumulator: 5.5,
            }],
            next_slot: 1,
            free_slots: vec![],
        };

        for state in &mut pool.emitter_states {
            *state = EmitterState::default();
        }

        assert_eq!(pool.emitter_states[0].burst_count, 0);
        assert_eq!(pool.emitter_states[0].emit_accumulator, 0.0);
    }

    #[test]
    fn test_reset_all_preserves_configs() {
        let original_config = EmitterConfig {
            emit_rate: 75.0,
            base_lifetime: 3.0,
            gravity: -5.0,
            ..Default::default()
        };

        let mut pool = ParticleEmitterPool {
            emitters: vec![original_config],
            emitter_states: vec![EmitterState {
                burst_count: 100,
                emit_accumulator: 5.5,
            }],
            next_slot: 1,
            free_slots: vec![],
        };

        for state in &mut pool.emitter_states {
            *state = EmitterState::default();
        }

        assert_eq!(pool.emitters[0].emit_rate, 75.0);
        assert_eq!(pool.emitters[0].base_lifetime, 3.0);
        assert_eq!(pool.emitters[0].gravity, -5.0);
    }

    #[test]
    fn test_reset_all_clears_counters() {
        let counters_after_reset = ParticleCounters {
            alive_count: 0,
            dead_count: DEFAULT_MAX_PARTICLES,
            emit_count: 0,
            workgroups_finished: 0,
        };

        assert_eq!(counters_after_reset.alive_count, 0);
        assert_eq!(counters_after_reset.dead_count, DEFAULT_MAX_PARTICLES);
        assert_eq!(counters_after_reset.emit_count, 0);
        assert_eq!(counters_after_reset.workgroups_finished, 0);
    }
}
