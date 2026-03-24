//! GPU-driven particle system using a single global buffer with atomic counters,
//! index list management, and indirect drawing.

pub mod buffer;
pub mod debug_readback;
pub mod descriptors;
pub mod pipeline;
pub mod presets;
pub mod stats;
pub mod timing;
pub mod types;
pub mod validation;

pub use buffer::{FrameData, GlobalParticleBuffer, ParticleCounters, ParticleData};
pub use debug_readback::{IndirectDrawCommandData, ParticleDebugData, ParticleDebugReadback};
pub use presets::EmitterPreset;
pub use stats::ParticleStats;
pub use types::{Align16Vec4, EmitterConfig, EmitterHandle, EmitterShape};
pub use validation::{
    ValidationError, validate_all_emitters, validate_counters, validate_emitter_config,
};

use std::rc::Rc;

use ash::vk;
use log::{info, warn};

use crate::handle::PipelineHandle;
use crate::renderer::registry::AssetRegistry;
use crate::sync::{
    AccessFlags2, BufferMemoryBarrier2, DependencyInfo, PipelineStage2Flags, VkBuffer,
};
use crate::vulkan::context::VulkanContext;

use types::EmitterState;

/// Default maximum particles across all emitters
pub const DEFAULT_MAX_PARTICLES: u32 = 1_048_576; // 1M particles (48MB)

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

    /// Per-frame data buffers for push descriptor updates [frames_in_flight]
    frame_data_buffers: [Option<(vk::Buffer, gpu_allocator::vulkan::Allocation)>; 2],

    /// Per-frame emitter configs buffers for push descriptor updates [frames_in_flight]
    emitter_configs_buffers: [Option<(vk::Buffer, gpu_allocator::vulkan::Allocation)>; 2],

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

    /// Cached upper bound on alive particles across all emitters
    estimated_max_alive: u32,

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
            frame_data_buffers: [None, None],
            emitter_configs_buffers: [None, None],
            destroyed: false,
            compute_descriptor_set: None,
            render_descriptor_set: None,
            _compute_descriptor_pool: vk::DescriptorPool::null(),
            _render_descriptor_pool: vk::DescriptorPool::null(),
            max_particles,
            estimated_max_alive: max_particles,
            total_emitted: 0,
            debug_readback: None,
        };

        // All particles start dead, alive lists are empty
        system.buffer.initialize_index_lists()?;

        system.create_descriptor_layouts(context)?;

        let (compute_descriptor_set, compute_pool) = system.create_compute_descriptor_set()?;
        system.compute_descriptor_set = Some(compute_descriptor_set);
        system._compute_descriptor_pool = compute_pool;

        let (render_descriptor_set, render_pool) = system.create_render_descriptor_set()?;
        system.render_descriptor_set = Some(render_descriptor_set);
        system._render_descriptor_pool = render_pool;

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

        if self.emitters.len() <= index as usize {
            self.emitters
                .resize(index as usize + 1, EmitterConfig::default());
        }
        if self.emitter_states.len() <= index as usize {
            self.emitter_states
                .resize(index as usize + 1, EmitterState::default());
        }

        self.emitters[index as usize] = config;
        self.recompute_estimated_max_alive();

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
            self.recompute_estimated_max_alive();
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

        self.upload_emitter_configs(frame_index as usize)?;

        self.recompute_estimated_max_alive();

        // Use calculate_emit_count for proper rate-based emission with accumulators
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

        self.update_frame_data(delta_time, total_emit_count, total_burst_count, frame_index)?;

        for state in &mut self.emitter_states {
            state.burst_count = 0;
        }

        // The actual compute dispatch happens during render graph execution
        // via record_compute_dispatch(). This just prepares the data.

        let emit_count = total_emit_count + total_burst_count;

        #[cfg(debug_assertions)]
        {
            let validation_errors = validate_all_emitters(&self.emitters);
            if !validation_errors.is_empty() {
                for error in &validation_errors {
                    log::warn!("Emitter validation error: {}", error);
                }
            }
        }

        if total_this_frame > 0 {
            self.total_emitted += total_this_frame as u64;
        }

        Ok((self.estimated_max_alive, emit_count))
    }

    /// Upload emitter configurations to GPU buffer for the given frame.
    fn upload_emitter_configs(&self, frame_index: usize) -> Result<(), String> {
        let fi = frame_index % 2;
        if let Some((_buffer, allocation)) = &self.emitter_configs_buffers[fi] {
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
        let fi = (frame_index as usize) % 2;
        if let Some((_buffer, allocation)) = &self.frame_data_buffers[fi] {
            if let Some(mapped) = allocation.mapped_ptr() {
                // Calculate active emitter count (emitters with emit_rate > 0 or burst_count > 0)
                let active_emitter_count = self
                    .emitters
                    .iter()
                    .zip(self.emitter_states.iter())
                    .filter(|(e, s)| e.emit_rate > 0.0 || s.burst_count > 0)
                    .count() as u32;

                // total_simulate_count = estimated max alive + newly emitted particles
                let total_simulate_count = self.estimated_max_alive + emit_count + burst_count;

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
                    "FrameData {}: dt={:.6} emit={} burst={} max_alive={} sim={} emitters={}",
                    frame_index,
                    frame_data.delta_time,
                    frame_data.total_emit_count,
                    frame_data.burst_count,
                    self.estimated_max_alive,
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

        unsafe {
            device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::GRAPHICS, pipeline);
        }

        // Update alive descriptor binding offset for this frame.
        // Render reads from the simulate output region: alive[(frame_index+1)%2].
        self.update_render_descriptor_binding(frame_index)?;

        // Set 0: particle buffers
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

        // Set 1: FrameUniforms from renderer (view/proj matrices)
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

        // Simulate shader writes VkDrawIndirectCommand with vertex_count = alive_count * 6
        if self.estimated_max_alive > 0 {
            unsafe {
                device.cmd_draw_indirect(
                    command_buffer,
                    self.buffer.indirect_draw_buffer(frame_index),
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
    /// Binding 4 (counters): per-frame counters buffer.
    /// Binding 5 (indirect draw): per-frame indirect draw buffer (compute set only).
    pub fn update_compute_descriptor_binding(&self, frame_index: usize) -> Result<(), String> {
        let device = &self.context.device;
        let descriptor_set = self
            .compute_descriptor_set
            .ok_or("Compute descriptor set not allocated")?;

        let layout = self.buffer.layout();
        let next_frame = (frame_index + 1) % 2;
        let fi = frame_index % 2;

        let alive_list_info = [vk::DescriptorBufferInfo {
            buffer: self.buffer.particle_buffer(),
            offset: layout.alive_frame_offset[frame_index % 2],
            range: layout.alive_list_size,
        }];

        let alive_next_info = [vk::DescriptorBufferInfo {
            buffer: self.buffer.particle_buffer(),
            offset: layout.alive_frame_offset[next_frame],
            range: layout.alive_list_size,
        }];

        let counters_info = [vk::DescriptorBufferInfo {
            buffer: self.buffer.counters_buffer(fi),
            offset: 0,
            range: std::mem::size_of::<buffer::ParticleCounters>() as u64,
        }];

        let indirect_draw_info = [vk::DescriptorBufferInfo {
            buffer: self.buffer.indirect_draw_buffer(fi),
            offset: 0,
            range: 16,
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
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(4)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&counters_info),
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(5)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&indirect_draw_info),
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
    pub fn update_render_descriptor_binding(&self, frame_index: usize) -> Result<(), String> {
        let device = &self.context.device;
        let descriptor_set = self
            .render_descriptor_set
            .ok_or("Render descriptor set not allocated")?;

        let layout = self.buffer.layout();
        let next_frame = (frame_index + 1) % 2;
        let fi = frame_index % 2;

        let alive_list_info = [vk::DescriptorBufferInfo {
            buffer: self.buffer.particle_buffer(),
            offset: layout.alive_frame_offset[next_frame],
            range: layout.alive_list_size,
        }];

        let counters_info = [vk::DescriptorBufferInfo {
            buffer: self.buffer.counters_buffer(fi),
            offset: 0,
            range: std::mem::size_of::<buffer::ParticleCounters>() as u64,
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
                .dst_binding(4)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&counters_info),
        ];

        unsafe {
            device.update_descriptor_sets(&descriptor_writes, &[]);
        }

        Ok(())
    }

    /// Get current alive particle count (estimated upper bound).
    ///
    /// Returns `estimated_max_alive` computed from emitter configs.
    /// This is an upper bound — the actual GPU-side count may be lower.
    pub fn alive_count(&self) -> u32 {
        self.estimated_max_alive
    }

    /// Set alive count from external readback (e.g., debug readback via `vkCmdCopyBuffer`).
    pub fn set_alive_count(&mut self, count: u32) {
        self.estimated_max_alive = count;
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
                state.emit_accumulator += emitter.emit_rate * delta_time;

                let to_emit = state.emit_accumulator as u32;
                state.emit_accumulator -= to_emit as f32;

                total_emit += to_emit;
            }
        }

        total_emit
    }

    pub fn max_estimated_alive(&self) -> u32 {
        self.estimated_max_alive
    }

    fn recompute_estimated_max_alive(&mut self) {
        self.estimated_max_alive = self
            .emitters
            .iter()
            .filter(|e| e.emit_rate > 0.0)
            .map(|e| {
                let max_alive = e.emit_rate * e.base_lifetime * (1.0 + e.lifetime_variation);
                max_alive.ceil() as u32
            })
            .sum::<u32>()
            .min(self.max_particles);
    }

    /// Get emit pipeline handle.
    pub fn emit_pipeline_handle(&self) -> Option<PipelineHandle> {
        self.emit_pipeline
    }

    /// Get simulate pipeline handle.
    pub fn simulate_pipeline_handle(&self) -> Option<PipelineHandle> {
        self.simulate_pipeline
    }

    /// Get render pipeline handle.
    pub fn render_pipeline_handle(&self) -> Option<PipelineHandle> {
        self.render_pipeline
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

    /// Get the pre-computed buffer layout offsets.
    pub fn buffer_layout(&self) -> &buffer::ParticleBufferLayout {
        self.buffer.layout()
    }

    /// Get the emitter configs push descriptor buffer for the given frame.
    pub fn emitter_configs_buffer(&self, frame_index: usize) -> Option<vk::Buffer> {
        self.emitter_configs_buffers[frame_index % 2]
            .as_ref()
            .map(|(buf, _)| *buf)
    }

    /// Destroy all particle system resources.
    pub fn destroy(&mut self) {
        if self.destroyed {
            return;
        }
        self.destroyed = true;

        info!("Destroying particle system");

        info!("  destroying push descriptor buffers");
        for frame_idx in 0..2 {
            if let Some((buffer, allocation)) = self.frame_data_buffers[frame_idx].take() {
                unsafe {
                    if let Ok(mut allocator) = self.context.allocator.try_borrow_mut() {
                        allocator.free(allocation).ok();
                    }
                    self.context.device.destroy_buffer(buffer, None);
                }
            }
            if let Some((buffer, allocation)) = self.emitter_configs_buffers[frame_idx].take() {
                unsafe {
                    if let Ok(mut allocator) = self.context.allocator.try_borrow_mut() {
                        allocator.free(allocation).ok();
                    }
                    self.context.device.destroy_buffer(buffer, None);
                }
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

    /// Reset counters for the simulate pass on the GPU.
    ///
    /// Always resets `workgroups_finished` to 0. When `emit_ran` is false (emit was
    /// skipped), also resets `alive_count` to 0 and copies the actual survivor count
    /// from the previous frame's counters to `emit_count` so simulate processes
    /// the correct range of the alive list.
    ///
    /// When emit ran, it already reset `alive_count` to 0 and set `emit_count`
    /// to the actual survivor count + actual_emissions (via vkCmdCopyBuffer),
    /// so we must not overwrite those values.
    pub fn reset_simulate_counters(
        &self,
        command_buffer: vk::CommandBuffer,
        emit_ran: bool,
        frame_index: usize,
    ) {
        let device = &self.context.device;
        let counters_buffer = self.buffer.counters_buffer(frame_index);

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
            // Emit was skipped — reset alive_count and copy the actual survivor count
            // from the previous frame's counters to emit_count.
            // This ensures simulate processes only valid alive_list entries.
            unsafe {
                device.cmd_update_buffer(
                    command_buffer,
                    counters_buffer,
                    0, // alive_count at offset 0
                    &zero_bytes,
                );
            }
            // Copy alive_count → emit_count and dead_count → dead_count
            // from previous frame's counters (dead list is shared, see record_emit_dispatch).
            let prev_fi = (frame_index + 1) % 2;
            let prev_counters = self.buffer.counters_buffer(prev_fi);
            let copy_regions = [
                vk::BufferCopy {
                    src_offset: 0, // alive_count at offset 0
                    dst_offset: 8, // emit_count at offset 8
                    size: 4,
                },
                vk::BufferCopy {
                    src_offset: 4, // dead_count at offset 4
                    dst_offset: 4, // dead_count at offset 4
                    size: 4,
                },
            ];
            unsafe {
                device.cmd_copy_buffer(
                    command_buffer,
                    prev_counters,
                    counters_buffer,
                    &copy_regions,
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
        frame_index: usize,
    ) -> Result<(), String> {
        let pipeline = self.emit_pipeline.ok_or("Emit pipeline not created")?;

        let compute_pipeline = asset_registry
            .get_pipeline(pipeline)
            .ok_or("Failed to get emit pipeline from registry")?;

        let vk_pipeline = compute_pipeline.vk_pipeline();
        let vk_layout = compute_pipeline.vk_layout();

        let device = &self.context.device;

        // Copy the actual alive_count from the previous frame's counters buffer to
        // emit_count in the current frame's counters buffer.
        //
        // The previous frame (fi_prev) wrote its final alive_count to
        // counters_buffers[fi_prev].alive_count (offset 0). The semaphore
        // guarantees that frame's GPU work is complete before this command
        // buffer executes, so the value is fresh and correct.
        //
        // We copy alive_count (4 bytes at offset 0) from the previous frame's
        // counters to emit_count (offset 8) in the current frame's counters.
        // The emit shader then appends new particles after the existing survivors.
        //
        // This replaces the old approach of using CPU-provided cached_alive_count
        // via vkCmdUpdateBuffer, which was stale due to 2-FiF timing: the CPU
        // reads counters before the previous frame's GPU has finished, getting
        // a value from 2 frames ago that doesn't match the actual survivor count
        // in the alive list.
        let prev_fi = (frame_index + 1) % 2;
        let counters_buffer = self.buffer.counters_buffer(frame_index);
        let prev_counters_buffer = self.buffer.counters_buffer(prev_fi);

        // Copy alive_count → emit_count and dead_count → dead_count.
        // dead_count must be synchronized because the dead list is shared across
        // frames but counters are double-buffered.
        let copy_regions = [
            vk::BufferCopy {
                src_offset: 0, // alive_count at offset 0 in prev counters
                dst_offset: 8, // emit_count at offset 8 in current counters
                size: 4,
            },
            vk::BufferCopy {
                src_offset: 4, // dead_count at offset 4 in prev counters
                dst_offset: 4, // dead_count at offset 4 in current counters
                size: 4,
            },
        ];
        unsafe {
            device.cmd_copy_buffer(
                command_buffer,
                prev_counters_buffer,
                counters_buffer,
                &copy_regions,
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

        unsafe {
            device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::COMPUTE, vk_pipeline);
        }

        // Set 0: particle buffers
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
        let fi = frame_index % 2;
        if let Some((frame_buffer, _)) = &self.frame_data_buffers[fi]
            && let Some((emitter_buffer, _)) = &self.emitter_configs_buffers[fi]
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

        unsafe {
            device.cmd_dispatch(command_buffer, emit_workgroups, 1, 1);
        }

        // Add pipeline barrier after EMIT pass to ensure memory synchronization before SIMULATE pass
        // EMIT pass writes to: particle buffers, alive list, counters
        // SIMULATE pass reads from: particle buffers, alive list, counters
        self.emit_to_simulate_barrier(command_buffer, frame_index)?;

        Ok(())
    }

    /// Record simulate pass dispatch.
    pub fn record_simulate_dispatch(
        &self,
        command_buffer: vk::CommandBuffer,
        asset_registry: &AssetRegistry,
        simulate_workgroups: u32,
        frame_index: usize,
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

        unsafe {
            device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::COMPUTE, vk_pipeline);
        }

        // Set 0: particle buffers
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

        let fi = frame_index % 2;
        if let Some((frame_buffer, _)) = &self.frame_data_buffers[fi] {
            let frame_data_size = std::mem::size_of::<FrameData>() as u64;
            let emitter_size =
                (MAX_EMITTERS as usize * std::mem::size_of::<EmitterConfig>()) as u64;

            let frame_buffer_info = [vk::DescriptorBufferInfo::default()
                .buffer(*frame_buffer)
                .offset(0)
                .range(frame_data_size)];

            let emitter_buffer_info =
                if let Some((emitter_buf, _)) = &self.emitter_configs_buffers[fi] {
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

        unsafe {
            device.cmd_dispatch(command_buffer, simulate_workgroups, 1, 1);
        }

        // Add pipeline barrier after SIMULATE pass to ensure memory synchronization before RENDER pass
        // SIMULATE pass writes to: particle buffers, alive list
        // RENDER pass reads from: particle buffers (for vertex attributes), alive list (for indirect drawing)
        self.simulate_barrier(command_buffer, frame_index)?;

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
        frame_index: usize,
    ) -> Result<(), String> {
        let particle_buffer = self.buffer.particle_buffer();
        let counters_buffer = self.buffer.counters_buffer(frame_index);
        let device = &self.context.device;

        let total_buffer_size = self.buffer.layout().total_size;

        let counters_size = std::mem::size_of::<buffer::ParticleCounters>() as u64;

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
    fn simulate_barrier(
        &self,
        command_buffer: vk::CommandBuffer,
        frame_index: usize,
    ) -> Result<(), String> {
        let particle_buffer = self.buffer.particle_buffer();
        let indirect_draw_buffer = self.buffer.indirect_draw_buffer(frame_index);
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
            _pad_forces: 0.0,
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
