//! Modern GPU-driven particle system using single global buffer.
//!
//! This module implements a 2025-vulkan particle system with:
//! - Single global buffer for all emitters
//! - GPU-driven lifecycle via atomic counters
//! - Index list management for efficient particle tracking
//! - Hybrid descriptor approach (static + push descriptors)
//! - Indirect drawing for optimal GPU utilization
//!
//! # Architecture
//!
//! ## Data Structures
//!
//! - `GlobalParticleSystem`: Main particle system manager
//! - `GlobalParticleBuffer`: Single buffer for all particles + index lists
//! - `EmitterConfig`: Per-emitter configuration for particle behavior
//!
//! ## Pipeline Flow
//!
//! 1. **Compute Pass**: Emit new particles → Simulate alive particles → Update index lists
//! 2. **Render Pass**: Indirect draw using alive list (only alive particles rendered)
//!
//! # Memory Layout
//!
//! The particle system uses a single ~60MB GPU buffer for 1M particles:
//! - Particle data: 48 MB (1M × 48 bytes per particle)
//! - Dead list: 4 MB (1M × 4 bytes indices)
//! - Alive list current: 4 MB
//! - Alive list next: 4 MB
//! - Counters: 32 bytes (atomic counters)
//! - Emitter configs: 80 KB (1024 × 80 bytes per emitter)
//!
//! # Example
//!
//! ```ignore
//! // Create particle system (typically done during engine initialization)
//! let mut particle_system = GlobalParticleSystem::new(&context, 1_048_576)?;
//!
//! // Create an emitter (e.g., fire effect at specific position)
//! let fire_emitter = particle_system.create_emitter(EmitterConfig {
//!     position: [0.0, 1.0, 0.0],
//!     emit_rate: 1000.0,
//!     base_lifetime: 2.0,
//!     velocity_direction: [0.0, 1.0, 0.0],
//!     velocity_magnitude: 2.0,
//!     color: [1.0, 0.5, 0.0, 1.0], // Orange
//!     ..Default::default()
//! })?;
//!
//! // Each frame: update simulation
//! let alive_count = particle_system.update(delta_time)?;
//!
//! // Render particles (after tonemap pass)
//! particle_system.render(command_buffer, render_pass)?;
//! ```
//!
//! # Performance Characteristics
//!
//! - **Memory**: Fixed 60MB GPU allocation for 1M particles
//! - **CPU**: Minimal overhead (only config updates, no per-particle work)
//! - **GPU**: Single compute dispatch for emission + simulation
//! - **Draw**: Indirect draw renders only alive particles (no vertex processing overhead)
//!
//! # Integration with ECS
//!
//! The particle system integrates with the ECS via `ParticleSystem` wrapper in `katla_app`:
//!
//! ```ignore
//! // In your ECS system
//! self.particle_system.update(&mut world, &mut renderer.particle_system);
//!
//! // Particle emitters are regular ECS components
//! world.add_entity(entity)
//!     .with(ParticleEmitterComponent::fire_effect([0.0, 1.0, 0.0]))?
//!     .build()?;
//! ```

pub mod buffer;
pub mod presets;
pub mod stats;
pub mod timing;
pub mod validation;

pub use buffer::{FrameData, GlobalParticleBuffer, ParticleCounters};
pub use presets::{EmitterPreset, PresetManager};
pub use stats::ParticleStats;
pub use timing::TimestampQuery;
pub use validation::{
    ValidationError, validate_all_emitters, validate_counters, validate_emitter_config,
};

use std::rc::Rc;

use ash::vk;
use log::{info, warn};
use serde::{Deserialize, Serialize};

use crate::handle::PipelineHandle;
use crate::renderer::registry::AssetRegistry;
use crate::sync::{
    AccessFlags2, BufferMemoryBarrier2, DependencyInfo, PipelineStage2Flags, VkBuffer,
    VkShaderModule,
};
use crate::vulkan::context::VulkanContext;
use crate::vulkan::material::compute_pipeline::ComputePipelineBuilder;

/// Emitter shape for particle spawn positions.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum EmitterShape {
    #[default]
    Point = 0,
    Line = 1,
    Circle = 2,
    Sphere = 3,
    Box = 4,
}

impl EmitterShape {
    /// Convert to u32 for GPU
    pub fn as_u32(self) -> u32 {
        self as u32
    }

    /// Convert from u32 from GPU
    pub fn from_u32(val: u32) -> Self {
        match val {
            0 => EmitterShape::Point,
            1 => EmitterShape::Line,
            2 => EmitterShape::Circle,
            3 => EmitterShape::Sphere,
            4 => EmitterShape::Box,
            _ => EmitterShape::Point,
        }
    }
}

/// Default maximum particles across all emitters
pub const DEFAULT_MAX_PARTICLES: u32 = 1_048_576; // 1M particles (48MB)

/// Maximum emitters in system
pub const MAX_EMITTERS: u32 = 1024;

/// Workgroup size for particle compute shader
pub const PARTICLE_WORKGROUP_SIZE: u32 = 256;

/// Per-emitter configuration for GPU.
///
/// This is uploaded to a storage buffer that the compute shader
/// accesses when spawning particles.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable, Serialize, Deserialize)]
pub struct EmitterConfig {
    /// World position of emitter
    #[serde(default = "default_position")]
    pub position: [f32; 3],

    /// Emitter shape (Point, Line, Circle, Sphere, Box)
    #[serde(default)]
    pub shape: u32,

    /// Particles to emit per second
    #[serde(default = "default_emit_rate")]
    pub emit_rate: f32,

    /// Base lifetime for new particles (seconds)
    #[serde(default = "default_base_lifetime")]
    pub base_lifetime: f32,

    /// Random variation in lifetime (±percentage)
    #[serde(default = "default_lifetime_variation")]
    pub lifetime_variation: f32,

    /// Base velocity direction (normalized)
    #[serde(default = "default_velocity_direction")]
    pub velocity_direction: [f32; 3],

    #[serde(skip)]
    pub _pad0: f32,

    /// Velocity magnitude
    #[serde(default = "default_velocity_magnitude")]
    pub velocity_magnitude: f32,

    /// Velocity spread cone angle (0 = straight, PI/2 = hemisphere)
    #[serde(default = "default_velocity_cone_angle")]
    pub velocity_cone_angle: f32,

    /// Base scale for new particles
    #[serde(default = "default_base_scale")]
    pub base_scale: f32,

    /// Scale variation (±percentage)
    #[serde(default = "default_scale_variation")]
    pub scale_variation: f32,

    /// Color for new particles (RGBA)
    #[serde(default = "default_color")]
    pub color: [f32; 4],

    /// Color variation (±percentage per channel)
    #[serde(default = "default_color_variation")]
    pub color_variation: f32,

    /// Shape parameters (length/radius for Line/Circle/Sphere, dimensions for Box)
    #[serde(default)]
    pub shape_params: [f32; 4],
}

impl EmitterConfig {
    /// Get the emitter shape as an enum
    pub fn get_shape(&self) -> EmitterShape {
        EmitterShape::from_u32(self.shape)
    }

    /// Set the emitter shape from an enum
    pub fn set_shape(&mut self, shape: EmitterShape) {
        self.shape = shape.as_u32();
    }
}

// Serde default functions
fn default_position() -> [f32; 3] {
    [0.0; 3]
}
fn default_emit_rate() -> f32 {
    50.0
}
fn default_base_lifetime() -> f32 {
    5.0
}
fn default_lifetime_variation() -> f32 {
    0.2
}
fn default_velocity_direction() -> [f32; 3] {
    [0.0, 1.0, 0.0]
}
fn default_velocity_magnitude() -> f32 {
    1.0
}
fn default_velocity_cone_angle() -> f32 {
    0.5
}
fn default_base_scale() -> f32 {
    0.1
}
fn default_scale_variation() -> f32 {
    0.5
}
fn default_color() -> [f32; 4] {
    [1.0, 1.0, 1.0, 1.0]
}
fn default_color_variation() -> f32 {
    0.1
}

impl Default for EmitterConfig {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            shape: EmitterShape::Point.as_u32(),
            emit_rate: 50.0,
            base_lifetime: 5.0,
            lifetime_variation: 0.2,
            velocity_direction: [0.0, 1.0, 0.0],
            _pad0: 0.0,
            velocity_magnitude: 1.0,
            velocity_cone_angle: 0.5,
            base_scale: 0.1,
            scale_variation: 0.5,
            color: [1.0, 1.0, 1.0, 1.0],
            color_variation: 0.1,
            shape_params: [0.0; 4],
        }
    }
}

/// Handle to an emitter in the global particle system.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EmitterHandle {
    index: u32,
}

impl EmitterHandle {
    /// Invalid emitter handle
    pub const NONE: Self = Self { index: u32::MAX };

    /// Create a new emitter handle from index
    pub fn new(index: u32) -> Self {
        Self { index }
    }

    /// Get the emitter index
    pub fn index(&self) -> u32 {
        self.index
    }
}

/// Per-emitter runtime state (not uploaded to GPU).
#[derive(Clone, Default)]
struct EmitterState {
    /// Burst particles to emit this frame
    burst_count: u32,
}

/// Modern GPU-driven particle system.
///
/// Manages all particle effects using a single global buffer pool.
/// Each emitter is just configuration data - no per-emitter GPU resources.
pub struct GlobalParticleSystem {
    /// Global particle buffer (particles + index lists + counters)
    buffer: GlobalParticleBuffer,

    /// Compute pipeline for particle emission
    emit_pipeline: Option<PipelineHandle>,

    /// Compute pipeline for particle simulation
    simulate_pipeline: Option<PipelineHandle>,

    /// Render pipeline for particle rendering
    render_pipeline: Option<PipelineHandle>,

    /// Descriptor set layout for compute (Set 0: static buffers)
    compute_descriptor_layout: Option<vk::DescriptorSetLayout>,

    /// Descriptor set layout for render (Set 0: static buffers)
    render_descriptor_layout: Option<vk::DescriptorSetLayout>,

    /// Descriptor set layout for compute push descriptors (Set 1: frame data + emitter configs)
    compute_push_descriptor_layout: Option<vk::DescriptorSetLayout>,

    /// Descriptor set layout for render push descriptors (Set 1: frame data)
    render_push_descriptor_layout: Option<vk::DescriptorSetLayout>,

    /// Per-emitter configurations (CPU-side, uploaded to GPU each frame)
    emitters: Vec<EmitterConfig>,

    /// Per-emitter runtime state (burst counts, etc.)
    emitter_states: Vec<EmitterState>,

    /// Next free emitter slot
    next_emitter_slot: u32,

    /// Vulkan context for resource creation
    context: Rc<VulkanContext>,

    /// Frame counter for emission timing
    frame_count: u32,

    /// Cached alive particle count (for rendering)
    cached_alive_count: u32,

    /// Frame data buffer for push descriptor updates
    frame_data_buffer: Option<(vk::Buffer, gpu_allocator::vulkan::Allocation)>,

    /// Emitter configs buffer for push descriptor updates
    emitter_configs_buffer: Option<(vk::Buffer, gpu_allocator::vulkan::Allocation)>,

    /// Flag to prevent double destruction
    destroyed: bool,

    /// Static descriptor set for compute (Set 0)
    compute_descriptor_set: Option<vk::DescriptorSet>,

    /// Static descriptor set for graphics/render (Set 0)
    /// Uses the same layout but different stage flags (VERTEX/FRAGMENT vs COMPUTE)
    render_descriptor_set: Option<vk::DescriptorSet>,

    /// Descriptor pool for compute descriptor set
    /// NOTE: This pool's lifetime is tied to the particle system itself
    /// and will be destroyed when the particle system is dropped
    _compute_descriptor_pool: vk::DescriptorPool,

    /// Descriptor pool for render descriptor set
    _render_descriptor_pool: vk::DescriptorPool,

    /// GPU timing queries for compute shader
    timing_queries: Option<TimestampQuery>,

    // Statistics tracking
    /// Total particles emitted since system start
    total_emitted: u64,
    /// Total particles that died since system start
    total_died: u64,
    /// Peak compute shader execution time (milliseconds)
    peak_compute_time: f32,
    /// Average compute shader execution time (milliseconds)
    avg_compute_time: f32,
    /// History of compute times for rolling average (last 60 frames)
    compute_time_history: Vec<f32>,
    /// Total compute dispatches executed
    total_dispatches: u64,
    /// Maximum particles in the system
    max_particles: u32,
}

impl GlobalParticleSystem {
    /// Create a new global particle system.
    ///
    /// # Arguments
    /// * `renderer` - Vulkan renderer (borrowed for initialization only)
    /// * `max_particles` - Maximum particles across all emitters (default: 1M)
    ///
    /// # Returns
    /// Initialized particle system ready for emitter creation
    pub fn new(context: &Rc<VulkanContext>, max_particles: u32) -> Result<Self, String> {
        info!(
            "Initializing modern particle system (max particles: {})",
            max_particles
        );

        let buffer = GlobalParticleBuffer::new(context.clone(), max_particles)
            .map_err(|e| format!("Failed to create particle buffer: {}", e))?;

        let mut system = Self {
            buffer,
            emit_pipeline: None,
            simulate_pipeline: None,
            render_pipeline: None,
            compute_descriptor_layout: None,
            render_descriptor_layout: None,
            compute_push_descriptor_layout: None,
            render_push_descriptor_layout: None,
            emitters: Vec::with_capacity(MAX_EMITTERS as usize),
            emitter_states: Vec::with_capacity(MAX_EMITTERS as usize),
            next_emitter_slot: 0,
            context: context.clone(),
            frame_count: 0,
            frame_data_buffer: None,
            emitter_configs_buffer: None,
            cached_alive_count: 0,
            destroyed: false,
            compute_descriptor_set: None,
            render_descriptor_set: None,
            _compute_descriptor_pool: vk::DescriptorPool::null(),
            _render_descriptor_pool: vk::DescriptorPool::null(),
            timing_queries: None,
            total_emitted: 0,
            total_died: 0,
            peak_compute_time: 0.0,
            avg_compute_time: 0.0,
            compute_time_history: Vec::with_capacity(60),
            total_dispatches: 0,
            max_particles,
        };

        // Initialize index lists (all particles start dead, alive lists are empty)
        system.buffer.initialize_index_lists()?;

        // Create descriptor set layouts
        system.create_descriptor_layouts(context)?;

        // Create and allocate static descriptor sets
        let (compute_descriptor_set, compute_pool) = system.create_compute_descriptor_set()?;
        system.compute_descriptor_set = Some(compute_descriptor_set);
        system._compute_descriptor_pool = compute_pool;

        let (render_descriptor_set, render_pool) = system.create_render_descriptor_set()?;
        system.render_descriptor_set = Some(render_descriptor_set);
        system._render_descriptor_pool = render_pool;

        // Create push descriptor buffers
        system.create_push_descriptor_buffers(context)?;

        // Create timing queries
        match TimestampQuery::new(context) {
            Ok(timing) => {
                system.timing_queries = Some(timing);
                info!("Created GPU timing queries for particle compute shader");
            }
            Err(e) => {
                warn!("Failed to create timing queries: {}. Timing disabled.", e);
                system.timing_queries = None;
            }
        }

        info!("Modern particle system initialized successfully");
        Ok(system)
    }

    /// Create a new particle emitter.
    ///
    /// This is lightweight - just allocates a config slot.
    /// No GPU resources are created per-emitter.
    ///
    /// # Arguments
    /// * `config` - Emitter configuration (position, emit rate, color, etc.)
    ///
    /// # Returns
    /// Handle to the emitter for use with update/destroy
    ///
    /// # Errors
    /// Returns error if maximum emitter count (1024) is reached
    pub fn create_emitter(&mut self, config: EmitterConfig) -> Result<EmitterHandle, String> {
        if self.emitters.len() >= MAX_EMITTERS as usize {
            log::warn!(
                "Cannot create emitter: maximum emitter count ({}) reached",
                MAX_EMITTERS
            );
            return Err(format!("Maximum emitter count ({}) reached", MAX_EMITTERS));
        }

        let index = self.next_emitter_slot;
        self.next_emitter_slot += 1;

        // Ensure vectors have space
        if self.emitters.len() <= index as usize {
            self.emitters
                .resize(index as usize + 1, EmitterConfig::default());
        }
        if self.emitter_states.len() <= index as usize {
            self.emitter_states
                .resize(index as usize + 1, EmitterState::default());
        }

        self.emitters[index as usize] = config;

        log::debug!(
            "Created particle emitter {} at position {:?}",
            index,
            config.position
        );

        Ok(EmitterHandle::new(index))
    }

    /// Update emitter configuration.
    ///
    /// Call this to change emit rate, position, color, etc.
    pub fn update_emitter(&mut self, handle: EmitterHandle, config: EmitterConfig) {
        if handle.index() < self.emitters.len() as u32 {
            self.emitters[handle.index() as usize] = config;
        } else {
            warn!("Invalid emitter handle: {:?}", handle);
        }
    }

    /// Burst particles from an emitter immediately.
    ///
    /// This overrides the normal emit rate for this frame and emits
    /// the specified number of particles immediately.
    ///
    /// # Arguments
    /// * `handle` - Emitter handle
    /// * `count` - Number of particles to burst
    pub fn burst(&mut self, handle: EmitterHandle, count: u32) -> Result<(), String> {
        if handle.index() < self.emitter_states.len() as u32 {
            self.emitter_states[handle.index() as usize].burst_count = count;
            log::debug!("Burst {} particles from emitter {}", count, handle.index());
            Ok(())
        } else {
            Err(format!("Invalid emitter handle: {:?}", handle))
        }
    }

    /// Destroy an emitter.
    ///
    /// Frees the config slot for reuse.
    pub fn destroy_emitter(&mut self, handle: EmitterHandle) {
        if handle.index() < self.emitters.len() as u32 {
            self.emitters[handle.index() as usize] = EmitterConfig::default();
            if handle.index() < self.emitter_states.len() as u32 {
                self.emitter_states[handle.index() as usize] = EmitterState::default();
            }
            info!("Destroyed particle emitter {}", handle.index());
        }
    }

    /// Update particle simulation and emit new particles.
    ///
    /// Call this once per frame before rendering.
    ///
    /// # Arguments
    /// * `delta_time` - Frame time in seconds
    /// * `frame_index` - Current frame index (for per-frame buffer offsets)
    ///
    /// # Returns
    /// Number of active particles this frame
    ///
    /// # Errors
    /// Returns error if frame data upload fails, but gracefully continues
    /// with cached particle count to avoid rendering interruptions
    pub fn update(&mut self, delta_time: f32, frame_index: u32) -> Result<u32, String> {
        self.frame_count += 1;

        // Upload emitter configs to GPU buffer
        self.upload_emitter_configs()?;

        // Calculate total particles to emit this frame (including bursts)
        let total_burst_count: u32 = self
            .emitter_states
            .iter()
            .map(|state| state.burst_count)
            .sum();

        let total_emit_count: u32 = self
            .emitters
            .iter()
            .map(|config| (config.emit_rate * delta_time) as u32)
            .sum();

        let total_this_frame = total_emit_count + total_burst_count;

        // Update frame data buffer
        self.update_frame_data(delta_time, total_emit_count, total_burst_count, frame_index)?;

        // Clear burst counts after processing
        for state in &mut self.emitter_states {
            state.burst_count = 0;
        }

        // The actual compute dispatch happens during render graph execution
        // via record_compute_dispatch(). This just prepares the data.

        // Read back alive count from counters buffer (from previous frame)
        let alive_count = self.buffer.get_alive_count().unwrap_or(0);
        self.cached_alive_count = alive_count;

        // Debug-only validation: Check counter consistency
        #[cfg(debug_assertions)]
        {
            if let Ok(dead_count) = self.buffer.get_dead_count()
                && let Err(e) = validate_counters(alive_count, dead_count, self.max_particles)
            {
                log::warn!("Particle system validation error: {}", e);
            }

            // Validate all active emitters
            let validation_errors = validate_all_emitters(&self.emitters);
            if !validation_errors.is_empty() {
                for error in &validation_errors {
                    log::warn!("Emitter validation error: {}", error);
                }
            }
        }

        // Update emission statistics
        if total_this_frame > 0 {
            self.update_emission_stats(total_this_frame);
        }

        Ok(alive_count)
    }

    /// Upload emitter configurations to GPU buffer.
    fn upload_emitter_configs(&self) -> Result<(), String> {
        if let Some((_buffer, allocation)) = &self.emitter_configs_buffer {
            if let Some(mapped) = allocation.mapped_ptr() {
                let dst = mapped.as_ptr() as *mut EmitterConfig;
                unsafe {
                    std::ptr::copy_nonoverlapping(self.emitters.as_ptr(), dst, self.emitters.len());
                }
                self.context.flush_mapped_memory(
                    allocation,
                    0,
                    (self.emitters.len() * std::mem::size_of::<EmitterConfig>()) as u64,
                );
            } else {
                log::warn!("Emitter configs buffer is not mapped for CPU access");
                return Err("Emitter configs buffer mapping failed".to_string());
            }
        } else {
            log::warn!("Emitter configs buffer not initialized");
            return Err("Emitter configs buffer not created".to_string());
        }
        Ok(())
    }

    /// Update frame data for push descriptor.
    fn update_frame_data(
        &self,
        delta_time: f32,
        emit_count: u32,
        burst_count: u32,
        frame_index: u32,
    ) -> Result<(), String> {
        if let Some((_buffer, allocation)) = &self.frame_data_buffer {
            if let Some(mapped) = allocation.mapped_ptr() {
                // Calculate active emitter count (emitters with emit_rate > 0 or burst_count > 0)
                let active_emitter_count =
                    self.emitters.iter().filter(|e| e.emit_rate > 0.0).count() as u32;

                // Total particles to simulate = newly emitted + previously alive
                let total_simulate_count = emit_count + burst_count + self.cached_alive_count;

                let frame_data = FrameData {
                    delta_time,
                    total_emit_count: emit_count + burst_count,
                    emitter_count: active_emitter_count,
                    random_seed: self.frame_count,
                    total_simulate_count,
                    burst_count,
                    frame_index,
                    _pad: [0u32; 9],
                };
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        &frame_data as *const FrameData as *const u8,
                        mapped.as_ptr() as *mut u8,
                        std::mem::size_of::<FrameData>(),
                    );
                }
                self.context.flush_mapped_memory(
                    allocation,
                    0,
                    std::mem::size_of::<FrameData>() as u64,
                );
            } else {
                log::warn!("Frame data buffer is not mapped for CPU access");
                return Err("Frame data buffer mapping failed".to_string());
            }
        } else {
            log::warn!("Frame data buffer not initialized");
            return Err("Frame data buffer not created".to_string());
        }
        Ok(())
    }

    /// Render particles.
    ///
    /// Uses direct drawing with vertex count based on alive particles.
    /// Each particle renders as 6 vertices (2 triangles for a quad).
    ///
    /// # Arguments
    /// * `command_buffer` - Command buffer to record draw calls into
    /// * `render_pass` - Current render pass
    /// * `pipeline` - Graphics pipeline to use for rendering
    /// * `layout` - Pipeline layout for descriptor binding
    /// * `storage_descriptor_set` - Storage descriptor set (Set 1) from renderer containing FrameUniforms
    pub fn render(
        &mut self,
        command_buffer: vk::CommandBuffer,
        _render_pass: vk::RenderPass,
        pipeline: vk::Pipeline,
        layout: vk::PipelineLayout,
        storage_descriptor_set: vk::DescriptorSet,
    ) -> Result<(), String> {
        let device = &self.context.device;

        // Bind graphics pipeline
        unsafe {
            device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::GRAPHICS, pipeline);
        }

        // Bind static descriptor set (Set 0: particle buffers)
        // CRITICAL: Use render_descriptor_set (with VERTEX/FRAGMENT stage flags)
        // NOT compute_descriptor_set (with COMPUTE stage flags)
        if let Some(descriptor_set) = self.render_descriptor_set {
            unsafe {
                device.cmd_bind_descriptor_sets(
                    command_buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    layout,
                    0, // Set 0
                    std::slice::from_ref(&descriptor_set),
                    &[],
                );
            }
        } else {
            return Err("Particle render descriptor set not allocated".to_string());
        }

        // Bind storage descriptor set (Set 1: FrameUniforms from renderer)
        // This provides view/proj matrices for camera transformation
        unsafe {
            device.cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                layout,
                1, // Set 1
                std::slice::from_ref(&storage_descriptor_set),
                &[],
            );
        }

        // Draw particles (6 vertices per particle)
        let vertex_count = self.cached_alive_count * 6;
        if vertex_count > 0 {
            unsafe {
                device.cmd_draw(command_buffer, vertex_count, 1, 0, 0);
            }
        }

        Ok(())
    }

    /// Get current alive particle count.
    pub fn alive_count(&self) -> u32 {
        self.cached_alive_count
    }

    /// Get emitter configurations (for compute dispatch).
    pub fn get_emitters(&self) -> &[EmitterConfig] {
        &self.emitters
    }

    /// Get emit pipeline handle.
    pub fn emit_pipeline_handle(&self) -> Option<PipelineHandle> {
        self.emit_pipeline
    }

    /// Get simulate pipeline handle.
    pub fn simulate_pipeline_handle(&self) -> Option<PipelineHandle> {
        self.simulate_pipeline
    }

    /// Destroy all particle system resources.
    pub fn destroy(&mut self) {
        if self.destroyed {
            return;
        }
        self.destroyed = true;

        info!("Destroying particle system");

        // Destroy push descriptor buffers first
        if let Some((buffer, allocation)) = self.frame_data_buffer.take() {
            unsafe {
                if let Ok(mut allocator) = self.context.allocator.try_borrow_mut() {
                    allocator.free(allocation).ok();
                }
                self.context.device.destroy_buffer(buffer, None);
            }
        }
        if let Some((buffer, allocation)) = self.emitter_configs_buffer.take() {
            unsafe {
                if let Ok(mut allocator) = self.context.allocator.try_borrow_mut() {
                    allocator.free(allocation).ok();
                }
                self.context.device.destroy_buffer(buffer, None);
            }
        }

        self.buffer.destroy();

        // Destroy timing queries
        if let Some(mut timing) = self.timing_queries.take() {
            timing.destroy();
        }

        // Destroy descriptor set layouts (we own these, pipelines just reference them)
        if let Some(layout) = self.compute_descriptor_layout.take() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(layout, None);
            }
        }
        if let Some(layout) = self.render_descriptor_layout.take() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(layout, None);
            }
        }
        if let Some(layout) = self.compute_push_descriptor_layout.take() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(layout, None);
            }
        }
        if let Some(layout) = self.render_push_descriptor_layout.take() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(layout, None);
            }
        }

        self.emitters.clear();
        self.next_emitter_slot = 0;

        // Destroy descriptor pools
        if self._compute_descriptor_pool != vk::DescriptorPool::null() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_pool(self._compute_descriptor_pool, None);
            }
            self._compute_descriptor_pool = vk::DescriptorPool::null();
        }
        if self._render_descriptor_pool != vk::DescriptorPool::null() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_pool(self._render_descriptor_pool, None);
            }
            self._render_descriptor_pool = vk::DescriptorPool::null();
        }
    }

    /// Create descriptor set layouts for particle system.
    fn create_descriptor_layouts(&mut self, context: &Rc<VulkanContext>) -> Result<(), String> {
        // Compute layout (Set 0: static buffers only - particles, dead list, alive lists, counters)
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
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(4)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];

        let compute_layout_info =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(&compute_bindings);

        let compute_layout = unsafe {
            context
                .device
                .create_descriptor_set_layout(&compute_layout_info, None)
                .map_err(|e| format!("Failed to create compute descriptor layout: {:?}", e))?
        };

        self.compute_descriptor_layout = Some(compute_layout);

        // Compute push descriptor layout (Set 1: frame data + emitter configs)
        // NOTE: This uses PUSH_DESCRIPTOR bit to indicate these will be pushed via cmd_push_descriptor_set
        let compute_push_bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];

        let compute_push_layout_create_info = vk::DescriptorSetLayoutCreateInfo::default()
            .bindings(&compute_push_bindings)
            .flags(vk::DescriptorSetLayoutCreateFlags::PUSH_DESCRIPTOR_KHR);

        let compute_push_layout = unsafe {
            context
                .device
                .create_descriptor_set_layout(&compute_push_layout_create_info, None)
                .map_err(|e| format!("Failed to create compute push descriptor layout: {:?}", e))?
        };

        self.compute_push_descriptor_layout = Some(compute_push_layout);

        // Render layout (Set 0: particle data + alive lists)
        // We reuse the compute layout since both pipelines share the same descriptor set
        // The render shader uses binding 0 (particles) and binding 2 (alive_current from compute)
        // All 5 bindings must match the compute layout for descriptor set compatibility
        let render_bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(3)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(4)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
        ];

        let render_layout_info =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(&render_bindings);

        let render_layout = unsafe {
            context
                .device
                .create_descriptor_set_layout(&render_layout_info, None)
                .map_err(|e| format!("Failed to create render descriptor layout: {:?}", e))?
        };

        self.render_descriptor_layout = Some(render_layout);

        // Render push descriptor layout (Set 1: frame data for graphics)
        // This is similar to compute push layout but with VERTEX/FRAGMENT stages
        let render_push_bindings = [vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)];

        let render_push_layout_create_info = vk::DescriptorSetLayoutCreateInfo::default()
            .bindings(&render_push_bindings)
            .flags(vk::DescriptorSetLayoutCreateFlags::PUSH_DESCRIPTOR_KHR);

        let render_push_layout = unsafe {
            context
                .device
                .create_descriptor_set_layout(&render_push_layout_create_info, None)
                .map_err(|e| format!("Failed to create render push descriptor layout: {:?}", e))?
        };

        self.render_push_descriptor_layout = Some(render_push_layout);

        info!("Created particle system descriptor layouts");
        Ok(())
    }

    /// Create buffers for push descriptor updates.
    fn create_push_descriptor_buffers(
        &mut self,
        context: &Rc<VulkanContext>,
    ) -> Result<(), String> {
        // Frame data buffer (uniform + storage, CPU-visible)
        // Compute shaders use this as UNIFORM_BUFFER, render shaders use it as STORAGE_BUFFER
        let frame_data_size = std::mem::size_of::<FrameData>() as u64;
        let frame_buffer_info = vk::BufferCreateInfo::default()
            .size(frame_data_size)
            .usage(vk::BufferUsageFlags::UNIFORM_BUFFER | vk::BufferUsageFlags::STORAGE_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let frame_buffer = unsafe {
            context
                .device
                .create_buffer(&frame_buffer_info, None)
                .map_err(|e| format!("Failed to create frame data buffer: {:?}", e))?
        };

        let frame_requirements =
            unsafe { context.device.get_buffer_memory_requirements(frame_buffer) };

        let frame_allocation = context
            .allocator
            .borrow_mut()
            .allocate(&gpu_allocator::vulkan::AllocationCreateDesc {
                name: "particle_frame_data",
                requirements: frame_requirements,
                location: gpu_allocator::MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| format!("Failed to allocate frame data memory: {}", e))?;

        unsafe {
            context
                .device
                .bind_buffer_memory(
                    frame_buffer,
                    frame_allocation.memory(),
                    frame_allocation.offset(),
                )
                .map_err(|e| format!("Failed to bind frame data memory: {:?}", e))?
        }

        self.frame_data_buffer = Some((frame_buffer, frame_allocation));

        // Emitter configs buffer (storage, CPU-visible)
        let emitter_size = (MAX_EMITTERS as usize * std::mem::size_of::<EmitterConfig>()) as u64;
        let emitter_buffer_info = vk::BufferCreateInfo::default()
            .size(emitter_size)
            .usage(vk::BufferUsageFlags::STORAGE_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let emitter_buffer = unsafe {
            context
                .device
                .create_buffer(&emitter_buffer_info, None)
                .map_err(|e| format!("Failed to create emitter configs buffer: {:?}", e))?
        };

        let emitter_requirements = unsafe {
            context
                .device
                .get_buffer_memory_requirements(emitter_buffer)
        };

        let emitter_allocation = context
            .allocator
            .borrow_mut()
            .allocate(&gpu_allocator::vulkan::AllocationCreateDesc {
                name: "particle_emitter_configs",
                requirements: emitter_requirements,
                location: gpu_allocator::MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| format!("Failed to allocate emitter configs memory: {}", e))?;

        unsafe {
            context
                .device
                .bind_buffer_memory(
                    emitter_buffer,
                    emitter_allocation.memory(),
                    emitter_allocation.offset(),
                )
                .map_err(|e| format!("Failed to bind emitter configs memory: {:?}", e))?
        }

        self.emitter_configs_buffer = Some((emitter_buffer, emitter_allocation));

        // Validate push descriptor buffer alignments
        let device_properties = unsafe {
            context
                .instance
                .get_physical_device_properties(context.physical_device)
        };

        let min_storage_buffer_offset_alignment =
            device_properties.limits.min_storage_buffer_offset_alignment;

        // Push descriptor buffers start at offset 0, so alignment is automatic
        // Just validate buffer sizes meet minimum requirements
        let frame_data_alignment = std::mem::size_of::<FrameData>() as u64;
        let emitter_config_alignment = std::mem::size_of::<EmitterConfig>() as u64;

        if frame_data_alignment < min_storage_buffer_offset_alignment {
            log::warn!(
                "FrameData size ({}) is smaller than min_storage_buffer_offset_alignment ({}), \
                 this may cause performance issues",
                frame_data_alignment,
                min_storage_buffer_offset_alignment
            );
        }

        if emitter_config_alignment < min_storage_buffer_offset_alignment {
            log::warn!(
                "EmitterConfig size ({}) is smaller than min_storage_buffer_offset_alignment ({}), \
                 this may cause performance issues",
                emitter_config_alignment,
                min_storage_buffer_offset_alignment
            );
        }

        info!("Created particle system push descriptor buffers");
        Ok(())
    }

    /// Create descriptor pool and allocate static descriptor set for compute (Set 0).
    fn create_compute_descriptor_set(
        &mut self,
    ) -> Result<(vk::DescriptorSet, vk::DescriptorPool), String> {
        let compute_layout = self
            .compute_descriptor_layout
            .ok_or("Compute descriptor layout not created")?;

        // Create descriptor pool
        let pool_sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(5), // 5 storage buffers in Set 0
        ];

        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(1)
            .flags(vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET);

        let descriptor_pool = unsafe {
            self.context
                .device
                .create_descriptor_pool(&pool_info, None)
                .map_err(|e| format!("Failed to create descriptor pool: {:?}", e))?
        };

        // Allocate descriptor set
        let set_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(std::slice::from_ref(&compute_layout));

        let descriptor_sets = unsafe {
            self.context
                .device
                .allocate_descriptor_sets(&set_info)
                .map_err(|e| format!("Failed to allocate descriptor sets: {:?}", e))?
        };

        let descriptor_set = descriptor_sets[0];

        // Update descriptor set with buffer views
        let particle_buffer_info = [vk::DescriptorBufferInfo {
            buffer: self.buffer.particle_buffer(),
            offset: 0,
            range: (self.buffer.max_particles() as u64)
                * std::mem::size_of::<buffer::ParticleData>() as u64,
        }];

        let dead_list_info = [vk::DescriptorBufferInfo {
            buffer: self.buffer.particle_buffer(),
            offset: (self.buffer.max_particles() as u64)
                * std::mem::size_of::<buffer::ParticleData>() as u64,
            range: (self.buffer.max_particles() as u64) * std::mem::size_of::<u32>() as u64,
        }];

        // Binding 2: alive_current (read_write for simulate shader)
        // CRITICAL: This must cover BOTH regions for double-buffering (2 frames in flight)
        // Frame 0 reads from [0, MAX_PARTICLES), Frame 1 reads from [MAX_PARTICLES, 2*MAX_PARTICLES)
        let frames_in_flight = 2u64;
        let alive_current_info = [vk::DescriptorBufferInfo {
            buffer: self.buffer.particle_buffer(),
            offset: (self.buffer.max_particles() as u64)
                * (std::mem::size_of::<buffer::ParticleData>() + std::mem::size_of::<u32>()) as u64,
            range: (self.buffer.max_particles() as u64) * frames_in_flight
                * std::mem::size_of::<u32>() as u64,
        }];

        // Binding 3: alive_next (read_write for emit/simulate shaders)
        let alive_next_info = [vk::DescriptorBufferInfo {
            buffer: self.buffer.particle_buffer(),
            offset: (self.buffer.max_particles() as u64)
                * (std::mem::size_of::<buffer::ParticleData>() + 2 * std::mem::size_of::<u32>())
                    as u64,
            range: (self.buffer.max_particles() as u64) * std::mem::size_of::<u32>() as u64,
        }];

        let counters_info = [vk::DescriptorBufferInfo {
            buffer: self.buffer.counters_buffer(),
            offset: 0,
            range: std::mem::size_of::<buffer::ParticleCounters>() as u64,
        }];

        let descriptor_writes = [
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&particle_buffer_info),
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&dead_list_info),
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&alive_current_info),
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(3)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&alive_next_info),
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(4)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&counters_info),
        ];

        unsafe {
            self.context
                .device
                .update_descriptor_sets(&descriptor_writes, &[]);
        }

        // Validate descriptor set offsets for alignment
        let device_properties = unsafe {
            self.context
                .instance
                .get_physical_device_properties(self.context.physical_device)
        };

        let min_storage_buffer_offset_alignment =
            device_properties.limits.min_storage_buffer_offset_alignment;

        // Validate that all descriptor buffer offsets are properly aligned
        let particle_data_size = (self.buffer.max_particles() as u64)
            * (std::mem::size_of::<buffer::ParticleData>() as u64);
        let index_entry_size = std::mem::size_of::<u32>() as u64;

        // Check alignment for each binding
        let binding_offsets = [
            (0, 0u64),               // particle data
            (1, particle_data_size), // dead list
            (
                2,
                particle_data_size + index_entry_size * self.buffer.max_particles() as u64,
            ), // alive_current
            (
                3,
                particle_data_size + 2 * index_entry_size * self.buffer.max_particles() as u64,
            ), // alive_next
        ];

        for (binding, offset) in binding_offsets.iter() {
            if offset % min_storage_buffer_offset_alignment != 0 {
                return Err(format!(
                    "Descriptor set binding {} offset {} is not aligned to min_storage_buffer_offset_alignment ({})",
                    binding, offset, min_storage_buffer_offset_alignment
                ));
            }
        }

        info!("Created and allocated particle compute descriptor set");
        Ok((descriptor_set, descriptor_pool))
    }

    /// Create descriptor pool and allocate static descriptor set for render (Set 0).
    /// Uses VERTEX/FRAGMENT stage flags instead of COMPUTE for graphics pipeline compatibility.
    fn create_render_descriptor_set(
        &mut self,
    ) -> Result<(vk::DescriptorSet, vk::DescriptorPool), String> {
        let render_layout = self
            .render_descriptor_layout
            .ok_or("Render descriptor layout not created")?;

        // Create descriptor pool
        let pool_sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(5), // 5 storage buffers in Set 0
        ];

        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(1)
            .flags(vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET);

        let descriptor_pool = unsafe {
            self.context
                .device
                .create_descriptor_pool(&pool_info, None)
                .map_err(|e| format!("Failed to create render descriptor pool: {:?}", e))?
        };

        // Allocate descriptor set
        let set_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(std::slice::from_ref(&render_layout));

        let descriptor_sets = unsafe {
            self.context
                .device
                .allocate_descriptor_sets(&set_info)
                .map_err(|e| format!("Failed to allocate render descriptor sets: {:?}", e))?
        };

        let descriptor_set = descriptor_sets[0];

        // Update descriptor set with buffer views (same as compute, different stage flags)
        let particle_buffer_info = [vk::DescriptorBufferInfo {
            buffer: self.buffer.particle_buffer(),
            offset: 0,
            range: (self.buffer.max_particles() as u64)
                * std::mem::size_of::<buffer::ParticleData>() as u64,
        }];

        let dead_list_info = [vk::DescriptorBufferInfo {
            buffer: self.buffer.particle_buffer(),
            offset: (self.buffer.max_particles() as u64)
                * std::mem::size_of::<buffer::ParticleData>() as u64,
            range: (self.buffer.max_particles() as u64) * std::mem::size_of::<u32>() as u64,
        }];

        // Binding 2: alive_current (read for vertex shader)
        // CRITICAL: Must cover BOTH regions for double-buffering
        let frames_in_flight = 2u64;
        let alive_current_info = [vk::DescriptorBufferInfo {
            buffer: self.buffer.particle_buffer(),
            offset: (self.buffer.max_particles() as u64)
                * (std::mem::size_of::<buffer::ParticleData>() + std::mem::size_of::<u32>()) as u64,
            range: (self.buffer.max_particles() as u64) * frames_in_flight
                * std::mem::size_of::<u32>() as u64,
        }];

        // Binding 3: alive_next (unused in render, but must match layout)
        let alive_next_info = [vk::DescriptorBufferInfo {
            buffer: self.buffer.particle_buffer(),
            offset: (self.buffer.max_particles() as u64)
                * (std::mem::size_of::<buffer::ParticleData>() + 2 * std::mem::size_of::<u32>())
                    as u64,
            range: (self.buffer.max_particles() as u64) * std::mem::size_of::<u32>() as u64,
        }];

        let counters_info = [vk::DescriptorBufferInfo {
            buffer: self.buffer.counters_buffer(),
            offset: 0,
            range: std::mem::size_of::<buffer::ParticleCounters>() as u64,
        }];

        let descriptor_writes = [
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&particle_buffer_info),
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&dead_list_info),
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&alive_current_info),
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(3)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&alive_next_info),
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(4)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&counters_info),
        ];

        unsafe {
            self.context
                .device
                .update_descriptor_sets(&descriptor_writes, &[]);
        }

        info!("Created and allocated particle render descriptor set");
        Ok((descriptor_set, descriptor_pool))
    }

    /// Create emit pipeline for particle emission.
    pub fn create_emit_pipeline(
        &mut self,
        asset_registry: &mut AssetRegistry,
        shader_module: VkShaderModule,
    ) -> Result<(), String> {
        let compute_layout = self
            .compute_descriptor_layout
            .ok_or("Compute descriptor layout not created")?;

        let compute_push_layout = self
            .compute_push_descriptor_layout
            .ok_or("Compute push descriptor layout not created")?;

        let emit_pipeline = ComputePipelineBuilder::new(self.context.clone())
            .with_shader(shader_module)
            .with_descriptor_layouts(vec![
                crate::sync::VkDescriptorSetLayout(compute_layout),
                crate::sync::VkDescriptorSetLayout(compute_push_layout),
            ])
            .build()
            .map_err(|e| format!("Failed to build emit pipeline: {}", e))?;

        let pipeline_handle = asset_registry.register_compute_pipeline(emit_pipeline);
        self.emit_pipeline = Some(pipeline_handle);

        info!("Created particle emit pipeline");
        Ok(())
    }

    /// Create simulate pipeline for particle simulation.
    pub fn create_simulate_pipeline(
        &mut self,
        asset_registry: &mut AssetRegistry,
        shader_module: VkShaderModule,
    ) -> Result<(), String> {
        let compute_layout = self
            .compute_descriptor_layout
            .ok_or("Compute descriptor layout not created")?;

        let compute_push_layout = self
            .compute_push_descriptor_layout
            .ok_or("Compute push descriptor layout not created")?;

        let simulate_pipeline = ComputePipelineBuilder::new(self.context.clone())
            .with_shader(shader_module)
            .with_descriptor_layouts(vec![
                crate::sync::VkDescriptorSetLayout(compute_layout),
                crate::sync::VkDescriptorSetLayout(compute_push_layout),
            ])
            .build()
            .map_err(|e| format!("Failed to build simulate pipeline: {}", e))?;

        let pipeline_handle = asset_registry.register_compute_pipeline(simulate_pipeline);
        self.simulate_pipeline = Some(pipeline_handle);

        info!("Created particle simulate pipeline");
        Ok(())
    }

    /// Create render pipeline for particle rendering.
    ///
    /// Note: Particle rendering uses 2 descriptor sets:
    /// - Set 0: Particle buffers (particles, alive_list, etc.)
    /// - Set 1: Standard renderer storage uniforms (view/proj matrices)
    /// The render graph will bind Set 1 automatically during particle rendering.
    pub fn create_render_pipeline(
        &mut self,
        asset_registry: &mut AssetRegistry,
        vertex_shader: VkShaderModule,
        fragment_shader: VkShaderModule,
    ) -> Result<(), String> {
        use crate::pipeline::{CullMode, FrontFace};
        use crate::vulkan::material::builder::PipelineBuilder;

        let render_layout = self
            .render_descriptor_layout
            .ok_or("Render descriptor layout not created")?;

        // Create a storage descriptor layout matching the renderer's storage uniforms
        // This must match exactly what StorageDescriptorSet creates
        let storage_bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
        ];

        let storage_layout_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&storage_bindings);

        let storage_layout = unsafe {
            self.context
                .device
                .create_descriptor_set_layout(&storage_layout_info, None)
                .map_err(|e| format!("Failed to create storage descriptor layout: {:?}", e))?
        };

        let pipeline = PipelineBuilder::new(self.context.clone())
            .with_shaders(vertex_shader.vk(), fragment_shader.vk())
            .with_descriptor_layouts(vec![render_layout, storage_layout])
            // No vertex binding - particles generated from storage buffer
            .with_depth_test(false, false, crate::pipeline::CompareOp::Always)
            .with_cull_mode(CullMode::None, FrontFace::CounterClockwise)
            .with_rendering_formats(
                Some(crate::texture::ImageFormat::B8G8R8A8Srgb),
                Some(crate::texture::ImageFormat::D32SfloatS8Uint),
            );

        let pipeline = pipeline
            .build_dynamic()
            .map_err(|e| format!("Failed to build render pipeline: {}", e))?;

        let pipeline_handle = asset_registry.register_pipeline(pipeline);
        self.render_pipeline = Some(pipeline_handle);

        // Clean up the temporary layout (pipeline holds its own reference)
        unsafe {
            self.context
                .device
                .destroy_descriptor_set_layout(storage_layout, None);
        }

        info!("Created particle render pipeline");
        Ok(())
    }

    /// Get render pipeline handle.
    pub fn render_pipeline_handle(&self) -> Option<PipelineHandle> {
        self.render_pipeline
    }

    /// Record compute dispatch with timing queries.
    ///
    /// Wraps the compute dispatch with timestamp queries to measure GPU execution time.
    /// This should be used instead of `record_compute_dispatch` when timing is needed.
    pub fn record_compute_with_timing(
        &mut self,
        command_buffer: vk::CommandBuffer,
        asset_registry: &AssetRegistry,
        total_workgroups: u32,
    ) -> Result<(), String> {
        if let Some(timing) = &self.timing_queries {
            // Reset query pools for new measurements
            timing.reset(command_buffer);

            // Write start timestamp
            timing.write_start(command_buffer);
        }

        // Execute compute dispatch
        self.record_compute_dispatch(command_buffer, asset_registry, total_workgroups)?;

        if let Some(timing) = &self.timing_queries {
            // Write end timestamp
            timing.write_end(command_buffer);
        }

        // Increment dispatch counter
        self.increment_dispatch_count();

        Ok(())
    }

    /// Get compute shader execution time in milliseconds.
    ///
    /// Returns the timing data from the last compute dispatch.
    /// Returns None if timing is not available or readback failed.
    pub fn get_compute_time_ms(&mut self) -> Option<f32> {
        if let Some(timing) = &mut self.timing_queries {
            match timing.get_compute_time_ms() {
                Ok(time) => {
                    // Update statistics with the new timing data
                    self.update_compute_stats(time);
                    Some(time)
                }
                Err(e) => {
                    warn!("Failed to read compute timing: {}", e);
                    None
                }
            }
        } else {
            None
        }
    }

    /// Get cached compute time without readback.
    ///
    /// Returns the last successfully measured compute time.
    /// Returns 0.0 if no timing data is available.
    pub fn cached_compute_time_ms(&self) -> f32 {
        self.timing_queries
            .as_ref()
            .map(|t| t.cached_time_ms())
            .unwrap_or(0.0)
    }

    /// Record compute dispatch with graceful timing fallback.
    ///
    /// This method attempts to use timing queries, but falls back to non-timing
    /// dispatch if timing queries fail or are not available. This prevents
    /// GPU crashes due to timing query issues.
    ///
    /// # Arguments
    /// * `command_buffer` - Command buffer to record into
    /// * `asset_registry` - Asset registry for pipeline access
    /// * `total_workgroups` - Number of workgroups to dispatch
    ///
    /// # Returns
    /// Ok(()) if dispatch succeeded (with or without timing)
    /// Err(String) if dispatch itself failed
    pub fn record_compute_with_timing_fallback(
        &mut self,
        command_buffer: vk::CommandBuffer,
        asset_registry: &AssetRegistry,
        total_workgroups: u32,
    ) -> Result<(), String> {
        // Try timing queries if available
        if self.timing_queries.is_some() {
            // Attempt to reset timing queries
            if let Some(timing) = &self.timing_queries {
                timing.reset(command_buffer);
                timing.write_start(command_buffer);
            }

            // Execute compute dispatch
            let dispatch_result =
                self.record_compute_dispatch(command_buffer, asset_registry, total_workgroups);

            // Write end timestamp if timing was started
            if let Some(timing) = &self.timing_queries {
                timing.write_end(command_buffer);
            }

            // Increment dispatch counter
            self.increment_dispatch_count();

            // Return dispatch result (timing failures don't affect this)
            dispatch_result
        } else {
            // No timing queries available, use standard dispatch
            self.record_compute_dispatch(command_buffer, asset_registry, total_workgroups)
        }
    }

    /// Record emit pass dispatch.
    pub fn record_emit_dispatch(
        &self,
        command_buffer: vk::CommandBuffer,
        asset_registry: &AssetRegistry,
        emit_workgroups: u32,
    ) -> Result<(), String> {
        let pipeline = self.emit_pipeline.ok_or("Emit pipeline not created")?;

        let compute_pipeline = asset_registry
            .get_pipeline(pipeline)
            .ok_or("Failed to get emit pipeline from registry")?;

        let vk_pipeline = compute_pipeline.vk_pipeline();
        let vk_layout = compute_pipeline.vk_layout();

        let device = &self.context.device;

        // Bind emit pipeline
        unsafe {
            device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::COMPUTE, vk_pipeline);
        }

        // Bind static descriptor set (Set 0: particle buffers)
        if let Some(descriptor_set) = self.compute_descriptor_set {
            unsafe {
                device.cmd_bind_descriptor_sets(
                    command_buffer,
                    vk::PipelineBindPoint::COMPUTE,
                    vk_layout,
                    0, // Set 0
                    std::slice::from_ref(&descriptor_set),
                    &[],
                );
            }
        } else {
            return Err("Compute descriptor set not allocated".to_string());
        }

        // Update push descriptors (Set 1: frame data + emitter configs)
        if let Some((frame_buffer, _)) = &self.frame_data_buffer
            && let Some((emitter_buffer, _)) = &self.emitter_configs_buffer
        {
            let frame_buffer_info = [vk::DescriptorBufferInfo::default()
                .buffer(*frame_buffer)
                .offset(0)
                .range(std::mem::size_of::<FrameData>() as u64)];

            let emitter_buffer_info = [vk::DescriptorBufferInfo::default()
                .buffer(*emitter_buffer)
                .offset(0)
                .range((MAX_EMITTERS as usize * std::mem::size_of::<EmitterConfig>()) as u64)];

            let push_descriptor_writes = [
                vk::WriteDescriptorSet::default()
                    .dst_binding(0) // Binding 0 in Set 1 (frame data)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&frame_buffer_info),
                vk::WriteDescriptorSet::default()
                    .dst_binding(1) // Binding 1 in Set 1 (emitter configs)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&emitter_buffer_info),
            ];

            unsafe {
                let push_descriptor = self
                    .context
                    .push_descriptor_khr
                    .as_ref()
                    .ok_or("Push descriptor extension not available")?;

                push_descriptor.cmd_push_descriptor_set(
                    command_buffer,
                    vk::PipelineBindPoint::COMPUTE,
                    vk_layout,
                    1, // Set 1
                    &push_descriptor_writes,
                );
            }
        }

        // Dispatch emit shader
        unsafe {
            device.cmd_dispatch(command_buffer, emit_workgroups, 1, 1);
        }

        // Add pipeline barrier after EMIT pass to ensure memory synchronization before SIMULATE pass
        // EMIT pass writes to: particle buffers, alive list, counters
        // SIMULATE pass reads from: particle buffers, alive list, counters
        self.emit_barrier(command_buffer)?;

        Ok(())
    }

    /// Record simulate pass dispatch.
    pub fn record_simulate_dispatch(
        &self,
        command_buffer: vk::CommandBuffer,
        asset_registry: &AssetRegistry,
        simulate_workgroups: u32,
    ) -> Result<(), String> {
        let pipeline = self
            .simulate_pipeline
            .ok_or("Simulate pipeline not created")?;

        let compute_pipeline = asset_registry
            .get_pipeline(pipeline)
            .ok_or("Failed to get simulate pipeline from registry")?;

        let vk_pipeline = compute_pipeline.vk_pipeline();
        let vk_layout = compute_pipeline.vk_layout();

        let device = &self.context.device;

        // Bind simulate pipeline
        unsafe {
            device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::COMPUTE, vk_pipeline);
        }

        // Bind static descriptor set (Set 0: particle buffers)
        if let Some(descriptor_set) = self.compute_descriptor_set {
            unsafe {
                device.cmd_bind_descriptor_sets(
                    command_buffer,
                    vk::PipelineBindPoint::COMPUTE,
                    vk_layout,
                    0, // Set 0
                    std::slice::from_ref(&descriptor_set),
                    &[],
                );
            }
        } else {
            return Err("Compute descriptor set not allocated".to_string());
        }

        // Update push descriptors (Set 1: frame data only - no emitter configs needed)
        if let Some((frame_buffer, _)) = &self.frame_data_buffer {
            let frame_buffer_info = [vk::DescriptorBufferInfo::default()
                .buffer(*frame_buffer)
                .offset(0)
                .range(std::mem::size_of::<FrameData>() as u64)];

            let push_descriptor_writes = [vk::WriteDescriptorSet::default()
                .dst_binding(0) // Binding 0 in Set 1 (frame data)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .buffer_info(&frame_buffer_info)];

            unsafe {
                let push_descriptor = self
                    .context
                    .push_descriptor_khr
                    .as_ref()
                    .ok_or("Push descriptor extension not available")?;

                push_descriptor.cmd_push_descriptor_set(
                    command_buffer,
                    vk::PipelineBindPoint::COMPUTE,
                    vk_layout,
                    1, // Set 1
                    &push_descriptor_writes,
                );
            }
        }

        // Dispatch simulate shader
        unsafe {
            device.cmd_dispatch(command_buffer, simulate_workgroups, 1, 1);
        }

        // Add pipeline barrier after SIMULATE pass to ensure memory synchronization before RENDER pass
        // SIMULATE pass writes to: particle buffers, alive list
        // RENDER pass reads from: particle buffers (for vertex attributes), alive list (for indirect drawing)
        self.simulate_barrier(command_buffer)?;

        Ok(())
    }

    /// Pipeline barrier after EMIT pass → before SIMULATE pass.
    ///
    /// Ensures memory synchronization between compute passes:
    /// - EMIT writes to particle buffers, alive lists, and counters
    /// - SIMULATE reads these buffers to update particle state
    ///
    /// Barrier details:
    /// - src_stage: COMPUTE_SHADER (EMIT pass)
    /// - dst_stage: COMPUTE_SHADER (SIMULATE pass)
    /// - src_access: SHADER_WRITE (EMIT wrote to buffers)
    /// - dst_access: SHADER_READ | SHADER_WRITE (SIMULATE reads and writes)
    fn emit_barrier(&self, command_buffer: vk::CommandBuffer) -> Result<(), String> {
        let particle_buffer = self.buffer.particle_buffer();
        let counters_buffer = self.buffer.counters_buffer();
        let device = &self.context.device;

        // Calculate buffer sizes - cover entire particle buffer
        let particle_data_size = (self.buffer.max_particles() as u64)
            * (std::mem::size_of::<buffer::ParticleData>() as u64);
        let index_list_size =
            (self.buffer.max_particles() as u64) * (std::mem::size_of::<u32>() as u64);
        let total_buffer_size = particle_data_size + (3 * index_list_size); // particles + dead + alive_current + alive_next

        let counters_size = std::mem::size_of::<buffer::ParticleCounters>() as u64;

        // Create buffer memory barrier for entire particle buffer
        // EMIT pass writes to particle data, dead list, and alive_current
        let particle_barrier = BufferMemoryBarrier2 {
            src_stage_mask: PipelineStage2Flags::COMPUTE_SHADER,
            dst_stage_mask: PipelineStage2Flags::COMPUTE_SHADER,
            src_access_mask: AccessFlags2::SHADER_WRITE,
            dst_access_mask: AccessFlags2::SHADER_READ | AccessFlags2::SHADER_WRITE,
            src_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            dst_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            buffer: VkBuffer::new(particle_buffer),
            offset: 0,
            size: total_buffer_size, // Cover entire buffer
        };

        // Create buffer memory barrier for counters buffer
        // EMIT pass reads/writes atomic counters
        let counters_barrier = BufferMemoryBarrier2 {
            src_stage_mask: PipelineStage2Flags::COMPUTE_SHADER,
            dst_stage_mask: PipelineStage2Flags::COMPUTE_SHADER,
            src_access_mask: AccessFlags2::SHADER_READ | AccessFlags2::SHADER_WRITE,
            dst_access_mask: AccessFlags2::SHADER_READ | AccessFlags2::SHADER_WRITE,
            src_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            dst_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            buffer: VkBuffer::new(counters_buffer),
            offset: 0,
            size: counters_size,
        };

        // Build and execute dependency info
        let dep_info = DependencyInfo::new()
            .add_buffer_barrier2(particle_barrier)
            .add_buffer_barrier2(counters_barrier);

        dep_info.build(|dep_info| unsafe {
            device.cmd_pipeline_barrier2(command_buffer, dep_info);
        });

        Ok(())
    }

    /// Pipeline barrier after SIMULATE pass → before RENDER pass.
    ///
    /// Ensures memory synchronization between compute and graphics:
    /// - SIMULATE writes to particle buffers and alive list
    /// - RENDER reads these buffers for vertex attributes and drawing
    ///
    /// Barrier details:
    /// - src_stage: COMPUTE_SHADER (SIMULATE pass)
    /// - dst_stage: VERTEX_SHADER (RENDER pass)
    /// - src_access: SHADER_WRITE (SIMULATE wrote to buffers)
    /// - dst_access: SHADER_READ (RENDER reads via storage buffer)
    fn simulate_barrier(&self, command_buffer: vk::CommandBuffer) -> Result<(), String> {
        let particle_buffer = self.buffer.particle_buffer();
        let device = &self.context.device;

        // Calculate buffer size for the entire particle buffer
        let particle_buffer_size = (self.buffer.max_particles() as u64)
            * (std::mem::size_of::<buffer::ParticleData>() as u64
                + 3 * std::mem::size_of::<u32>() as u64); // particles + dead + alive_current + alive_next

        // Create buffer memory barrier for particle buffer (including alive list)
        // This barrier ensures that:
        // 1. Particle data written by compute is visible to vertex shader
        // 2. Alive list written by compute is visible to vertex shader for indirect drawing
        //
        // NOTE: We use SHADER_READ instead of VERTEX_ATTRIBUTE_READ because the particle
        // render shader accesses particle data via storage buffer binding, not vertex attributes.
        // VERTEX_ATTRIBUTE_READ is only valid for VERTEX_INPUT stage, not VERTEX_SHADER.
        let particle_barrier = BufferMemoryBarrier2 {
            src_stage_mask: PipelineStage2Flags::COMPUTE_SHADER,
            dst_stage_mask: PipelineStage2Flags::VERTEX_SHADER,
            src_access_mask: AccessFlags2::SHADER_WRITE,
            dst_access_mask: AccessFlags2::SHADER_READ,
            src_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            dst_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            buffer: VkBuffer::new(particle_buffer),
            offset: 0,
            size: particle_buffer_size,
        };

        // Build and execute dependency info
        let dep_info = DependencyInfo::new().add_buffer_barrier2(particle_barrier);

        dep_info.build(|dep_info| unsafe {
            device.cmd_pipeline_barrier2(command_buffer, dep_info);
        });

        Ok(())
    }

    ///
    /// This copies alive_next (written by simulate) to alive_current (read by emit).
    /// Must be called AFTER simulate pass, BEFORE next emit pass.
    ///
    /// # Arguments
    /// * `command_buffer` - Command buffer to record the swap into
    /// * `frame_idx` - Current frame index (for per-frame buffer offsets)
    ///
    /// # Returns
    /// Ok(()) if swap succeeded, Err otherwise
    pub fn swap_alive_lists(
        &self,
        command_buffer: vk::CommandBuffer,
        frame_idx: usize,
    ) -> Result<(), String> {
        self.buffer.swap_alive_lists(command_buffer, frame_idx)
    }

    /// Record compute dispatch commands (legacy method for compatibility).
    pub fn record_compute_dispatch(
        &self,
        command_buffer: vk::CommandBuffer,
        asset_registry: &AssetRegistry,
        total_workgroups: u32,
    ) -> Result<(), String> {
        let pipeline = self
            .simulate_pipeline
            .ok_or("Simulate pipeline not created")?;

        let compute_pipeline = asset_registry
            .get_pipeline(pipeline)
            .ok_or("Failed to get compute pipeline from registry")?;

        let vk_pipeline = compute_pipeline.vk_pipeline();
        let vk_layout = compute_pipeline.vk_layout();

        let device = &self.context.device;

        // Bind compute pipeline
        unsafe {
            device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::COMPUTE, vk_pipeline);
        }

        // Bind static descriptor set (Set 0: particle buffers)
        if let Some(descriptor_set) = self.compute_descriptor_set {
            unsafe {
                device.cmd_bind_descriptor_sets(
                    command_buffer,
                    vk::PipelineBindPoint::COMPUTE,
                    vk_layout,
                    0, // Set 0
                    std::slice::from_ref(&descriptor_set),
                    &[],
                );
            }
        } else {
            return Err("Compute descriptor set not allocated".to_string());
        }

        // Update push descriptors (Set 1: frame data + emitter configs)
        if let Some((frame_buffer, _)) = &self.frame_data_buffer
            && let Some((emitter_buffer, _)) = &self.emitter_configs_buffer
        {
            let frame_buffer_info = [vk::DescriptorBufferInfo::default()
                .buffer(*frame_buffer)
                .offset(0)
                .range(std::mem::size_of::<FrameData>() as u64)];

            let emitter_buffer_info = [vk::DescriptorBufferInfo::default()
                .buffer(*emitter_buffer)
                .offset(0)
                .range((MAX_EMITTERS as usize * std::mem::size_of::<EmitterConfig>()) as u64)];

            let push_descriptor_writes = [
                vk::WriteDescriptorSet::default()
                    .dst_binding(0) // Binding 0 in Set 1 (frame data)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&frame_buffer_info),
                vk::WriteDescriptorSet::default()
                    .dst_binding(1) // Binding 1 in Set 1 (emitter configs)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&emitter_buffer_info),
            ];

            unsafe {
                // Use push descriptors (no allocation, writes directly to command buffer)
                let push_descriptor = self
                    .context
                    .push_descriptor_khr
                    .as_ref()
                    .ok_or("Push descriptor extension not available")?;

                push_descriptor.cmd_push_descriptor_set(
                    command_buffer,
                    vk::PipelineBindPoint::COMPUTE,
                    vk_layout,
                    1, // Set 1
                    &push_descriptor_writes,
                );
            }
        }

        // Dispatch compute shader
        unsafe {
            device.cmd_dispatch(command_buffer, total_workgroups, 1, 1);
        }

        Ok(())
    }

    /// Save an emitter configuration as a preset.
    ///
    /// # Arguments
    /// * `name` - Preset name (will be saved as name.json)
    /// * `config` - Emitter configuration to save
    ///
    /// # Errors
    /// Returns error if file write or serialization fails
    pub fn save_preset(&self, name: &str, config: &EmitterConfig) -> Result<(), String> {
        let presets_dir = std::path::Path::new("assets/particles");
        let preset = EmitterPreset::new(name.to_string(), *config);
        let path = presets_dir.join(format!("{}.json", name));

        preset.save_to_file(&path)
    }

    /// Load an emitter configuration from a preset.
    ///
    /// # Arguments
    /// * `name` - Preset name (filename without .json extension)
    ///
    /// # Errors
    /// Returns error if preset file not found or deserialization fails
    pub fn load_preset(&self, name: &str) -> Result<EmitterConfig, String> {
        let presets_dir = std::path::Path::new("assets/particles");
        let path = presets_dir.join(format!("{}.json", name));

        if !path.exists() {
            return Err(format!("Preset '{}' not found at {}", name, path.display()));
        }

        let preset = EmitterPreset::load_from_file(&path)?;
        Ok(preset.config)
    }

    /// Get list of available preset names.
    ///
    /// Scans the assets/particles/ directory for .json files.
    ///
    /// # Returns
    /// Vector of preset names (filenames without .json extension)
    pub fn get_available_presets(&self) -> Vec<String> {
        let presets_dir = std::path::Path::new("assets/particles");
        let mut presets = Vec::new();

        if let Ok(entries) = std::fs::read_dir(presets_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();

                if path.extension().and_then(|s| s.to_str()) == Some("json")
                    && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                {
                    presets.push(stem.to_string());
                }
            }
        }

        presets.sort();
        presets
    }

    /// Load all presets from the assets/particles/ directory.
    ///
    /// This is a convenience method that loads all available presets.
    /// Presets can then be accessed by name via `load_preset`.
    ///
    /// # Errors
    /// Returns error if presets directory doesn't exist or isn't readable
    pub fn load_all_presets(&self) -> Result<(), String> {
        let presets_dir = std::path::Path::new("assets/particles");

        if !presets_dir.exists() {
            std::fs::create_dir_all(presets_dir).map_err(|e| {
                format!(
                    "Failed to create presets directory {}: {}",
                    presets_dir.display(),
                    e
                )
            })?;
        }

        let presets = self.get_available_presets();
        log::info!(
            "Found {} particle presets in {}",
            presets.len(),
            presets_dir.display()
        );

        Ok(())
    }

    /// Get comprehensive particle system statistics.
    ///
    /// Returns a snapshot of current system state including particle counts,
    /// performance metrics, and memory usage.
    pub fn get_stats(&self) -> ParticleStats {
        ParticleStats {
            max_alive_count: self.max_particles,
            current_alive_count: self.alive_count(),
            dead_count: self.max_particles - self.alive_count(),
            total_emitted: self.total_emitted,
            total_died: self.total_died,
            compute_time_ms: self.cached_compute_time_ms(),
            avg_compute_time_ms: self.avg_compute_time,
            peak_compute_time_ms: self.peak_compute_time,
            emitter_counts: self.calculate_emitter_counts(),
            memory_used_mb: self.calculate_memory_usage(),
            buffer_utilization: if self.max_particles > 0 {
                self.alive_count() as f32 / self.max_particles as f32
            } else {
                0.0
            },
            frame_count: self.frame_count as u64,
            total_dispatches: self.total_dispatches,
        }
    }

    /// Calculate particle counts per emitter.
    ///
    /// Note: This is a simplified implementation that returns zeros for all emitters.
    /// A full implementation would require GPU readback of per-emitter counters.
    fn calculate_emitter_counts(&self) -> Vec<u32> {
        // Simplified implementation - returns zeros for all active emitters
        // A full implementation would require GPU readback or atomic counters per emitter
        self.emitters
            .iter()
            .filter(|e| e.emit_rate > 0.0)
            .map(|_| 0)
            .collect()
    }

    /// Calculate total GPU memory usage in megabytes.
    fn calculate_memory_usage(&self) -> f32 {
        // Particle data: 48 bytes per particle
        let particle_data_mb = (self.max_particles as f32) * 48.0 / (1024.0 * 1024.0);

        // Index lists: dead + alive_current + alive_next (4 bytes per particle per list)
        let index_lists_mb = (self.max_particles as f32) * 12.0 / (1024.0 * 1024.0);

        // Counters: 32 bytes
        let counters_mb = 32.0 / (1024.0 * 1024.0);

        // Emitter configs: 80 bytes per emitter
        let configs_mb = (self.emitters.len() as f32) * 80.0 / (1024.0 * 1024.0);

        particle_data_mb + index_lists_mb + counters_mb + configs_mb
    }

    /// Update compute statistics with new timing data.
    ///
    /// Call this after each compute dispatch to update rolling averages and peak values.
    ///
    /// # Arguments
    /// * `compute_time_ms` - Compute shader execution time in milliseconds
    pub fn update_compute_stats(&mut self, compute_time_ms: f32) {
        // Update history for rolling average
        self.compute_time_history.push(compute_time_ms);
        if self.compute_time_history.len() > 60 {
            self.compute_time_history.remove(0);
        }

        // Calculate rolling average
        if !self.compute_time_history.is_empty() {
            self.avg_compute_time = self.compute_time_history.iter().sum::<f32>()
                / self.compute_time_history.len() as f32;
        }

        // Update peak
        if compute_time_ms > self.peak_compute_time {
            self.peak_compute_time = compute_time_ms;
        }
    }

    /// Update emission statistics.
    ///
    /// Call this after emitting particles to track lifetime statistics.
    ///
    /// # Arguments
    /// * `emitted_count` - Number of particles emitted this frame
    pub fn update_emission_stats(&mut self, emitted_count: u32) {
        self.total_emitted += emitted_count as u64;
    }

    /// Update death statistics.
    ///
    /// Call this after particle simulation to track particles that died this frame.
    ///
    /// # Arguments
    /// * `died_count` - Number of particles that died this frame
    pub fn update_death_stats(&mut self, died_count: u32) {
        self.total_died += died_count as u64;
    }

    /// Increment dispatch counter.
    ///
    /// Call this after each compute dispatch to track total dispatches.
    pub fn increment_dispatch_count(&mut self) {
        self.total_dispatches += 1;
    }

    /// Get maximum particle capacity.
    pub fn max_particles(&self) -> u32 {
        self.max_particles
    }
}

impl Drop for GlobalParticleSystem {
    fn drop(&mut self) {
        // Ensure resources are cleaned up if destroy() wasn't called explicitly
        if !self.destroyed {
            // Call destroy() but suppress any errors since we're in Drop
            self.destroy();
        }
        // Descriptor pool is already destroyed in destroy() method
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emitter_config_size() {
        // Ensure config fits in uniform buffer and is properly aligned
        assert_eq!(std::mem::size_of::<EmitterConfig>(), 96);
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
        assert_eq!(config.get_shape(), EmitterShape::Point);
        assert_eq!(config.shape_params, [0.0; 4]);
    }

    #[test]
    fn test_emitter_shape_point() {
        let mut config = EmitterConfig::default();
        config.set_shape(EmitterShape::Point);
        assert_eq!(config.get_shape(), EmitterShape::Point);
    }

    #[test]
    fn test_emitter_shape_line() {
        let mut config = EmitterConfig::default();
        config.set_shape(EmitterShape::Line);
        config.shape_params = [10.0, 0.0, 0.0, 0.0];
        assert_eq!(config.get_shape(), EmitterShape::Line);
        assert_eq!(config.shape_params[0], 10.0);
    }

    #[test]
    fn test_emitter_shape_circle() {
        let mut config = EmitterConfig::default();
        config.set_shape(EmitterShape::Circle);
        config.shape_params = [5.0, 0.0, 0.0, 0.0];
        assert_eq!(config.get_shape(), EmitterShape::Circle);
        assert_eq!(config.shape_params[0], 5.0);
    }

    #[test]
    fn test_emitter_shape_sphere() {
        let mut config = EmitterConfig::default();
        config.set_shape(EmitterShape::Sphere);
        config.shape_params = [3.0, 0.0, 0.0, 0.0];
        assert_eq!(config.get_shape(), EmitterShape::Sphere);
        assert_eq!(config.shape_params[0], 3.0);
    }

    #[test]
    fn test_emitter_shape_box() {
        let mut config = EmitterConfig::default();
        config.set_shape(EmitterShape::Box);
        config.shape_params = [4.0, 3.0, 2.0, 0.0];
        assert_eq!(config.get_shape(), EmitterShape::Box);
        assert_eq!(config.shape_params[0], 4.0);
        assert_eq!(config.shape_params[1], 3.0);
        assert_eq!(config.shape_params[2], 2.0);
    }

    #[test]
    fn test_emitter_shape_serialization() {
        let config = EmitterConfig {
            position: [1.0, 2.0, 3.0],
            shape: EmitterShape::Sphere.as_u32(),
            emit_rate: 100.0,
            base_lifetime: 2.0,
            lifetime_variation: 0.5,
            velocity_direction: [0.0, 1.0, 0.0],
            velocity_magnitude: 5.0,
            velocity_cone_angle: 0.3,
            base_scale: 0.2,
            scale_variation: 0.3,
            color: [1.0, 0.5, 0.0, 1.0],
            color_variation: 0.2,
            shape_params: [2.5, 0.0, 0.0, 0.0],
            _pad0: 0.0,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: EmitterConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.get_shape(), EmitterShape::Sphere);
        assert_eq!(deserialized.shape_params[0], 2.5);
        assert_eq!(deserialized.position, [1.0, 2.0, 3.0]);
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
            config.set_shape(shape);
            assert_eq!(config.get_shape(), shape);
        }
    }
}
