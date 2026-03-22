//! GPU-driven particle system using a single global buffer with atomic counters,
//! index list management, and indirect drawing.

pub mod buffer;
pub mod debug_readback;
pub mod presets;
pub mod stats;
pub mod timing;
pub mod validation;

pub use buffer::{FrameData, GlobalParticleBuffer, ParticleCounters, ParticleData};
pub use debug_readback::{ParticleDebugData, ParticleDebugReadback};
pub use presets::EmitterPreset;
pub use stats::ParticleStats;
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

/// Workgroup size for particle emit compute shader (must match @workgroup_size in particle_emit.wgsl)
pub const PARTICLE_EMIT_WORKGROUP_SIZE: u32 = 256;

/// Workgroup size for particle simulate compute shader (must match @workgroup_size in particle_simulate.wgsl)
pub const PARTICLE_SIMULATE_WORKGROUP_SIZE: u32 = 64;

/// Per-emitter configuration uploaded to a GPU storage buffer.
///
/// Must match WGSL `EmitterConfig` exactly. WGSL `vec3f` has 16-byte alignment
/// while Rust `[f32; 3]` has 4-byte alignment in `repr(C)`, so explicit padding
/// fields bridge the gap.
#[repr(C)]
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct EmitterConfig {
    #[serde(default = "default_position")]
    pub position: [f32; 3],

    #[serde(skip)]
    pub _pad_position: f32,

    #[serde(default)]
    pub shape: u32,

    #[serde(default = "default_emit_rate")]
    pub emit_rate: f32,

    #[serde(default = "default_base_lifetime")]
    pub base_lifetime: f32,

    /// Random variation in lifetime (±percentage)
    #[serde(default = "default_lifetime_variation")]
    pub lifetime_variation: f32,

    #[serde(default = "default_velocity_direction")]
    pub velocity_direction: [f32; 3],

    #[serde(skip)]
    pub _pad_velocity: f32,

    #[serde(default = "default_velocity_magnitude")]
    pub velocity_magnitude: f32,

    /// Velocity spread cone angle (0 = straight, PI/2 = hemisphere)
    #[serde(default = "default_velocity_cone_angle")]
    pub velocity_cone_angle: f32,

    #[serde(default = "default_base_scale")]
    pub base_scale: f32,

    /// Scale variation (±percentage)
    #[serde(default = "default_scale_variation")]
    pub scale_variation: f32,

    #[serde(default = "default_color")]
    pub color: [f32; 4],

    /// Color variation (±percentage per channel)
    #[serde(default = "default_color_variation")]
    pub color_variation: f32,

    #[serde(skip)]
    pub _pad_color: Align16Vec4,

    /// Shape parameters (length/radius for Line/Circle/Sphere, dimensions for Box)
    #[serde(default)]
    pub shape_params: [f32; 4],

    /// Gravity acceleration applied each frame (negative = downward, 0 = none, positive = upward)
    #[serde(default)]
    pub gravity: f32,

    /// Turbulence strength (amplitude of sinusoidal force applied perpendicular to velocity)
    #[serde(default)]
    pub turbulence_strength: f32,

    /// Turbulence frequency (how fast the sine wave oscillates)
    #[serde(default = "default_turbulence_frequency")]
    pub turbulence_frequency: f32,
}

/// 16-byte aligned `[f32; 4]` to match WGSL `vec4f` alignment.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Default)]
pub struct Align16Vec4(pub [f32; 4]);

// Safety: Align16Vec4 is repr(C) with align(16), contains only f32 (Pod).
unsafe impl bytemuck::Pod for Align16Vec4 {}
unsafe impl bytemuck::Zeroable for Align16Vec4 {}

// Safety: EmitterConfig is repr(C), all fields are Pod or padding from Align16Vec4 alignment.
// The 12 bytes of padding between color_variation and _pad_color are never read uninitialized
// because the struct is always created via Default or explicit field init.
unsafe impl bytemuck::Pod for EmitterConfig {}
unsafe impl bytemuck::Zeroable for EmitterConfig {}

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
fn default_turbulence_frequency() -> f32 {
    3.0
}

impl Default for EmitterConfig {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            _pad_position: 0.0,
            shape: EmitterShape::Point.as_u32(),
            emit_rate: 50.0,
            base_lifetime: 5.0,
            lifetime_variation: 0.2,
            velocity_direction: [0.0, 1.0, 0.0],
            _pad_velocity: 0.0,
            velocity_magnitude: 1.0,
            velocity_cone_angle: 0.5,
            base_scale: 0.1,
            scale_variation: 0.5,
            color: [1.0, 1.0, 1.0, 1.0],
            color_variation: 0.1,
            _pad_color: Align16Vec4([0.0; 4]),
            shape_params: [0.0; 4],
            gravity: -9.8,
            turbulence_strength: 0.0,
            turbulence_frequency: 3.0,
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
    /// Accumulated fractional emit time for rate-based emission
    emit_accumulator: f32,
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

    /// Recycled emitter slots available for reuse
    free_emitter_slots: Vec<u32>,

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

    /// Maximum particles in the system
    max_particles: u32,

    /// Total particles emitted since system start
    total_emitted: u64,

    /// Debug readback helper (optional, created only when debugging)
    debug_readback: Option<ParticleDebugReadback>,
}

impl GlobalParticleSystem {
    /// Create a new global particle system.
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
            free_emitter_slots: Vec::new(),
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
            max_particles,
            total_emitted: 0,
            debug_readback: None,
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

        info!("Modern particle system initialized successfully");
        Ok(system)
    }

    /// Create a new particle emitter. Lightweight — just allocates a config slot.
    pub fn create_emitter(&mut self, config: EmitterConfig) -> Result<EmitterHandle, String> {
        if self.emitters.len() >= MAX_EMITTERS as usize {
            log::warn!(
                "Cannot create emitter: maximum emitter count ({}) reached",
                MAX_EMITTERS
            );
            return Err(format!("Maximum emitter count ({}) reached", MAX_EMITTERS));
        }

        let index = self
            .free_emitter_slots
            .pop()
            .unwrap_or(self.next_emitter_slot);
        if index >= self.next_emitter_slot {
            self.next_emitter_slot = index + 1;
        }

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

        // Explicitly initialize emitter state to ensure clean state
        self.emitter_states[index as usize] = EmitterState::default();

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

    /// Burst particles from an emitter immediately (overrides emit rate for this frame).
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
            self.free_emitter_slots.push(handle.index());
            info!("Destroyed particle emitter {}", handle.index());
        }
    }

    /// Update particle simulation and emit new particles. Call once per frame before rendering.
    pub fn update(&mut self, delta_time: f32, frame_index: u32) -> Result<(u32, u32), String> {
        self.frame_count += 1;

        // Upload emitter configs to GPU buffer
        self.upload_emitter_configs()?;

        // Calculate total particles to emit this frame (including bursts)
        // Use calculate_emit_count to get proper rate-based emission with accumulators
        let total_emit_count = self.calculate_emit_count(delta_time);

        let total_burst_count: u32 = self
            .emitter_states
            .iter()
            .map(|state| state.burst_count)
            .sum();

        let total_this_frame = total_emit_count + total_burst_count;

        log::debug!(
            "Particle emit: rate={} burst={} total={}",
            total_emit_count,
            total_burst_count,
            total_this_frame
        );

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

        let emit_count = total_emit_count + total_burst_count;

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

        // Track total emitted
        if total_this_frame > 0 {
            self.total_emitted += total_this_frame as u64;
        }

        Ok((alive_count, emit_count))
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
                let active_emitter_count = self
                    .emitters
                    .iter()
                    .zip(self.emitter_states.iter())
                    .filter(|(e, s)| e.emit_rate > 0.0 || s.burst_count > 0)
                    .count() as u32;

                // total_simulate_count = previous survivors + newly emitted particles
                let total_simulate_count = self.cached_alive_count + emit_count + burst_count;

                let frame_data = FrameData {
                    delta_time,
                    total_emit_count: emit_count + burst_count,
                    emitter_count: active_emitter_count,
                    random_seed: self.frame_count,
                    total_simulate_count,
                    burst_count,
                    frame_index,
                    _pad: 0,
                };

                log::debug!(
                    "FrameData {}: dt={:.6} emit={} burst={} cached_alive={} sim={} emitters={}",
                    frame_index,
                    frame_data.delta_time,
                    frame_data.total_emit_count,
                    frame_data.burst_count,
                    self.cached_alive_count,
                    frame_data.total_simulate_count,
                    frame_data.emitter_count
                );

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

    /// Render particles using GPU-driven indirect draw.
    pub fn render(
        &mut self,
        command_buffer: vk::CommandBuffer,
        _render_pass: vk::RenderPass,
        pipeline: vk::Pipeline,
        layout: vk::PipelineLayout,
        storage_descriptor_set: vk::DescriptorSet,
        frame_index: usize,
    ) -> Result<(), String> {
        let device = &self.context.device;

        // Bind graphics pipeline
        unsafe {
            device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::GRAPHICS, pipeline);
        }

        // Update alive descriptor binding offset for this frame.
        // Render reads from the simulate output region: alive[(frame_index+1)%2].
        self.update_alive_descriptor_binding(frame_index)?;

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

        // Draw particles using GPU-driven indirect draw.
        // The simulate shader writes VkDrawIndirectCommand with vertex_count = alive_count * 6.
        if self.cached_alive_count > 0 {
            unsafe {
                device.cmd_draw_indirect(
                    command_buffer,
                    self.buffer.indirect_draw_buffer(),
                    0,  // offset into indirect buffer
                    1,  // draw count (one VkDrawIndirectCommand)
                    16, // stride between commands (sizeof(VkDrawIndirectCommand))
                );
            }
        }

        Ok(())
    }

    /// Update alive_list descriptor binding offsets for compute shaders (call before each dispatch).
    ///
    /// Binding 2 (alive_list/read): points to alive[frame_index] — emit reads survivors and appends,
    /// simulate reads the full list (survivors + emitted).
    /// Binding 3 (alive_list_next/write): points to alive[(frame_index+1)%2] — simulate writes
    /// survivors here, which render will then read from.
    pub fn update_compute_descriptor_binding(&self, frame_index: usize) -> Result<(), String> {
        let device = &self.context.device;
        let descriptor_set = self
            .compute_descriptor_set
            .ok_or("Compute descriptor set not allocated")?;

        let layout = self.buffer.layout();
        let next_frame = (frame_index + 1) % 2;

        let alive_read_offset = layout.alive_frame_offset[frame_index];
        let alive_write_offset = layout.alive_frame_offset[next_frame];
        let alive_list_region_size = layout.alive_list_size;

        let alive_list_info = [vk::DescriptorBufferInfo {
            buffer: self.buffer.particle_buffer(),
            offset: alive_read_offset,
            range: alive_list_region_size,
        }];

        let alive_next_info = [vk::DescriptorBufferInfo {
            buffer: self.buffer.particle_buffer(),
            offset: alive_write_offset,
            range: alive_list_region_size,
        }];

        let descriptor_writes = [
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&alive_list_info),
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(3)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&alive_next_info),
        ];

        unsafe {
            device.update_descriptor_sets(&descriptor_writes, &[]);
        }

        Ok(())
    }

    /// Update alive_list descriptor binding offset for render shaders (call each frame before rendering).
    ///
    /// Binds to alive[(frame_index+1)%2] (written by the current frame's simulate pass).
    /// The indirect draw command's vertex_count reflects the survivor count written there.
    fn update_alive_descriptor_binding(&self, frame_index: usize) -> Result<(), String> {
        let device = &self.context.device;
        let descriptor_set = self
            .render_descriptor_set
            .ok_or("Render descriptor set not allocated")?;

        let layout = self.buffer.layout();
        let next_frame = (frame_index + 1) % 2;

        let alive_list_info = [vk::DescriptorBufferInfo {
            buffer: self.buffer.particle_buffer(),
            offset: layout.alive_frame_offset[next_frame],
            range: layout.alive_list_size,
        }];

        let descriptor_write = vk::WriteDescriptorSet::default()
            .dst_set(descriptor_set)
            .dst_binding(2)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .buffer_info(&alive_list_info);

        unsafe {
            device.update_descriptor_sets(std::slice::from_ref(&descriptor_write), &[]);
        }

        Ok(())
    }

    /// Get current alive particle count.
    ///
    /// Returns the cached value last set by `update()` or `set_alive_count()`.
    /// For reliable GPU-read counts, use debug readback + `set_alive_count()`.
    pub fn alive_count(&self) -> u32 {
        self.cached_alive_count
    }

    /// Set cached alive count from external readback (e.g., debug readback via `vkCmdCopyBuffer`).
    pub fn set_alive_count(&mut self, count: u32) {
        self.cached_alive_count = count;
    }

    /// Get emitter configurations (for compute dispatch).
    pub fn get_emitters(&self) -> &[EmitterConfig] {
        &self.emitters
    }

    /// Calculate particles to emit this frame using fractional accumulation across frames.
    pub fn calculate_emit_count(&mut self, delta_time: f32) -> u32 {
        let mut total_emit = 0u32;

        for (emitter, state) in self.emitters.iter().zip(self.emitter_states.iter_mut()) {
            if emitter.emit_rate > 0.0 {
                // Accumulate fractional particles
                state.emit_accumulator += emitter.emit_rate * delta_time;

                // Extract whole particles to emit this frame
                let to_emit = state.emit_accumulator as u32;
                state.emit_accumulator -= to_emit as f32;

                total_emit += to_emit;
            }
        }

        total_emit
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
        info!("  destroying push descriptor buffers");
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

        info!("  destroying global particle buffer");
        self.buffer.destroy();

        // Destroy descriptor set layouts (we own these, pipelines just reference them)
        info!("  destroying descriptor set layouts");
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
        self.free_emitter_slots.clear();

        // Destroy descriptor pools
        info!("  destroying descriptor pools");
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

        // Destroy debug readback if present
        info!("  destroying debug readback");
        if let Some(mut readback) = self.debug_readback.take() {
            readback.destroy();
        }
        info!("  particle system destroy done");
    }

    /// Initialize debug readback for particle data inspection (staging buffer GPU→CPU copies).
    pub fn init_debug_readback(&mut self) -> Result<(), String> {
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

    /// Record copy commands for debug readback. Submit and wait on GPU fence before calling `read_debug_data()`.
    pub fn record_debug_readback(
        &mut self,
        command_buffer: vk::CommandBuffer,
        frame_index: usize,
    ) -> Result<(), String> {
        if let Some(ref mut readback) = self.debug_readback {
            readback.record_copy(command_buffer, &self.buffer, frame_index)?;
            Ok(())
        } else {
            Err("Debug readback not initialized. Call init_debug_readback() first.".to_string())
        }
    }

    /// Read debug data from staging buffers. Must be called after GPU fence wait.
    pub fn read_debug_data(&self) -> Result<ParticleDebugData, String> {
        if let Some(ref readback) = self.debug_readback {
            readback.read(&self.buffer)
        } else {
            Err("Debug readback not initialized. Call init_debug_readback() first.".to_string())
        }
    }

    /// Check if debug readback is initialized.
    pub fn has_debug_readback(&self) -> bool {
        self.debug_readback.is_some()
    }

    /// Destroy debug readback to free staging buffers.
    pub fn destroy_debug_readback(&mut self) {
        if let Some(mut readback) = self.debug_readback.take() {
            readback.destroy();
            info!("Particle debug readback destroyed");
        }
    }

    /// Create descriptor set layouts for particle system.
    fn create_descriptor_layouts(&mut self, context: &Rc<VulkanContext>) -> Result<(), String> {
        // Compute layout (Set 0: static buffers only - particles, dead list, alive lists, counters, indirect draw)
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
            vk::DescriptorSetLayoutBinding::default()
                .binding(5)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];

        // Enable UPDATE_AFTER_BIND for all bindings to allow per-frame descriptor updates
        // without causing validation errors when command buffers are still pending
        let compute_binding_flags = [
            vk::DescriptorBindingFlags::UPDATE_AFTER_BIND, // Binding 0: particles
            vk::DescriptorBindingFlags::UPDATE_AFTER_BIND, // Binding 1: dead_list
            vk::DescriptorBindingFlags::UPDATE_AFTER_BIND, // Binding 2: alive_list (critical!)
            vk::DescriptorBindingFlags::UPDATE_AFTER_BIND, // Binding 3: alive write target
            vk::DescriptorBindingFlags::UPDATE_AFTER_BIND, // Binding 4: counters
            vk::DescriptorBindingFlags::UPDATE_AFTER_BIND, // Binding 5: indirect draw command
        ];

        let mut compute_binding_flags_info =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::default()
                .binding_flags(&compute_binding_flags);

        let compute_layout_info = vk::DescriptorSetLayoutCreateInfo::default()
            .bindings(&compute_bindings)
            .flags(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL)
            .push_next(&mut compute_binding_flags_info);

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
        // The render shader uses binding 0 (particles) and binding 2 (alive list from simulate)
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

        // Enable UPDATE_AFTER_BIND for all bindings to allow per-frame descriptor updates
        let render_binding_flags = [
            vk::DescriptorBindingFlags::UPDATE_AFTER_BIND, // Binding 0
            vk::DescriptorBindingFlags::UPDATE_AFTER_BIND, // Binding 1
            vk::DescriptorBindingFlags::UPDATE_AFTER_BIND, // Binding 2
            vk::DescriptorBindingFlags::UPDATE_AFTER_BIND, // Binding 3
            vk::DescriptorBindingFlags::UPDATE_AFTER_BIND, // Binding 4
        ];

        let mut render_binding_flags_info =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::default()
                .binding_flags(&render_binding_flags);

        let render_layout_info = vk::DescriptorSetLayoutCreateInfo::default()
            .bindings(&render_bindings)
            .flags(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL)
            .push_next(&mut render_binding_flags_info);

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

    /// Create descriptor pool and allocate static descriptor set (internal helper).
    ///
    /// # Arguments
    /// * `layout` - Descriptor set layout to use
    /// * `pool_name` - Name for logging/debugging
    /// * `validate_alignment` - Whether to validate descriptor offset alignment
    fn create_descriptor_set_internal(
        &mut self,
        layout: vk::DescriptorSetLayout,
        pool_name: &str,
        validate_alignment: bool,
        include_indirect_binding: bool,
    ) -> Result<(vk::DescriptorSet, vk::DescriptorPool), String> {
        // Create descriptor pool
        let descriptor_count = if include_indirect_binding { 6 } else { 5 };
        let pool_sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(descriptor_count)];

        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(1)
            .flags(
                vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET
                    | vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND,
            );

        let descriptor_pool = unsafe {
            self.context
                .device
                .create_descriptor_pool(&pool_info, None)
                .map_err(|e| format!("Failed to create {} descriptor pool: {:?}", pool_name, e))?
        };

        // Allocate descriptor set
        let set_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(std::slice::from_ref(&layout));

        let descriptor_sets = unsafe {
            self.context
                .device
                .allocate_descriptor_sets(&set_info)
                .map_err(|e| format!("Failed to allocate {} descriptor sets: {:?}", pool_name, e))?
        };

        let descriptor_set = descriptor_sets[0];

        // Update descriptor set with buffer views
        let buf_layout = self.buffer.layout();

        // Calculate each region's size (use unaligned sizes for descriptor ranges)
        let particles_region_size =
            buf_layout.max_particles * std::mem::size_of::<buffer::ParticleData>() as u64;
        let dead_list_region_size = buf_layout.max_particles * std::mem::size_of::<u32>() as u64;
        let alive_list_region_size = buf_layout.alive_list_size;

        let particle_buffer_handle = self.buffer.particle_buffer();
        let counters_buffer_handle = self.buffer.counters_buffer();
        let frame_buffer_handle = self.frame_data_buffer.as_ref().map(|(b, _)| *b);

        log::info!(
            "Buffer handles - particle: {:?}, counters: {:?}, frame_data: {:?}",
            particle_buffer_handle,
            counters_buffer_handle,
            frame_buffer_handle
        );

        let particle_buffer_info = [vk::DescriptorBufferInfo {
            buffer: particle_buffer_handle,
            offset: 0,
            range: particles_region_size, // Use actual particle region size
        }];

        log::info!(
            "Creating descriptor with particle buffer range: {} bytes",
            particles_region_size
        );

        let dead_list_info = [vk::DescriptorBufferInfo {
            buffer: self.buffer.particle_buffer(),
            offset: buf_layout.dead_list_offset,
            range: dead_list_region_size, // Use actual dead list size, not aligned
        }];

        // Binding 2: alive_list (read by emit/simulate)
        // Maps to alive[0] initially, updated per-frame via update_compute_descriptor_binding
        let alive_list_info = [vk::DescriptorBufferInfo {
            buffer: self.buffer.particle_buffer(),
            offset: buf_layout.alive_offset,
            range: alive_list_region_size,
        }];

        // Binding 3: alive_list_next (written by simulate)
        // Maps to alive[1] initially, updated per-frame via update_compute_descriptor_binding
        let alive_list_next_info = [vk::DescriptorBufferInfo {
            buffer: self.buffer.particle_buffer(),
            offset: buf_layout.alive_frame_offset[1],
            range: alive_list_region_size,
        }];

        let counters_info = [vk::DescriptorBufferInfo {
            buffer: self.buffer.counters_buffer(),
            offset: 0,
            range: std::mem::size_of::<buffer::ParticleCounters>() as u64,
        }];

        // Binding 5: indirect draw command buffer (16 bytes)
        // Written by simulate shader, read by vkCmdDrawIndirect.
        // Only included for compute descriptor set.
        let indirect_draw_info = if include_indirect_binding {
            Some([vk::DescriptorBufferInfo {
                buffer: self.buffer.indirect_draw_buffer(),
                offset: 0,
                range: 16,
            }])
        } else {
            None
        };

        let mut descriptor_writes = vec![
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&particle_buffer_info),
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&dead_list_info),
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&alive_list_info),
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(3)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&alive_list_next_info),
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(4)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&counters_info),
        ];

        if include_indirect_binding && let Some(info) = &indirect_draw_info {
            descriptor_writes.push(
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(5)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .buffer_info(info),
            );
        }

        log::info!(
            "Updating descriptor set {:?}: binding 0 range={} bytes",
            descriptor_set,
            particle_buffer_info[0].range
        );
        log::info!(
            "Updating descriptor set {:?}: binding 1 range={} bytes",
            descriptor_set,
            dead_list_info[0].range
        );

        unsafe {
            self.context
                .device
                .update_descriptor_sets(&descriptor_writes, &[]);
        }

        // Validate descriptor set offsets for alignment (optional)
        if validate_alignment {
            let device_properties = unsafe {
                self.context
                    .instance
                    .get_physical_device_properties(self.context.physical_device)
            };

            let min_storage_buffer_offset_alignment =
                device_properties.limits.min_storage_buffer_offset_alignment;

            // Validate that all descriptor buffer offsets are properly aligned
            let binding_offsets = [
                (0, 0u64),                             // particle data
                (1, buf_layout.dead_list_offset),      // dead list
                (2, buf_layout.alive_offset),          // alive[0]
                (3, buf_layout.alive_frame_offset[1]), // alive[1]
            ];

            for (binding, offset) in binding_offsets.iter() {
                if offset % min_storage_buffer_offset_alignment != 0 {
                    return Err(format!(
                        "Descriptor set binding {} offset {} is not aligned to min_storage_buffer_offset_alignment ({})",
                        binding, offset, min_storage_buffer_offset_alignment
                    ));
                }
            }
        }

        info!(
            "Created and allocated particle {} descriptor set",
            pool_name
        );
        Ok((descriptor_set, descriptor_pool))
    }

    /// Create descriptor pool and allocate static descriptor set for compute (Set 0).
    fn create_compute_descriptor_set(
        &mut self,
    ) -> Result<(vk::DescriptorSet, vk::DescriptorPool), String> {
        let compute_layout = self
            .compute_descriptor_layout
            .ok_or("Compute descriptor layout not created")?;

        self.create_descriptor_set_internal(compute_layout, "compute", true, true)
    }

    /// Create descriptor pool and allocate static descriptor set for render (Set 0).
    /// Uses VERTEX/FRAGMENT stage flags instead of COMPUTE for graphics pipeline compatibility.
    fn create_render_descriptor_set(
        &mut self,
    ) -> Result<(vk::DescriptorSet, vk::DescriptorPool), String> {
        let render_layout = self
            .render_descriptor_layout
            .ok_or("Render descriptor layout not created")?;

        self.create_descriptor_set_internal(render_layout, "render", false, false)
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
    ///   The render graph will bind Set 1 automatically during particle rendering.
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

        let storage_layout_info =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(&storage_bindings);

        let storage_layout = unsafe {
            self.context
                .device
                .create_descriptor_set_layout(&storage_layout_info, None)
                .map_err(|e| format!("Failed to create storage descriptor layout: {:?}", e))?
        };

        let pipeline = PipelineBuilder::new(self.context.clone())
            .with_shaders(vertex_shader.vk(), fragment_shader.vk())
            .with_descriptor_layouts(vec![render_layout, storage_layout])
            .with_depth_test(true, false, crate::pipeline::CompareOp::Greater)
            .with_alpha_blending()
            .with_cull_mode(CullMode::None, FrontFace::CounterClockwise)
            .with_rendering_formats(
                Some(crate::texture::ImageFormat::R16G16B16A16Sfloat),
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

    pub fn particle_buffer(&self) -> vk::Buffer {
        self.buffer.particle_buffer()
    }

    pub fn indirect_draw_buffer(&self) -> vk::Buffer {
        self.buffer.indirect_draw_buffer()
    }

    /// Reset counters for the simulate pass on the GPU.
    ///
    /// Always resets `workgroups_finished` to 0. When `emit_ran` is false (emit was
    /// skipped), also resets `alive_count` to 0 and sets `emit_count` to
    /// `cached_alive_count` so simulate processes exactly the survivors in the alive list.
    ///
    /// When emit ran, it already reset `alive_count` to 0 and set `emit_count` to
    /// `cached_alive_count` + actual_emissions, so we must not overwrite those values.
    pub(crate) fn reset_simulate_counters(&self, command_buffer: vk::CommandBuffer, emit_ran: bool) {
        let device = &self.context.device;
        let counters_buffer = self.buffer.counters_buffer();

        // Always reset workgroups_finished to 0 — coordinate which wg writes draw command
        let zero_bytes = 0u32.to_le_bytes();
        unsafe {
            device.cmd_update_buffer(
                command_buffer,
                counters_buffer,
                12, // workgroups_finished at offset 12
                &zero_bytes,
            );
        }

        if !emit_ran {
            // Emit was skipped — reset alive_count and set emit_count to survivor count.
            // This ensures simulate processes only valid alive_list entries.
            unsafe {
                device.cmd_update_buffer(
                    command_buffer,
                    counters_buffer,
                    0, // alive_count at offset 0
                    &zero_bytes,
                );
                let alive_bytes = self.cached_alive_count.to_le_bytes();
                device.cmd_update_buffer(
                    command_buffer,
                    counters_buffer,
                    8, // emit_count at offset 8
                    &alive_bytes,
                );
            }
        }

        let counters_barrier = vk::BufferMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(counters_buffer)
            .offset(0)
            .size(std::mem::size_of::<buffer::ParticleCounters>() as u64);

        unsafe {
            device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                std::slice::from_ref(&counters_barrier),
                &[],
            );
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

        // Reset emit_count to cached_alive_count so emit appends after existing survivors.
        // alive[frame] contains survivors at slots 0..cached_alive_count-1.
        // Emit will write new particles starting at cached_alive_count.
        //
        // Use vkCmdUpdateBuffer to set emit_count on the GPU during command buffer execution.
        let counters_buffer = self.buffer.counters_buffer();
        let emit_count_offset = 8; // emit_count is at offset 8 in ParticleCounters (alive=0, dead=4, emit=8)
        let alive_count_value = self.cached_alive_count;
        let data_bytes = alive_count_value.to_le_bytes();
        unsafe {
            device.cmd_update_buffer(
                command_buffer,
                counters_buffer,
                emit_count_offset,
                &data_bytes,
            );
        }

        // Reset alive_count to 0 using vkCmdUpdateBuffer.
        // (Using vkCmdUpdateBuffer instead of vkCmdFillBuffer for broader driver compatibility,
        //  as some Intel drivers have issues with vkCmdFillBuffer + atomicAdd patterns.)
        let zero_bytes = 0u32.to_le_bytes();
        unsafe {
            device.cmd_update_buffer(
                command_buffer,
                counters_buffer,
                0, // alive_count is at offset 0
                &zero_bytes,
            );
        }

        // Barrier to ensure emit_count and alive_count initialization is visible to compute shader.
        let fill_barrier = vk::BufferMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(counters_buffer)
            .offset(0)
            .size(std::mem::size_of::<ParticleCounters>() as u64);

        unsafe {
            device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                std::slice::from_ref(&fill_barrier),
                &[],
            );
        }

        // Bind emit pipeline
        unsafe {
            device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::COMPUTE, vk_pipeline);
        }

        // Bind static descriptor set (Set 0: particle buffers)
        if let Some(descriptor_set) = self.compute_descriptor_set {
            log::debug!(
                "Emit dispatch: Set 0 descriptor={:?}, particle_buffer={:?}",
                descriptor_set,
                self.buffer.particle_buffer(),
            );
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
            let frame_data_size = std::mem::size_of::<FrameData>() as u64;
            let emitter_size =
                (MAX_EMITTERS as usize * std::mem::size_of::<EmitterConfig>()) as u64;

            let frame_buffer_info = [vk::DescriptorBufferInfo::default()
                .buffer(*frame_buffer)
                .offset(0)
                .range(frame_data_size)];

            let emitter_buffer_info = [vk::DescriptorBufferInfo::default()
                .buffer(*emitter_buffer)
                .offset(0)
                .range(emitter_size)];

            let push_descriptor_writes = [
                vk::WriteDescriptorSet::default()
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .descriptor_count(1)
                    .buffer_info(&frame_buffer_info),
                vk::WriteDescriptorSet::default()
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
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
        self.emit_to_simulate_barrier(command_buffer)?;

        Ok(())
    }

    /// Record simulate pass dispatch.
    pub fn record_simulate_dispatch(
        &self,
        command_buffer: vk::CommandBuffer,
        asset_registry: &AssetRegistry,
        simulate_workgroups: u32,
    ) -> Result<(), String> {
        let device = &self.context.device;

        // NOTE: alive_count and workgroups_finished are reset by reset_simulate_counters()
        // called in execute_compute_pass() before this method. This ensures counters are
        // always at 0 before simulate runs, even when the emit pass was skipped.

        let pipeline = self
            .simulate_pipeline
            .ok_or("Simulate pipeline not created")?;

        let compute_pipeline = asset_registry
            .get_pipeline(pipeline)
            .ok_or("Failed to get simulate pipeline from registry")?;

        let vk_pipeline = compute_pipeline.vk_pipeline();
        let vk_layout = compute_pipeline.vk_layout();

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

        // Update push descriptors (Set 1: frame data + emitter configs)
        if let Some((frame_buffer, _)) = &self.frame_data_buffer {
            let frame_data_size = std::mem::size_of::<FrameData>() as u64;
            let emitter_size =
                (MAX_EMITTERS as usize * std::mem::size_of::<EmitterConfig>()) as u64;

            let frame_buffer_info = [vk::DescriptorBufferInfo::default()
                .buffer(*frame_buffer)
                .offset(0)
                .range(frame_data_size)];

            let emitter_buffer_info = if let Some((emitter_buf, _)) = &self.emitter_configs_buffer {
                Some([vk::DescriptorBufferInfo::default()
                    .buffer(*emitter_buf)
                    .offset(0)
                    .range(emitter_size)])
            } else {
                None
            };

            let mut push_descriptor_writes = vec![
                vk::WriteDescriptorSet::default()
                    .dst_binding(0) // Binding 0 in Set 1 (frame data)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .descriptor_count(1)
                    .buffer_info(&frame_buffer_info),
            ];

            if let Some(info) = &emitter_buffer_info {
                push_descriptor_writes.push(
                    vk::WriteDescriptorSet::default()
                        .dst_binding(1) // Binding 1 in Set 1 (emitter configs)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .descriptor_count(1)
                        .buffer_info(info),
                );
            }

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
    pub fn emit_to_simulate_barrier(
        &self,
        command_buffer: vk::CommandBuffer,
    ) -> Result<(), String> {
        let particle_buffer = self.buffer.particle_buffer();
        let counters_buffer = self.buffer.counters_buffer();
        let device = &self.context.device;

        let total_buffer_size = self.buffer.layout().total_size;

        let counters_size = std::mem::size_of::<buffer::ParticleCounters>() as u64;

        // Create buffer memory barrier for entire particle buffer
        // EMIT pass writes to particle data, dead list, and alive list
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
    /// Ensures particle buffers, alive list, and indirect draw command are visible
    /// to the render pass (vertex shader + indirect draw).
    fn simulate_barrier(&self, command_buffer: vk::CommandBuffer) -> Result<(), String> {
        let particle_buffer = self.buffer.particle_buffer();
        let indirect_draw_buffer = self.buffer.indirect_draw_buffer();
        let device = &self.context.device;

        let particle_buffer_size = self.buffer.layout().total_size;

        // NOTE: SHADER_READ not VERTEX_ATTRIBUTE_READ — the render shader accesses
        // particle data via storage buffer binding, not vertex attributes.
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

        // Barrier for indirect draw buffer: COMPUTE_SHADER → DRAW_INDIRECT.
        // The simulate shader writes the VkDrawIndirectCommand, the render pass reads it.
        let indirect_draw_barrier = BufferMemoryBarrier2 {
            src_stage_mask: PipelineStage2Flags::COMPUTE_SHADER,
            dst_stage_mask: PipelineStage2Flags::DRAW_INDIRECT,
            src_access_mask: AccessFlags2::SHADER_WRITE,
            dst_access_mask: AccessFlags2::INDIRECT_COMMAND_READ,
            src_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            dst_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            buffer: VkBuffer::new(indirect_draw_buffer),
            offset: 0,
            size: 16,
        };

        // Build and execute dependency info
        let dep_info = DependencyInfo::new()
            .add_buffer_barrier2(particle_barrier)
            .add_buffer_barrier2(indirect_draw_barrier);

        dep_info.build(|dep_info| unsafe {
            device.cmd_pipeline_barrier2(command_buffer, dep_info);
        });

        Ok(())
    }

    /// Get maximum particle capacity.
    pub fn max_particles(&self) -> u32 {
        self.max_particles
    }

    /// Get a snapshot of current particle system state.
    ///
    /// Note: Lifetime and performance tracking fields (total_emitted, total_died,
    /// compute timing, dispatch count) are no longer tracked and always return 0.
    pub fn get_stats(&self) -> ParticleStats {
        let particle_data_mb = (self.max_particles as f32) * 48.0 / (1024.0 * 1024.0);
        let index_lists_mb = (self.max_particles as f32) * 12.0 / (1024.0 * 1024.0);
        let counters_mb = 32.0 / (1024.0 * 1024.0);
        let configs_mb = (self.emitters.len() as f32) * 80.0 / (1024.0 * 1024.0);

        ParticleStats {
            max_alive_count: self.max_particles,
            current_alive_count: self.alive_count(),
            dead_count: self.max_particles - self.alive_count(),
            total_emitted: self.total_emitted,
            total_died: 0,
            compute_time_ms: 0.0,
            avg_compute_time_ms: 0.0,
            peak_compute_time_ms: 0.0,
            emitter_counts: self
                .emitters
                .iter()
                .filter(|e| e.emit_rate > 0.0)
                .map(|_| 0)
                .collect(),
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
        assert_eq!(std::mem::size_of::<EmitterConfig>(), 144);
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
            _pad_position: 0.0,
            shape: EmitterShape::Sphere.as_u32(),
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
            _pad_color: Align16Vec4([0.0; 4]),
            shape_params: [2.5, 0.0, 0.0, 0.0],
            gravity: -9.8,
            turbulence_strength: 0.0,
            turbulence_frequency: 3.0,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: EmitterConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.get_shape(), EmitterShape::Sphere);
        assert_eq!(deserialized.shape_params[0], 2.5);
        assert_eq!(deserialized.position, [1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_emitter_config_field_offsets() {
        // Verify field offsets match WGSL vec3f/vec4f alignment rules
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
        assert_eq!(std::mem::offset_of!(EmitterConfig, _pad_color), 96);
        assert_eq!(std::mem::offset_of!(EmitterConfig, shape_params), 112);
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
