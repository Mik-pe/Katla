//! Metal-native GPU-driven particle system.
//!
//! Mirrors the Vulkan `GlobalParticleSystem` using Metal compute pipelines.
//! Uses the same WGSL shaders compiled to MSL via naga.

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLBlitCommandEncoder, MTLCommandBuffer, MTLCommandEncoder, MTLComputePipelineState, MTLDevice,
};

use log::info;

use crate::backend::command::{GpuCommandBuffer, GpuComputeEncoder};
use crate::backend::resource::GpuBuffer;
use crate::error::RendererError;
use crate::particles::types::{EmitterConfig, EmitterHandle};
use crate::pipeline::CompareOp;

use super::buffer::MetalBuffer;
use super::compute_encoder::MetalComputeEncoder;
use super::context::MetalContext;
use super::metal_renderer::{FRAMES_IN_FLIGHT, frame_slot};
use super::shader;

/// Maximum particles supported by shaders (must match MAX_PARTICLES in WGSL)
const SHADER_MAX_PARTICLES: u32 = 1_048_576;

/// Maximum emitters in system (must match MAX_EMITTERS in WGSL)
const MAX_EMITTERS: u32 = 1024;

/// Workgroup size for particle emit compute shader (must match @workgroup_size in particle_emit.wgsl)
pub(crate) const PARTICLE_EMIT_WORKGROUP_SIZE: u32 = 256;

/// Workgroup size for particle simulate compute shader (must match @workgroup_size in particle_simulate.wgsl)
pub(crate) const PARTICLE_SIMULATE_WORKGROUP_SIZE: u32 = 64;

/// Particle data structure (64 bytes).
///
/// Layout must match WGSL struct exactly.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct ParticleData {
    position: [f32; 3],
    scale: f32,
    velocity: [f32; 3],
    lifetime: f32,
    color: [f32; 4],
    emitter_index: u32,
    max_lifetime: f32,
    initial_scale: f32,
    _pad: f32,
}

/// Per-frame data for particle simulation (32 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct FrameData {
    delta_time: f32,
    total_emit_count: u32,
    emitter_count: u32,
    random_seed: u32,
    total_simulate_count: u32,
    burst_count: u32,
    frame_index: u32,
    _pad: u32,
}

/// Atomic counters for particle management (16 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct ParticleCounters {
    alive_count: u32,
    dead_count: u32,
    emit_count: u32,
    workgroups_finished: u32,
}

/// Per-emitter runtime state (not uploaded to GPU).
#[derive(Clone, Default)]
struct EmitterState {
    _burst_count: u32,
    _emit_accumulator: f32,
}

/// Pre-computed buffer layout offsets.
#[derive(Clone, Copy, Debug)]
struct BufferLayout {
    _particles_size: u64,
    dead_list_offset: u64,
    _dead_list_size: u64,
    _alive_offset: u64,
    _alive_list_size: u64,
    _alive_frame_offset: [u64; FRAMES_IN_FLIGHT],
    total_size: u64,
}

impl BufferLayout {
    fn new(max_particles: u32) -> Self {
        let mp = max_particles as u64;
        let particle_data_size = mp * std::mem::size_of::<ParticleData>() as u64;
        let particles_size = particle_data_size.next_multiple_of(256);

        let dead_list_raw = mp * std::mem::size_of::<u32>() as u64;
        let dead_list_size = dead_list_raw.next_multiple_of(256);

        let alive_list_raw = mp * std::mem::size_of::<u32>() as u64;
        let alive_list_size = alive_list_raw.next_multiple_of(256);

        let dead_list_offset = particles_size;
        let alive_offset = dead_list_offset + dead_list_size;
        let alive_frame_offset = [alive_offset, alive_offset + alive_list_size];

        let total_size = alive_offset + 2 * alive_list_size;

        Self {
            _particles_size: particles_size,
            dead_list_offset,
            _dead_list_size: dead_list_size,
            _alive_offset: alive_offset,
            _alive_list_size: alive_list_size,
            _alive_frame_offset: alive_frame_offset,
            total_size,
        }
    }
}

/// Metal-native GPU-driven particle system.
///
/// Manages all particle effects using a single global buffer pool with atomic
/// counters, index list management, and three compute passes (emit, simulate,
/// draw_command).
pub(crate) struct MetalParticleSubsystem {
    // GPU buffers
    _particle_buffer: MetalBuffer,
    _counters_buffers: [MetalBuffer; FRAMES_IN_FLIGHT],
    _indirect_draw_buffers: [MetalBuffer; FRAMES_IN_FLIGHT],
    _frame_data_buffers: [MetalBuffer; FRAMES_IN_FLIGHT],
    _emitter_config_buffers: [MetalBuffer; FRAMES_IN_FLIGHT],

    // Compute pipelines
    _emit_pipeline: Retained<ProtocolObject<dyn MTLComputePipelineState>>,
    _simulate_pipeline: Retained<ProtocolObject<dyn MTLComputePipelineState>>,
    _draw_command_pipeline: Retained<ProtocolObject<dyn MTLComputePipelineState>>,
    render_pipeline: Option<super::pipeline::MetalGraphicsPipeline>,

    // Buffer layout
    _layout: BufferLayout,

    // Emitter pool
    _emitters: Vec<EmitterConfig>,
    _emitter_states: Vec<EmitterState>,
    _next_slot: u32,
    _free_slots: Vec<u32>,

    // State
    _max_particles: u32,
    _frame_count: u32,
    _estimated_max_alive: u32,
    _total_emitted: u64,
}

impl MetalParticleSubsystem {
    /// Create and initialize the particle subsystem.
    pub(crate) fn new(context: &MetalContext, max_particles: u32) -> Result<Self, RendererError> {
        if max_particles == 0 {
            return Err(RendererError::InvalidOperation(
                "max_particles must be greater than 0".to_string(),
            ));
        }
        if max_particles > SHADER_MAX_PARTICLES {
            return Err(RendererError::InvalidOperation(format!(
                "max_particles ({}) exceeds shader limit ({})",
                max_particles, SHADER_MAX_PARTICLES
            )));
        }

        info!(
            "Initializing Metal particle system (max particles: {})",
            max_particles
        );

        let layout = BufferLayout::new(max_particles);

        // Create main particle buffer (GPU-only, large)
        let particle_buffer = context.create_buffer(layout.total_size, false)?;

        // Create double-buffered resources
        let counters_size = std::mem::size_of::<ParticleCounters>() as u64;
        let indirect_draw_size = 16u64; // DrawIndirectCommand: 4 x u32
        let frame_data_size = std::mem::size_of::<FrameData>() as u64;
        let emitter_config_size =
            (MAX_EMITTERS as u64) * std::mem::size_of::<EmitterConfig>() as u64;

        let counters_buf_0 = context.create_buffer(counters_size, true)?;
        let counters_buf_1 = context.create_buffer(counters_size, true)?;
        let indirect_draw_buf_0 = context.create_buffer(indirect_draw_size, true)?;
        let indirect_draw_buf_1 = context.create_buffer(indirect_draw_size, true)?;
        let frame_data_buf_0 = context.create_buffer(frame_data_size, true)?;
        let frame_data_buf_1 = context.create_buffer(frame_data_size, true)?;
        let emitter_config_buf_0 = context.create_buffer(emitter_config_size, true)?;
        let emitter_config_buf_1 = context.create_buffer(emitter_config_size, true)?;

        // Initialize counters: dead_count = max_particles, all else 0
        for buf in [&counters_buf_0, &counters_buf_1] {
            let counters = ParticleCounters {
                alive_count: 0,
                dead_count: max_particles,
                emit_count: 0,
                workgroups_finished: 0,
            };
            let ptr = buf.map();
            unsafe {
                std::ptr::copy_nonoverlapping(
                    &counters as *const ParticleCounters as *const u8,
                    ptr,
                    std::mem::size_of::<ParticleCounters>(),
                );
            }
            buf.unmap();
        }

        // Compile compute shaders
        let emit_source = super::metal_renderer::read_shader("particles/particle_emit.wgsl")?;
        let simulate_source =
            super::metal_renderer::read_shader("particles/particle_simulate.wgsl")?;
        let draw_command_source =
            super::metal_renderer::read_shader("particles/particle_draw_command.wgsl")?;

        let emit_compiled = shader::compile_wgsl_to_metal(
            &context.device,
            &emit_source,
            &["cs_main"],
            shader::ShaderProfile::ParticleCompute,
        )?;
        let simulate_compiled = shader::compile_wgsl_to_metal(
            &context.device,
            &simulate_source,
            &["cs_main"],
            shader::ShaderProfile::ParticleCompute,
        )?;
        let draw_command_compiled = shader::compile_wgsl_to_metal(
            &context.device,
            &draw_command_source,
            &["cs_main"],
            shader::ShaderProfile::ParticleCompute,
        )?;

        let emit_fn = emit_compiled
            .module
            .entry_points
            .get("cs_main")
            .ok_or_else(|| RendererError::InvalidOperation("Emit entry point not found".into()))?;
        let simulate_fn = simulate_compiled
            .module
            .entry_points
            .get("cs_main")
            .ok_or_else(|| {
                RendererError::InvalidOperation("Simulate entry point not found".into())
            })?;
        let draw_command_fn = draw_command_compiled
            .module
            .entry_points
            .get("cs_main")
            .ok_or_else(|| {
                RendererError::InvalidOperation("Draw command entry point not found".into())
            })?;

        let emit_pipeline = context
            .device
            .newComputePipelineStateWithFunction_error(emit_fn)
            .map_err(|e| {
                let msg = e.localizedDescription().to_string();
                RendererError::ResourceCreationFailed(format!(
                    "Failed to create emit compute pipeline: {}",
                    msg
                ))
            })?;

        let simulate_pipeline = context
            .device
            .newComputePipelineStateWithFunction_error(simulate_fn)
            .map_err(|e| {
                let msg = e.localizedDescription().to_string();
                RendererError::ResourceCreationFailed(format!(
                    "Failed to create simulate compute pipeline: {}",
                    msg
                ))
            })?;

        let draw_command_pipeline = context
            .device
            .newComputePipelineStateWithFunction_error(draw_command_fn)
            .map_err(|e| {
                let msg = e.localizedDescription().to_string();
                RendererError::ResourceCreationFailed(format!(
                    "Failed to create draw command compute pipeline: {}",
                    msg
                ))
            })?;

        info!("Metal particle system initialized successfully");

        let system = Self {
            _particle_buffer: particle_buffer,
            _counters_buffers: [counters_buf_0, counters_buf_1],
            _indirect_draw_buffers: [indirect_draw_buf_0, indirect_draw_buf_1],
            _frame_data_buffers: [frame_data_buf_0, frame_data_buf_1],
            _emitter_config_buffers: [emitter_config_buf_0, emitter_config_buf_1],
            _emit_pipeline: emit_pipeline,
            _simulate_pipeline: simulate_pipeline,
            _draw_command_pipeline: draw_command_pipeline,
            render_pipeline: None,
            _layout: layout,
            _emitters: Vec::with_capacity(MAX_EMITTERS as usize),
            _emitter_states: Vec::with_capacity(MAX_EMITTERS as usize),
            _next_slot: 0,
            _free_slots: Vec::new(),
            _max_particles: max_particles,
            _frame_count: 0,
            _estimated_max_alive: max_particles,
            _total_emitted: 0,
        };
        system.initialize_index_lists(context)?;
        Ok(system)
    }

    /// Repopulate the dead list (0..max_particles) via a staging blit and
    /// reset both slots' counters. Alive-list contents are stale afterwards,
    /// which is fine: alive_count gates every read.
    pub(crate) fn initialize_index_lists(
        &self,
        context: &MetalContext,
    ) -> Result<(), RendererError> {
        let dead_list_size = (self._max_particles as u64) * 4;
        let staging = context.create_buffer(dead_list_size, true)?;
        let ptr = staging.map();
        unsafe {
            let dst = ptr as *mut u32;
            for i in 0..self._max_particles {
                *dst.add(i as usize) = i;
            }
        }
        staging.unmap();

        let mut cmd_buffer = context.create_command_buffer();
        cmd_buffer.begin();
        let blit_encoder = cmd_buffer.inner.blitCommandEncoder().expect("blit encoder");
        unsafe {
            blit_encoder.copyFromBuffer_sourceOffset_toBuffer_destinationOffset_size(
                &staging.inner,
                0,
                &self._particle_buffer.inner,
                self._layout.dead_list_offset as usize,
                dead_list_size as usize,
            );
            blit_encoder.endEncoding();
        }
        cmd_buffer.end();
        cmd_buffer.submit(context);
        cmd_buffer.inner.waitUntilCompleted();

        for buf in &self._counters_buffers {
            let counters = ParticleCounters {
                alive_count: 0,
                dead_count: self._max_particles,
                emit_count: 0,
                workgroups_finished: 0,
            };
            let ptr = buf.map();
            unsafe {
                std::ptr::copy_nonoverlapping(
                    &counters as *const ParticleCounters as *const u8,
                    ptr,
                    std::mem::size_of::<ParticleCounters>(),
                );
            }
            buf.unmap();
        }
        Ok(())
    }

    // -- Emitter management --

    pub(crate) fn create_emitter(
        &mut self,
        config: EmitterConfig,
    ) -> Result<EmitterHandle, String> {
        if self._emitters.len() >= MAX_EMITTERS as usize {
            return Err(format!("Maximum emitter count ({}) reached", MAX_EMITTERS));
        }

        let index = self._free_slots.pop().unwrap_or(self._next_slot);
        if index >= self._next_slot {
            self._next_slot = index + 1;
        }

        if self._emitters.len() <= index as usize {
            self._emitters
                .resize(index as usize + 1, EmitterConfig::default());
        }
        if self._emitter_states.len() <= index as usize {
            self._emitter_states
                .resize(index as usize + 1, EmitterState::default());
        }

        self._emitters[index as usize] = config;
        self._emitter_states[index as usize] = EmitterState::default();
        self.recompute_estimated_max_alive();

        Ok(EmitterHandle::new(index))
    }

    pub(crate) fn update_emitter(&mut self, handle: EmitterHandle, config: EmitterConfig) {
        if handle.index() < self._emitters.len() as u32 {
            self._emitters[handle.index() as usize] = config;
            self.recompute_estimated_max_alive();
        }
    }

    pub(crate) fn destroy_emitter(&mut self, handle: EmitterHandle, kill_all: bool) {
        if handle.index() < self._emitters.len() as u32 {
            self._emitters[handle.index() as usize] = EmitterConfig {
                emit_rate: 0.0,
                kill_all: if kill_all { 1 } else { 0 },
                ..Default::default()
            };
            if handle.index() < self._emitter_states.len() as u32 {
                self._emitter_states[handle.index() as usize] = EmitterState::default();
            }
            self._free_slots.push(handle.index());
        }
    }

    pub(crate) fn burst(&mut self, handle: EmitterHandle, count: u32) -> Result<(), String> {
        if handle.index() < self._emitter_states.len() as u32 {
            self._emitter_states[handle.index() as usize]._burst_count = count;
            Ok(())
        } else {
            Err(format!("Invalid emitter handle: {:?}", handle))
        }
    }

    #[cfg(test)]
    pub(crate) fn get_emitters(&self) -> &[EmitterConfig] {
        &self._emitters
    }

    // -- Update & dispatch --

    pub(crate) fn update(
        &mut self,
        delta_time: f32,
        frame_index: u32,
    ) -> Result<(u32, u32), String> {
        self._frame_count += 1;

        self.recompute_estimated_max_alive();

        let total_emit_count = self.calculate_emit_count(delta_time);

        let total_burst_count: u32 = self
            ._emitter_states
            .iter()
            .map(|state| state._burst_count)
            .sum();

        let total_this_frame = total_emit_count + total_burst_count;

        let fi = frame_slot(frame_index);

        {
            let active_emitter_count = self
                ._emitters
                .iter()
                .zip(self._emitter_states.iter())
                .filter(|(e, s)| e.emit_rate > 0.0 || s._burst_count > 0)
                .count() as u32;

            let total_simulate_count =
                self._estimated_max_alive + total_emit_count + total_burst_count;

            let frame_data = FrameData {
                delta_time,
                total_emit_count: total_emit_count + total_burst_count,
                emitter_count: active_emitter_count,
                random_seed: self._frame_count,
                total_simulate_count,
                burst_count: total_burst_count,
                frame_index,
                _pad: 0,
            };

            let ptr = self._frame_data_buffers[fi].map();
            unsafe {
                std::ptr::copy_nonoverlapping(
                    &frame_data as *const FrameData as *const u8,
                    ptr,
                    std::mem::size_of::<FrameData>(),
                );
            }
            self._frame_data_buffers[fi].unmap();
        }

        {
            if !self._emitters.is_empty() {
                let ptr = self._emitter_config_buffers[fi].map();
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        self._emitters.as_ptr() as *const u8,
                        ptr,
                        self._emitters.len() * std::mem::size_of::<EmitterConfig>(),
                    );
                }
                self._emitter_config_buffers[fi].unmap();
            }
        }

        {
            let prev_fi = (fi + FRAMES_IN_FLIGHT - 1) % FRAMES_IN_FLIGHT;
            let prev_ptr = self._counters_buffers[prev_fi].map() as *const ParticleCounters;
            let prev_counters = unsafe { *prev_ptr };
            self._counters_buffers[prev_fi].unmap();

            let new_counters = ParticleCounters {
                alive_count: 0,
                dead_count: prev_counters.dead_count,
                emit_count: prev_counters.alive_count,
                workgroups_finished: 0,
            };

            let ptr = self._counters_buffers[fi].map();
            unsafe {
                std::ptr::copy_nonoverlapping(
                    &new_counters as *const ParticleCounters as *const u8,
                    ptr,
                    std::mem::size_of::<ParticleCounters>(),
                );
            }
            self._counters_buffers[fi].unmap();
        }

        for state in &mut self._emitter_states {
            state._burst_count = 0;
        }

        let emit_count = total_emit_count + total_burst_count;
        if total_this_frame > 0 {
            self._total_emitted += total_this_frame as u64;
        }

        Ok((self._estimated_max_alive, emit_count))
    }

    #[cfg(test)]
    pub(crate) fn max_particles(&self) -> u32 {
        self._max_particles
    }

    #[cfg(test)]
    pub(crate) fn alive_count(&self) -> u32 {
        self._estimated_max_alive
    }

    #[cfg(test)]
    pub(crate) fn max_estimated_alive(&self) -> u32 {
        self._estimated_max_alive
    }

    #[cfg(test)]
    pub(crate) fn total_emitted(&self) -> u64 {
        self._total_emitted
    }

    fn calculate_emit_count(&mut self, delta_time: f32) -> u32 {
        let mut total_emit = 0u32;

        for (emitter, state) in self._emitters.iter().zip(self._emitter_states.iter_mut()) {
            if emitter.emit_rate > 0.0 {
                state._emit_accumulator += emitter.emit_rate * delta_time;
                let to_emit = state._emit_accumulator as u32;
                state._emit_accumulator -= to_emit as f32;
                total_emit += to_emit;
            }
        }

        total_emit
    }

    fn recompute_estimated_max_alive(&mut self) {
        self._estimated_max_alive = self
            ._emitters
            .iter()
            .filter(|e| e.emit_rate > 0.0)
            .map(|e| {
                let max_alive = e.emit_rate * e.base_lifetime * (1.0 + e.lifetime_variation);
                max_alive.ceil() as u32
            })
            .sum::<u32>()
            .min(self._max_particles);
    }

    // -- Render pipeline --

    /// Create the particle billboard render pipeline.
    ///
    /// The frame-uniform buffer is bound per-encode from the renderer (frame
    /// slots rotate). Blending matches the Vulkan particle pipeline
    /// (SrcAlpha/OneMinusSrcAlpha color, One/Zero alpha), depth testing uses
    /// the engine's inverse-Z convention with depth writes disabled.
    pub(crate) fn create_render_pipeline(
        &mut self,
        context: &MetalContext,
        shader_path: &str,
    ) -> Result<(), RendererError> {
        let wgsl_source = super::metal_renderer::read_shader(shader_path)?;
        let compiled = super::shader::compile_wgsl_to_metal(
            &context.device,
            &wgsl_source,
            &["vs_main", "fs_main"],
            super::shader::ShaderProfile::ParticleRender,
        )?;
        let vertex_fn =
            compiled.module.entry_points.get("vs_main").ok_or_else(|| {
                RendererError::InvalidOperation("Particle vs_main not found".into())
            })?;
        let fragment_fn =
            compiled.module.entry_points.get("fs_main").ok_or_else(|| {
                RendererError::InvalidOperation("Particle fs_main not found".into())
            })?;

        // Empty vertex descriptor: the billboard VS generates geometry from
        // vertex_index alone (no stage_in). The default PBR descriptor would
        // make validation demand attribute buffers the shader never reads.
        let empty_vertex_descriptor = objc2_metal::MTLVertexDescriptor::new();
        let pipeline = context.create_graphics_pipeline_with_vertex_descriptor(
            vertex_fn,
            Some(fragment_fn),
            &[objc2_metal::MTLPixelFormat::RGBA16Float],
            Some(objc2_metal::MTLPixelFormat::Depth32Float_Stencil8),
            false,
            CompareOp::GreaterOrEqual,
            objc2_metal::MTLCullMode::None,
            objc2_metal::MTLWinding::Clockwise,
            Some(&empty_vertex_descriptor),
            true,
        )?;

        self.render_pipeline = Some(pipeline);
        Ok(())
    }

    pub(crate) fn render_pipeline(&self) -> Option<&super::pipeline::MetalGraphicsPipeline> {
        self.render_pipeline.as_ref()
    }

    // -- GPU dispatch --

    /// Encode the emit / simulate / draw-command dispatches for this frame.
    ///
    /// Runs inside the frame's command buffer (light-culling pattern), before
    /// any render pass, so the indirect draw command is fresh when the particle
    /// render record executes. Workgroup counts are CPU-derived exactly like
    /// the Vulkan graph path: emit covers this frame's emissions, simulate is
    /// over-dispatched from the emitter-based alive estimate, draw command is
    /// a single workgroup.
    ///
    /// `emit_workgroups == 0` skips the emit dispatch; simulate always runs at
    /// least one workgroup so the alive-list swap still happens.
    pub(crate) fn encode_compute(
        &self,
        encoder: &mut MetalComputeEncoder,
        frame_index: u32,
        emit_workgroups: u32,
        simulate_workgroups: u32,
    ) {
        let fi = frame_slot(frame_index);
        let prev_fi = (fi + FRAMES_IN_FLIGHT - 1) % FRAMES_IN_FLIGHT;
        let layout = &self._layout;

        if emit_workgroups > 0 {
            encoder.bind_compute_pipeline_raw(&self._emit_pipeline);
            encoder.bind_storage_buffer(&self._particle_buffer, 0, 0);
            encoder.bind_storage_buffer_at_offset(
                &self._particle_buffer,
                layout.dead_list_offset,
                0,
                1,
            );
            encoder.bind_storage_buffer_at_offset(
                &self._particle_buffer,
                layout._alive_frame_offset[fi],
                0,
                2,
            );
            encoder.bind_storage_buffer_at_offset(
                &self._particle_buffer,
                layout._alive_frame_offset[fi],
                0,
                3,
            );
            encoder.bind_storage_buffer(&self._counters_buffers[fi], 0, 4);
            encoder.bind_storage_buffer(&self._frame_data_buffers[fi], 0, 5);
            encoder.bind_storage_buffer(&self._emitter_config_buffers[fi], 0, 6);
            encoder.dispatch_raw(emit_workgroups.max(1), 1, 1, PARTICLE_EMIT_WORKGROUP_SIZE);
        }

        encoder.memory_barrier_buffers();

        encoder.bind_compute_pipeline_raw(&self._simulate_pipeline);
        encoder.bind_storage_buffer(&self._particle_buffer, 0, 0);
        encoder.bind_storage_buffer_at_offset(
            &self._particle_buffer,
            layout.dead_list_offset,
            0,
            1,
        );
        encoder.bind_storage_buffer_at_offset(
            &self._particle_buffer,
            layout._alive_frame_offset[fi],
            0,
            2,
        );
        encoder.bind_storage_buffer_at_offset(
            &self._particle_buffer,
            layout._alive_frame_offset[prev_fi],
            0,
            3,
        );
        encoder.bind_storage_buffer(&self._counters_buffers[fi], 0, 4);
        encoder.bind_storage_buffer(&self._frame_data_buffers[fi], 0, 5);
        encoder.bind_storage_buffer(&self._emitter_config_buffers[fi], 0, 6);
        encoder.dispatch_raw(
            simulate_workgroups.max(1),
            1,
            1,
            PARTICLE_SIMULATE_WORKGROUP_SIZE,
        );

        encoder.memory_barrier_buffers();

        encoder.bind_compute_pipeline_raw(&self._draw_command_pipeline);
        encoder.bind_storage_buffer(&self._counters_buffers[fi], 0, 0);
        encoder.bind_storage_buffer(&self._indirect_draw_buffers[fi], 0, 1);
        encoder.dispatch_raw(1, 1, 1, 1);
    }
}

impl crate::particles::particle_drive::ParticleEmitterDriver for MetalParticleSubsystem {
    fn create_emitter(&mut self, config: EmitterConfig) -> Result<EmitterHandle, String> {
        MetalParticleSubsystem::create_emitter(self, config)
    }

    fn update_emitter(&mut self, handle: EmitterHandle, config: EmitterConfig) {
        MetalParticleSubsystem::update_emitter(self, handle, config)
    }

    fn destroy_emitter(&mut self, handle: EmitterHandle, kill_all: bool) {
        MetalParticleSubsystem::destroy_emitter(self, handle, kill_all)
    }

    fn burst(&mut self, handle: EmitterHandle, count: u32) -> Result<(), String> {
        MetalParticleSubsystem::burst(self, handle, count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_max_particles_rejected() {
        let ctx = MetalContext::init_headless().unwrap();
        let result = MetalParticleSubsystem::new(&ctx, 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_exceeds_shader_limit_rejected() {
        let ctx = MetalContext::init_headless().unwrap();
        let result = MetalParticleSubsystem::new(&ctx, SHADER_MAX_PARTICLES + 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_valid_creation() {
        let ctx = MetalContext::init_headless().unwrap();
        let subsystem = MetalParticleSubsystem::new(&ctx, 1024).unwrap();
        assert_eq!(subsystem.max_particles(), 1024);
    }

    #[test]
    fn test_create_emitter() {
        let ctx = MetalContext::init_headless().unwrap();
        let mut subsystem = MetalParticleSubsystem::new(&ctx, 1024).unwrap();

        let config = EmitterConfig {
            position: [1.0, 2.0, 3.0],
            emit_rate: 100.0,
            base_lifetime: 2.0,
            ..Default::default()
        };
        let handle = subsystem.create_emitter(config).unwrap();
        assert_ne!(handle, EmitterHandle::NONE);

        let emitters = subsystem.get_emitters();
        assert_eq!(emitters.len(), 1);
        assert_eq!(emitters[0].position, [1.0, 2.0, 3.0]);
        assert_eq!(emitters[0].emit_rate, 100.0);
    }

    #[test]
    fn test_destroy_emitter() {
        let ctx = MetalContext::init_headless().unwrap();
        let mut subsystem = MetalParticleSubsystem::new(&ctx, 1024).unwrap();

        let config = EmitterConfig {
            emit_rate: 50.0,
            ..Default::default()
        };
        let handle = subsystem.create_emitter(config).unwrap();
        subsystem.destroy_emitter(handle, false);

        // Emitter should have zero emit_rate
        assert_eq!(subsystem.get_emitters()[0].emit_rate, 0.0);
    }

    #[test]
    fn test_burst() {
        let ctx = MetalContext::init_headless().unwrap();
        let mut subsystem = MetalParticleSubsystem::new(&ctx, 1024).unwrap();

        let config = EmitterConfig::default();
        let handle = subsystem.create_emitter(config).unwrap();
        assert!(subsystem.burst(handle, 100).is_ok());
    }

    #[test]
    fn test_update_emitter() {
        let ctx = MetalContext::init_headless().unwrap();
        let mut subsystem = MetalParticleSubsystem::new(&ctx, 1024).unwrap();

        let config = EmitterConfig {
            emit_rate: 50.0,
            ..Default::default()
        };
        let handle = subsystem.create_emitter(config).unwrap();

        let new_config = EmitterConfig {
            emit_rate: 200.0,
            base_lifetime: 3.0,
            ..Default::default()
        };
        subsystem.update_emitter(handle, new_config);
        assert_eq!(subsystem.get_emitters()[0].emit_rate, 200.0);
        assert_eq!(subsystem.get_emitters()[0].base_lifetime, 3.0);
    }

    #[test]
    fn test_update_no_emit_returns_zero() {
        let ctx = MetalContext::init_headless().unwrap();
        let mut subsystem = MetalParticleSubsystem::new(&ctx, 1024).unwrap();

        let (alive, emit) = subsystem.update(1.0 / 60.0, 0).unwrap();
        assert_eq!(emit, 0);
        assert!(alive <= 1024);
    }

    #[test]
    fn test_update_with_emitter() {
        let ctx = MetalContext::init_headless().unwrap();
        let mut subsystem = MetalParticleSubsystem::new(&ctx, 1024).unwrap();

        let config = EmitterConfig {
            emit_rate: 600.0,
            base_lifetime: 2.0,
            ..Default::default()
        };
        subsystem.create_emitter(config).unwrap();

        let (alive, emit) = subsystem.update(1.0 / 60.0, 0).unwrap();
        assert_eq!(emit, 10); // 600 * (1/60) = 10
        assert!(alive > 0);
    }

    #[test]
    fn test_emitter_config_size() {
        assert_eq!(std::mem::size_of::<EmitterConfig>(), 160);
    }

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
    fn test_buffer_layout() {
        let layout = BufferLayout::new(1024);
        assert!(layout.dead_list_offset > 0);
        assert!(layout._alive_offset > layout.dead_list_offset);
        assert!(layout._alive_frame_offset[1] > layout._alive_frame_offset[0]);
        assert_eq!(
            layout.total_size,
            layout._alive_frame_offset[1] + layout._alive_list_size
        );
    }
}

/// Encode the particle render record: camera-facing billboards alpha-blended
/// onto the HDR target, depth-tested (no write) against the prepass depth.
///
/// GPU-driven — the vertex stage expands `alive_count` quads via the indirect
/// draw command the draw-command dispatch wrote earlier this frame.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_particles(
    cmd_buffer: &mut super::command_buffer::MetalCommandBuffer,
    pipeline: &super::pipeline::MetalGraphicsPipeline,
    color_view: &super::texture::MetalTextureView,
    depth_view: &super::texture::MetalTextureView,
    width: u32,
    height: u32,
    frame_uniforms: &MetalBuffer,
    system: &MetalParticleSubsystem,
    frame_index: u32,
) {
    use crate::backend::command::{
        ColorAttachmentInfo, DepthAttachmentInfo, GpuRenderEncoder, RenderPassInfo, ShaderStages,
    };
    use crate::render_pass::{ClearValue, LoadOp, StoreOp};
    use crate::texture::ImageFormat;

    let fi = frame_slot(frame_index);
    let pass_info = RenderPassInfo {
        color_attachments: vec![ColorAttachmentInfo {
            view: color_view.clone(),
            load_op: LoadOp::Load,
            store_op: StoreOp::Store,
            clear_value: ClearValue::OPAQUE_BLACK,
        }],
        depth_attachment: Some(DepthAttachmentInfo {
            view: depth_view.clone(),
            load_op: LoadOp::Load,
            store_op: StoreOp::Store,
            clear_value: ClearValue::DepthStencil {
                depth: 0.0,
                stencil: 0,
            },
            format: ImageFormat::D32SfloatS8Uint,
        }),
        debug_label: Some("particles"),
    };

    let mut encoder = cmd_buffer.begin_render_pass(pass_info);
    encoder.set_viewport(0.0, 0.0, width as f32, height as f32, 0.0, 1.0);
    encoder.set_scissor(0, 0, width, height);

    // ParticleRender binding map: storage 0..4 stay flat, frame uniforms at 5
    // (slot 1 belongs to dead_list — double-binding it would clobber uniforms).
    encoder.bind_storage_buffer(&system._particle_buffer, 0, 0, ShaderStages::VERTEX);
    encoder.bind_storage_buffer(frame_uniforms, 0, 5, ShaderStages::VERTEX);
    encoder.bind_storage_buffer_at_offset_render(
        &system._particle_buffer,
        system._layout.dead_list_offset,
        0,
        1,
        ShaderStages::VERTEX,
    );
    encoder.bind_storage_buffer_at_offset_render(
        &system._particle_buffer,
        system._layout._alive_frame_offset[fi],
        0,
        2,
        ShaderStages::VERTEX,
    );
    encoder.bind_storage_buffer_at_offset_render(
        &system._particle_buffer,
        system._layout._alive_frame_offset[fi],
        0,
        3,
        ShaderStages::VERTEX,
    );
    encoder.bind_storage_buffer(&system._counters_buffers[fi], 0, 4, ShaderStages::VERTEX);

    encoder.bind_graphics_pipeline(pipeline);
    encoder.draw_indirect(&system._indirect_draw_buffers[fi], 0);
    encoder.end_encoding();
}
