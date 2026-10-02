//! Application-owned particle pool, emitter policy and graph workloads.

use katla_ecs::World;
use katla_gfx::ShaderStages;
use katla_gfx::particles::{
    DEFAULT_MAX_PARTICLES, EmitterConfig, EmitterHandle, FrameData, MAX_EMITTERS,
};
use katla_gfx::render_graph::{
    BufferAccess, BufferByteRange, BufferDesc, BufferMemoryPolicy, BufferUsages, ComputeBinding,
    ComputeCommand, ComputeDispatch, ComputeDispatchSize, ComputePipelineDesc, PassDesc, PassType,
    ResourceId,
};
use katla_gfx::renderer::frame_bindings::BufferBinding;
use katla_gfx::renderer::frame_scope::FrameToken;
use katla_gfx::{BufferHandle, GpuRenderer, ParticleEmitterDriver};

use crate::rendering::FrameUniforms;
use crate::resources::ResourceManager;
use crate::systems::ParticleSystem;
use crate::{AppError, AppResult, FrameGraph, Renderer};

const FRAME_SLOTS: usize = 3;
const EMITTER_BYTES: u64 = 160;
const PARTICLE_BYTES: u64 = 64;

#[derive(Clone, Copy, Default)]
struct EmitterState {
    burst: u32,
    accumulator: f32,
}

#[derive(Default)]
struct Emitters {
    configs: Vec<EmitterConfig>,
    states: Vec<EmitterState>,
    generations: Vec<u32>,
    occupied: Vec<bool>,
    free: Vec<u32>,
}

impl Emitters {
    fn reset(&mut self) {
        self.states.fill(EmitterState::default());
    }

    fn active_indices(&self) -> Vec<u32> {
        self.configs
            .iter()
            .zip(&self.states)
            .zip(&self.occupied)
            .enumerate()
            .filter_map(|(index, ((config, state), occupied))| {
                (*occupied && (config.emit_rate > 0.0 || state.burst > 0)).then_some(index as u32)
            })
            .collect()
    }
    fn live(&self, handle: EmitterHandle) -> Option<usize> {
        let slot = handle.index() as usize;
        (!handle.is_none()
            && self.generations.get(slot) == Some(&handle.generation())
            && self.occupied.get(slot) == Some(&true))
        .then_some(slot)
    }

    fn prepare(&self, delta: f32, capacity: u32) -> (Vec<EmitterState>, u32, u32) {
        let mut states = self.states.clone();
        let mut count = 0u32;
        let mut bursts = 0u32;
        for (config, state) in self.configs.iter().zip(&mut states) {
            if config.emit_rate.is_finite() && config.emit_rate > 0.0 {
                state.accumulator += config.emit_rate * delta.max(0.0);
                let emitted = state.accumulator as u32;
                state.accumulator -= emitted as f32;
                count = count.saturating_add(emitted);
            }
            bursts = bursts.saturating_add(state.burst);
            state.burst = 0;
        }
        (
            states,
            count.saturating_add(bursts).min(capacity),
            bursts.min(capacity),
        )
    }
}

#[derive(Clone, Copy)]
struct ParticleSlot {
    alive: BufferHandle,
    counters: BufferHandle,
    indirect: BufferHandle,
    simulate_dispatch: BufferHandle,
    frame: BufferHandle,
    emitters: BufferHandle,
    emitter_indices: BufferHandle,
    readback: BufferHandle,
    render_frame: BufferHandle,
}

#[derive(Clone, Copy)]
struct ParticleResources {
    data: ResourceId,
    dead: ResourceId,
    alive_read: ResourceId,
    alive_write: ResourceId,
    counters: ResourceId,
    previous_counters: ResourceId,
    indirect: ResourceId,
    simulate_dispatch: ResourceId,
    frame: ResourceId,
    emitters: ResourceId,
    emitter_indices: ResourceId,
    readback: ResourceId,
    render_frame: ResourceId,
    initial_indices: ResourceId,
}

struct PendingFrame {
    token: FrameToken,
    emitter_states: Vec<EmitterState>,
    emitted: u32,
    reset: bool,
}

#[derive(Clone, Copy)]
struct SubmittedStats {
    sequence: u64,
    requested_emissions: u64,
    dispatches: u64,
}

pub(crate) struct ParticleFeatures {
    max_particles: u32,
    data: BufferHandle,
    dead: BufferHandle,
    initial_indices: BufferHandle,
    initial_counters: BufferHandle,
    initial_upload_done: bool,
    slots: Vec<ParticleSlot>,
    resources: Option<ParticleResources>,
    emitters: Emitters,
    emit_pipeline: ComputePipelineDesc,
    simulate_pipeline: ComputePipelineDesc,
    draw_pipeline: ComputePipelineDesc,
    dispatch_pipeline: ComputePipelineDesc,
    previous_slot: Option<usize>,
    pending: Option<PendingFrame>,
    reset_requested: bool,
    frame_count: u32,
    total_emitted: u64,
    total_dispatches: u64,
    submitted_frames: u64,
    slot_stats: [Option<SubmittedStats>; FRAME_SLOTS],
    cached_stats: Option<crate::rendering::ParticleStats>,
}

impl ParticleFeatures {
    pub(crate) fn new(renderer: &mut Renderer, resources: &ResourceManager) -> AppResult<Self> {
        Self::with_capacity(renderer, resources, DEFAULT_MAX_PARTICLES)
    }

    fn with_capacity(
        renderer: &mut Renderer,
        resources: &ResourceManager,
        max_particles: u32,
    ) -> AppResult<Self> {
        if max_particles == 0 {
            return Err(other_error("Particle capacity must be nonzero"));
        }
        let common = std::fs::read_to_string(resources.shader_path("particles/common.wgsl"))?;
        let shader = |name| -> AppResult<ComputePipelineDesc> {
            let wgsl = std::fs::read_to_string(resources.shader_path(name))?
                .replace("#include \"common.wgsl\"", &common);
            let descriptor = ComputePipelineDesc {
                wgsl,
                entry: "cs_main".into(),
            };
            descriptor.interface().map_err(other_error)?;
            Ok(descriptor)
        };
        let emit_pipeline = shader("particles/particle_emit.wgsl")?;
        let simulate_pipeline = shader("particles/particle_simulate.wgsl")?;
        let draw_pipeline = shader("particles/particle_draw_command.wgsl")?;
        let dispatch_pipeline = ComputePipelineDesc {
            wgsl: format!(
                "@group(0) @binding(0) var<storage, read> counters: array<u32, 4>;\n\
             @group(0) @binding(1) var<storage, read_write> command: array<u32, 3>;\n\
             @compute @workgroup_size(1) fn cs_main() {{\n\
             command[0] = (min(counters[2], {max_particles}u) + 63u) / 64u;\n\
             command[1] = 1u; command[2] = 1u; }}"
            ),
            entry: "cs_main".into(),
        };
        let mut allocated = Vec::new();
        let result = (|| {
            let mut readbacks = Vec::with_capacity(FRAME_SLOTS);
            for _ in 0..FRAME_SLOTS {
                let readback = renderer
                    .create_buffer(BufferDesc::new(
                        16,
                        BufferUsages::READBACK | BufferUsages::TRANSFER_DESTINATION,
                        BufferMemoryPolicy::Readback,
                    ))
                    .map_err(graphics_error)?;
                allocated.push(readback);
                readbacks.push(readback);
            }
            let mut create = |size, cpu| {
                let handle = renderer
                    .create_buffer(buffer_desc(size, cpu))
                    .map_err(graphics_error)?;
                allocated.push(handle);
                Ok::<_, AppError>(handle)
            };
            let data = create(u64::from(max_particles) * PARTICLE_BYTES, false)?;
            let dead = create(u64::from(max_particles) * 4, false)?;
            let initial_indices = create(u64::from(max_particles) * 4, true)?;
            let initial_counters = create(16, true)?;
            let mut slots = Vec::with_capacity(FRAME_SLOTS);
            for readback in readbacks {
                slots.push(ParticleSlot {
                    alive: create(u64::from(max_particles) * 4, false)?,
                    counters: create(16, true)?,
                    indirect: create(16, false)?,
                    simulate_dispatch: create(12, false)?,
                    frame: create(32, true)?,
                    emitters: create(u64::from(MAX_EMITTERS) * EMITTER_BYTES, true)?,
                    emitter_indices: create(u64::from(MAX_EMITTERS) * 4, true)?,
                    readback,
                    render_frame: create(std::mem::size_of::<FrameUniforms>() as u64, true)?,
                });
            }
            Ok(Self {
                max_particles,
                data,
                dead,
                initial_indices,
                initial_counters,
                initial_upload_done: false,
                slots,
                resources: None,
                emitters: Emitters::default(),
                emit_pipeline,
                simulate_pipeline,
                draw_pipeline,
                dispatch_pipeline,
                previous_slot: None,
                pending: None,
                reset_requested: true,
                frame_count: 0,
                total_emitted: 0,
                total_dispatches: 0,
                submitted_frames: 0,
                slot_stats: [None; FRAME_SLOTS],
                cached_stats: None,
            })
        })();
        if result.is_err() {
            for handle in allocated {
                let _ = renderer.destroy_buffer(handle);
            }
        }
        result
    }

    pub(crate) fn install_graph(&mut self, graph: &mut FrameGraph) -> AppResult<()> {
        let slot = self.slots[0];
        let previous = self.slots[FRAME_SLOTS - 1];
        let readback = graph
            .import_buffer(
                "scene_particle_readback",
                slot.readback,
                BufferDesc::new(
                    16,
                    BufferUsages::READBACK | BufferUsages::TRANSFER_DESTINATION,
                    BufferMemoryPolicy::Readback,
                ),
            )
            .map_err(other_error)?;
        let mut import = |name, handle, size, cpu| {
            graph
                .import_buffer(name, handle, buffer_desc(size, cpu))
                .map_err(other_error)
        };
        self.resources = Some(ParticleResources {
            data: import(
                "scene_particle_data",
                self.data,
                u64::from(self.max_particles) * PARTICLE_BYTES,
                false,
            )?,
            dead: import(
                "scene_particle_dead",
                self.dead,
                u64::from(self.max_particles) * 4,
                false,
            )?,
            alive_read: import(
                "scene_particle_alive_read",
                previous.alive,
                u64::from(self.max_particles) * 4,
                false,
            )?,
            alive_write: import(
                "scene_particle_alive_write",
                slot.alive,
                u64::from(self.max_particles) * 4,
                false,
            )?,
            counters: import("scene_particle_counters", slot.counters, 16, true)?,
            previous_counters: import(
                "scene_particle_previous_counters",
                self.initial_counters,
                16,
                true,
            )?,
            indirect: import("scene_particle_indirect", slot.indirect, 16, false)?,
            simulate_dispatch: import(
                "scene_particle_simulate_dispatch",
                slot.simulate_dispatch,
                12,
                false,
            )?,
            frame: import("scene_particle_frame", slot.frame, 32, true)?,
            emitters: import(
                "scene_particle_emitters",
                slot.emitters,
                u64::from(MAX_EMITTERS) * EMITTER_BYTES,
                true,
            )?,
            emitter_indices: import(
                "scene_particle_emitter_indices",
                slot.emitter_indices,
                u64::from(MAX_EMITTERS) * 4,
                true,
            )?,
            readback,
            render_frame: import(
                "scene_particle_render_frame",
                slot.render_frame,
                std::mem::size_of::<FrameUniforms>() as u64,
                true,
            )?,
            initial_indices: import(
                "scene_particle_initial_indices",
                self.initial_indices,
                u64::from(self.max_particles) * 4,
                true,
            )?,
        });
        for name in [
            "particle_host_ready",
            "particle_readback",
            "particle_draw_command",
            "particle_simulate",
            "particle_simulate_dispatch",
            "particle_emit",
            "particle_rollover",
            "particle_initialize",
        ] {
            let pass = PassDesc::new(
                name,
                if matches!(
                    name,
                    "particle_rollover"
                        | "particle_initialize"
                        | "particle_readback"
                        | "particle_host_ready"
                ) {
                    PassType::Transfer
                } else {
                    PassType::Compute
                },
                vec![],
                vec![],
            );
            graph.insert_pass(
                0,
                if name == "particle_host_ready" {
                    pass.with_side_effect()
                } else {
                    pass
                },
            );
        }
        self.set_workload(graph, 0, true)?;
        if graph.pass_id("particles").is_some() {
            graph
                .extend_pass_buffer_accesses("particles", self.graphics_accesses()?)
                .map_err(other_error)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_frame(
        &mut self,
        renderer: &mut Renderer,
        graph: &mut FrameGraph,
        token: &FrameToken,
        world: &mut World,
        cpu: &mut ParticleSystem,
        delta: f32,
        frame_index: u32,
        uniforms: &FrameUniforms,
    ) -> AppResult<()> {
        self.refresh_stats(renderer)?;
        cpu.update(world, self, delta);
        let resources = self.resource_ids()?;
        let slot = *self
            .slots
            .get(token.slot())
            .ok_or_else(|| other_error("Invalid particle frame slot"))?;
        let previous = self.previous_slot.map(|index| self.slots[index]);
        let previous_alive = previous
            .unwrap_or(self.slots[(token.slot() + FRAME_SLOTS - 1) % FRAME_SLOTS])
            .alive;
        let previous_counters = if self.reset_requested {
            self.initial_counters
        } else {
            previous
                .ok_or_else(|| other_error("Missing committed particle frame"))?
                .counters
        };
        for (id, handle) in [
            (resources.alive_read, previous_alive),
            (resources.alive_write, slot.alive),
            (resources.counters, slot.counters),
            (resources.previous_counters, previous_counters),
            (resources.indirect, slot.indirect),
            (resources.simulate_dispatch, slot.simulate_dispatch),
            (resources.frame, slot.frame),
            (resources.emitters, slot.emitters),
            (resources.emitter_indices, slot.emitter_indices),
            (resources.readback, slot.readback),
            (resources.render_frame, slot.render_frame),
        ] {
            graph
                .rebind_imported_buffer(id, handle)
                .map_err(other_error)?;
        }
        if !self.initial_upload_done {
            let indices: Vec<u32> = (0..self.max_particles).collect();
            renderer
                .write_buffer(
                    token,
                    self.initial_indices,
                    0,
                    bytemuck::cast_slice(&indices),
                )
                .map_err(graphics_error)?;
            renderer
                .write_buffer(
                    token,
                    self.initial_counters,
                    0,
                    bytemuck::cast_slice(&[0u32, self.max_particles, 0, 0]),
                )
                .map_err(graphics_error)?;
            self.initial_upload_done = true;
        }
        let (states, emitted, burst) = self.emitters.prepare(delta, self.max_particles);
        let active = self.emitters.active_indices();
        let mut indices = vec![0u32; MAX_EMITTERS as usize];
        indices[..active.len()].copy_from_slice(&active);
        renderer
            .write_buffer(
                token,
                slot.emitter_indices,
                0,
                bytemuck::cast_slice(&indices),
            )
            .map_err(graphics_error)?;
        let frame = FrameData {
            delta_time: delta.max(0.0),
            total_emit_count: emitted,
            emitter_count: active.len() as u32,
            random_seed: self.frame_count.wrapping_add(1),
            total_simulate_count: self.max_particles,
            burst_count: burst,
            frame_index,
            max_particles: self.max_particles,
        };
        renderer
            .write_buffer(token, slot.frame, 0, bytemuck::bytes_of(&frame))
            .map_err(graphics_error)?;
        renderer
            .write_buffer(
                token,
                slot.emitters,
                0,
                &emitter_bytes(&self.emitters.configs),
            )
            .map_err(graphics_error)?;
        renderer
            .write_buffer(token, slot.render_frame, 0, &frame_bytes(uniforms))
            .map_err(graphics_error)?;
        self.set_workload(graph, emitted, self.reset_requested)?;
        self.pending = Some(PendingFrame {
            token: *token,
            emitter_states: states,
            emitted,
            reset: self.reset_requested,
        });
        Ok(())
    }

    pub(crate) fn committed(&mut self, token: &FrameToken) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        if pending.token != *token {
            self.pending = Some(pending);
            return;
        }
        self.previous_slot = Some(token.slot());
        self.emitters.states = pending.emitter_states;
        self.frame_count = self.frame_count.wrapping_add(1);
        self.total_emitted = self
            .total_emitted
            .saturating_add(u64::from(pending.emitted));
        self.submitted_frames = self.submitted_frames.saturating_add(1);
        self.total_dispatches = self
            .total_dispatches
            .saturating_add(3 + u64::from(pending.emitted > 0));
        self.slot_stats[token.slot()] = Some(SubmittedStats {
            sequence: self.submitted_frames,
            requested_emissions: self.total_emitted,
            dispatches: self.total_dispatches,
        });
        if pending.reset {
            self.reset_requested = false;
        }
    }

    pub(crate) fn reset_all(&mut self) {
        self.reset_requested = true;
        self.emitters.reset();
        self.pending = None;
        self.total_emitted = 0;
        self.cached_stats = None;
        self.slot_stats.fill(None);
    }

    #[cfg(feature = "editor")]
    pub(crate) fn stats(&self) -> Option<crate::rendering::ParticleStats> {
        self.cached_stats.clone()
    }

    pub(crate) fn refresh_stats(&mut self, renderer: &mut Renderer) -> AppResult<()> {
        let previous = self
            .cached_stats
            .as_ref()
            .map_or(0, |stats| stats.frame_count);
        let mut latest = None;
        for (slot, metadata) in self.slots.iter().zip(&self.slot_stats) {
            let Some(metadata) = metadata.filter(|metadata| metadata.sequence > previous) else {
                continue;
            };
            let Some(bytes) = renderer
                .read_buffer_completed(slot.readback, BufferByteRange::new(0, 16))
                .map_err(graphics_error)?
            else {
                continue;
            };
            let (alive, dead) = completed_counts(&bytes, self.max_particles)?;
            if latest
                .as_ref()
                .is_none_or(|stats: &crate::rendering::ParticleStats| {
                    stats.frame_count < metadata.sequence
                })
            {
                latest = Some(crate::rendering::ParticleStats {
                    max_alive_count: self.max_particles,
                    current_alive_count: alive,
                    dead_count: dead,
                    total_emitted: metadata.requested_emissions,
                    frame_count: metadata.sequence,
                    total_dispatches: metadata.dispatches,
                    memory_used_mb: (u64::from(self.max_particles) * (PARTICLE_BYTES + 20)
                        + 16
                        + FRAME_SLOTS as u64
                            * (u64::from(MAX_EMITTERS) * (EMITTER_BYTES + 4) + 412))
                        as f32
                        / 1_048_576.0,
                    buffer_utilization: alive as f32 / self.max_particles as f32,
                    ..crate::rendering::ParticleStats::default()
                });
            }
        }
        if let Some(stats) = latest {
            self.cached_stats = Some(stats);
        }
        Ok(())
    }

    pub(crate) fn graphics_bindings(&self) -> AppResult<Vec<BufferBinding>> {
        let resources = self.resource_ids()?;
        Ok([
            (
                0,
                0,
                resources.data,
                u64::from(self.max_particles) * PARTICLE_BYTES,
            ),
            (0, 1, resources.dead, u64::from(self.max_particles) * 4),
            (
                0,
                2,
                resources.alive_write,
                u64::from(self.max_particles) * 4,
            ),
            (
                0,
                3,
                resources.alive_write,
                u64::from(self.max_particles) * 4,
            ),
            (0, 4, resources.counters, 16),
            (
                1,
                0,
                resources.render_frame,
                std::mem::size_of::<FrameUniforms>() as u64,
            ),
        ]
        .into_iter()
        .map(|(group, binding, resource, size)| BufferBinding {
            group,
            binding,
            resource,
            range: BufferByteRange::new(0, size),
            stages: ShaderStages::VERTEX,
        })
        .collect())
    }

    pub(crate) fn graphics_accesses(&self) -> AppResult<Vec<BufferAccess>> {
        let mut accesses = particle_graphics_accesses(self.graphics_bindings()?);
        accesses.push(
            BufferAccess::indirect_read(self.resource_ids()?.indirect)
                .with_range(BufferByteRange::new(0, 16)),
        );
        Ok(accesses)
    }

    pub(crate) fn indirect_resource(&self) -> AppResult<ResourceId> {
        Ok(self.resource_ids()?.indirect)
    }

    fn resource_ids(&self) -> AppResult<ParticleResources> {
        self.resources
            .ok_or_else(|| other_error("Particle resources are not installed"))
    }

    fn set_workload(&self, graph: &mut FrameGraph, emitted: u32, reset: bool) -> AppResult<()> {
        let r = self.resource_ids()?;
        let mut set = |name: &str, commands: Vec<ComputeCommand>, accesses| {
            let pass = graph
                .pass_id(name)
                .ok_or_else(|| other_error(format!("Missing particle pass '{name}'")))?;
            graph
                .set_pass_commands(pass, commands, accesses)
                .map_err(other_error)
        };
        let init_commands = if reset {
            vec![
                ComputeCommand::FillBuffer {
                    resource: r.data,
                    range: BufferByteRange::new(0, u64::from(self.max_particles) * PARTICLE_BYTES),
                    value: 0,
                },
                ComputeCommand::CopyBuffer {
                    source: r.initial_indices,
                    destination: r.dead,
                    source_offset: 0,
                    destination_offset: 0,
                    size: u64::from(self.max_particles) * 4,
                },
                ComputeCommand::FillBuffer {
                    resource: r.alive_write,
                    range: BufferByteRange::new(0, u64::from(self.max_particles) * 4),
                    value: 0,
                },
            ]
        } else {
            vec![]
        };
        set(
            "particle_initialize",
            init_commands,
            if reset {
                vec![
                    BufferAccess::transfer_write(r.data).with_range(BufferByteRange::new(
                        0,
                        u64::from(self.max_particles) * PARTICLE_BYTES,
                    )),
                    BufferAccess::transfer_read(r.initial_indices)
                        .with_range(BufferByteRange::new(0, u64::from(self.max_particles) * 4)),
                    BufferAccess::transfer_write(r.dead)
                        .with_range(BufferByteRange::new(0, u64::from(self.max_particles) * 4)),
                    BufferAccess::transfer_write(r.alive_write)
                        .with_range(BufferByteRange::new(0, u64::from(self.max_particles) * 4)),
                ]
            } else {
                vec![]
            },
        )?;
        set(
            "particle_rollover",
            vec![
                ComputeCommand::CopyBuffer {
                    source: r.previous_counters,
                    destination: r.counters,
                    source_offset: 0,
                    destination_offset: 8,
                    size: 4,
                },
                ComputeCommand::CopyBuffer {
                    source: r.previous_counters,
                    destination: r.counters,
                    source_offset: 4,
                    destination_offset: 4,
                    size: 4,
                },
                ComputeCommand::FillBuffer {
                    resource: r.counters,
                    range: BufferByteRange::new(0, 4),
                    value: 0,
                },
                ComputeCommand::FillBuffer {
                    resource: r.counters,
                    range: BufferByteRange::new(12, 4),
                    value: 0,
                },
            ],
            vec![
                BufferAccess::transfer_read(r.previous_counters)
                    .with_range(BufferByteRange::new(0, 8)),
                BufferAccess::transfer_write(r.counters).with_range(BufferByteRange::new(0, 16)),
            ],
        )?;
        let bindings = [
            (0, 0, r.data, u64::from(self.max_particles) * PARTICLE_BYTES),
            (0, 1, r.dead, u64::from(self.max_particles) * 4),
            (0, 2, r.alive_read, u64::from(self.max_particles) * 4),
            (0, 3, r.alive_write, u64::from(self.max_particles) * 4),
            (0, 4, r.counters, 16),
            (1, 0, r.frame, 32),
            (1, 1, r.emitters, u64::from(MAX_EMITTERS) * EMITTER_BYTES),
        ];
        let mut emit_bindings = bindings.to_vec();
        emit_bindings.push((1, 2, r.emitter_indices, u64::from(MAX_EMITTERS) * 4));
        let emit = dispatch(
            &self.emit_pipeline,
            &emit_bindings,
            ComputeDispatchSize::Direct([emitted.div_ceil(256), 1, 1]),
        );
        set(
            "particle_emit",
            vec![ComputeCommand::Dispatch(emit.clone())],
            emit.accesses().map_err(other_error)?,
        )?;
        let groups = dispatch(
            &self.dispatch_pipeline,
            &[(0, 0, r.counters, 16), (0, 1, r.simulate_dispatch, 12)],
            ComputeDispatchSize::Direct([1, 1, 1]),
        );
        set(
            "particle_simulate_dispatch",
            vec![ComputeCommand::Dispatch(groups.clone())],
            groups.accesses().map_err(other_error)?,
        )?;
        let simulate = dispatch(
            &self.simulate_pipeline,
            &bindings,
            ComputeDispatchSize::Indirect {
                resource: r.simulate_dispatch,
                offset: 0,
            },
        );
        let mut accesses = simulate.accesses().map_err(other_error)?;
        accesses.push(
            BufferAccess::indirect_read(r.simulate_dispatch)
                .with_range(BufferByteRange::new(0, 12)),
        );
        set(
            "particle_simulate",
            vec![ComputeCommand::Dispatch(simulate)],
            accesses,
        )?;
        let draw = dispatch(
            &self.draw_pipeline,
            &[(0, 0, r.counters, 16), (0, 1, r.indirect, 16)],
            ComputeDispatchSize::Direct([1, 1, 1]),
        );
        set(
            "particle_draw_command",
            vec![ComputeCommand::Dispatch(draw.clone())],
            draw.accesses().map_err(other_error)?,
        )?;
        set(
            "particle_readback",
            vec![ComputeCommand::CopyBuffer {
                source: r.counters,
                destination: r.readback,
                source_offset: 0,
                destination_offset: 0,
                size: 16,
            }],
            vec![
                BufferAccess::transfer_read(r.counters).with_range(BufferByteRange::new(0, 16)),
                BufferAccess::transfer_write(r.readback).with_range(BufferByteRange::new(0, 16)),
            ],
        )?;
        set(
            "particle_host_ready",
            vec![],
            vec![BufferAccess::readback_read(r.readback).with_range(BufferByteRange::new(0, 16))],
        )
    }
}

impl ParticleEmitterDriver for Emitters {
    fn create_emitter(&mut self, config: EmitterConfig) -> Result<EmitterHandle, String> {
        let index = match self.free.pop() {
            Some(index) => index,
            None if self.configs.len() < MAX_EMITTERS as usize => {
                let index = self.configs.len() as u32;
                self.configs.push(config);
                self.states.push(EmitterState::default());
                self.generations.push(0);
                self.occupied.push(true);
                index
            }
            None => return Err(format!("Maximum emitter count ({MAX_EMITTERS}) reached")),
        };
        self.configs[index as usize] = config;
        self.states[index as usize] = EmitterState::default();
        self.occupied[index as usize] = true;
        Ok(EmitterHandle::from_raw(
            index,
            self.generations[index as usize],
        ))
    }

    fn update_emitter(&mut self, handle: EmitterHandle, config: EmitterConfig) {
        if let Some(slot) = self.live(handle) {
            self.configs[slot] = config;
        }
    }

    fn destroy_emitter(&mut self, handle: EmitterHandle, kill_all: bool) {
        if let Some(slot) = self.live(handle) {
            self.configs[slot] = EmitterConfig {
                emit_rate: 0.0,
                kill_all: u32::from(kill_all),
                ..Default::default()
            };
            self.states[slot] = EmitterState::default();
            self.occupied[slot] = false;
            if let Some(generation) = self.generations[slot].checked_add(1) {
                self.generations[slot] = generation;
                self.free.push(handle.index());
            } else {
                self.generations[slot] = u32::MAX;
            }
        }
    }

    fn burst(&mut self, handle: EmitterHandle, count: u32) -> Result<(), String> {
        let slot = self
            .live(handle)
            .ok_or_else(|| format!("Invalid emitter handle: {handle:?}"))?;
        self.states[slot].burst = self.states[slot].burst.saturating_add(count);
        Ok(())
    }
}

impl ParticleEmitterDriver for ParticleFeatures {
    fn create_emitter(&mut self, config: EmitterConfig) -> Result<EmitterHandle, String> {
        self.emitters.create_emitter(config)
    }
    fn update_emitter(&mut self, handle: EmitterHandle, config: EmitterConfig) {
        self.emitters.update_emitter(handle, config);
    }
    fn destroy_emitter(&mut self, handle: EmitterHandle, kill_all: bool) {
        self.emitters.destroy_emitter(handle, kill_all);
    }
    fn burst(&mut self, handle: EmitterHandle, count: u32) -> Result<(), String> {
        self.emitters.burst(handle, count)
    }
}

fn buffer_desc(size: u64, cpu: bool) -> BufferDesc {
    BufferDesc::new(
        size,
        BufferUsages::STORAGE
            | BufferUsages::UNIFORM
            | BufferUsages::INDIRECT
            | BufferUsages::TRANSFER_SOURCE
            | BufferUsages::TRANSFER_DESTINATION,
        if cpu {
            BufferMemoryPolicy::CpuVisible
        } else {
            BufferMemoryPolicy::DeviceLocal
        },
    )
}

fn dispatch(
    pipeline: &ComputePipelineDesc,
    bindings: &[(u32, u32, ResourceId, u64)],
    size: ComputeDispatchSize,
) -> ComputeDispatch {
    ComputeDispatch {
        pipeline: pipeline.clone(),
        bindings: bindings
            .iter()
            .map(|&(group, binding, resource, size)| ComputeBinding {
                group,
                binding,
                resource,
                range: BufferByteRange::new(0, size),
            })
            .collect(),
        constants: vec![],
        size,
    }
}

fn emitter_bytes(configs: &[EmitterConfig]) -> Vec<u8> {
    let mut bytes = vec![0; MAX_EMITTERS as usize * EMITTER_BYTES as usize];
    for (config, target) in configs
        .iter()
        .zip(bytes.as_chunks_mut::<{ EMITTER_BYTES as usize }>().0)
    {
        target.copy_from_slice(&config.gpu_bytes());
    }
    bytes
}

fn particle_graphics_accesses(bindings: Vec<BufferBinding>) -> Vec<BufferAccess> {
    bindings
        .into_iter()
        .map(|binding| {
            BufferAccess::new(
                binding.resource,
                katla_gfx::render_graph::ResourceAccessMode::Read,
                katla_gfx::render_graph::BufferUsage::Storage,
                katla_gfx::render_graph::ResourceAccessStage::VertexShader,
                binding.range,
            )
        })
        .collect()
}

fn frame_bytes(frame: &FrameUniforms) -> Vec<u8> {
    bytemuck::bytes_of(frame).to_vec()
}

fn completed_counts(bytes: &[u8], capacity: u32) -> AppResult<(u32, u32)> {
    if bytes.len() != 16 {
        return Err(other_error(
            "Particle counter readback must contain four integers",
        ));
    }
    let alive = u32::from_ne_bytes(bytes[0..4].try_into().map_err(other_error)?);
    let dead = u32::from_ne_bytes(bytes[4..8].try_into().map_err(other_error)?);
    if alive > capacity || dead > capacity {
        return Err(other_error(
            "Completed particle counters exceed pool capacity",
        ));
    }
    Ok((alive, dead))
}

fn graphics_error(source: katla_gfx::RendererError) -> AppError {
    AppError::Graphics { source }
}
fn other_error(error: impl std::fmt::Display) -> AppError {
    AppError::RendererInitFailed {
        reason: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending_submission() -> (ParticleFeatures, FrameToken, EmitterHandle) {
        let mut emitters = Emitters::default();
        let handle = emitters
            .create_emitter(EmitterConfig {
                emit_rate: 10.0,
                ..Default::default()
            })
            .unwrap();
        emitters.states[handle.index() as usize].accumulator = 0.5;
        emitters.burst(handle, 7).unwrap();
        let (emitter_states, emitted, _) = emitters.prepare(0.1, 64);
        let token = FrameToken::new(2);
        let unused = BufferHandle::from_raw(100, 0);
        let slots = (0..FRAME_SLOTS)
            .map(|index| ParticleSlot {
                alive: BufferHandle::from_raw(index as u32, 0),
                counters: BufferHandle::from_raw(index as u32 + 3, 0),
                indirect: unused,
                simulate_dispatch: unused,
                frame: unused,
                emitters: unused,
                emitter_indices: unused,
                readback: unused,
                render_frame: unused,
            })
            .collect();
        let unused_pipeline = ComputePipelineDesc {
            wgsl: String::new(),
            entry: "cs_main".into(),
        };
        (
            ParticleFeatures {
                max_particles: 64,
                data: unused,
                dead: unused,
                initial_indices: unused,
                initial_counters: unused,
                initial_upload_done: true,
                slots,
                resources: None,
                emitters,
                emit_pipeline: unused_pipeline.clone(),
                simulate_pipeline: unused_pipeline.clone(),
                draw_pipeline: unused_pipeline.clone(),
                dispatch_pipeline: unused_pipeline,
                previous_slot: Some(0),
                pending: Some(PendingFrame {
                    token,
                    emitter_states,
                    emitted,
                    reset: true,
                }),
                reset_requested: true,
                frame_count: 5,
                total_emitted: 20,
                total_dispatches: 17,
                submitted_frames: 5,
                slot_stats: [None; FRAME_SLOTS],
                cached_stats: None,
            },
            token,
            handle,
        )
    }

    fn assert_committed_submission(features: &ParticleFeatures, token: FrameToken) {
        assert_eq!(features.previous_slot, Some(token.slot()));
        let previous = features.slots[features.previous_slot.unwrap()];
        assert_eq!(previous.alive, features.slots[token.slot()].alive);
        assert_eq!(previous.counters, features.slots[token.slot()].counters);
        assert!(features.pending.is_none());
        assert!(!features.reset_requested);
        assert_eq!(features.emitters.states[0].burst, 0);
        assert_eq!(features.emitters.states[0].accumulator, 0.5);
        assert_eq!(features.emitters.prepare(0.1, 64).1, 1);
        assert_eq!(features.total_emitted, 28);
        assert_eq!(features.frame_count, 6);
        let stats = features.slot_stats[token.slot()].unwrap();
        assert_eq!(stats.sequence, 6);
        assert_eq!(stats.requested_emissions, 28);
    }

    #[test]
    fn test_particle_frame_commits_when_submitted_surface_needs_recreation() {
        use katla_gfx::renderer::frame_scope::{PresentOutcome, SurfaceStatus};
        let (mut features, token, _) = pending_submission();
        let result = crate::application::renderer::commit_frame_submission(
            Ok(PresentOutcome {
                surface: Ok(SurfaceStatus::RecreateRequired),
            }),
            || features.committed(&token),
        );
        assert!(matches!(result, Ok(SurfaceStatus::RecreateRequired)));
        assert_committed_submission(&features, token);
    }

    #[test]
    fn test_particle_frame_commits_before_reporting_submitted_surface_failure() {
        use katla_gfx::renderer::frame_scope::PresentOutcome;
        let (mut features, token, _) = pending_submission();
        let result = crate::application::renderer::commit_frame_submission(
            Ok(PresentOutcome {
                surface: Err(katla_gfx::RendererError::SwapchainError(
                    "surface lost".into(),
                )),
            }),
            || features.committed(&token),
        );
        assert!(matches!(
            result,
            Err(katla_gfx::RendererError::SwapchainError(_))
        ));
        assert_committed_submission(&features, token);
    }

    #[test]
    fn test_particle_frame_rejected_submission_preserves_retry_state() {
        let (mut features, token, handle) = pending_submission();
        let result = crate::application::renderer::commit_frame_submission(
            Err(katla_gfx::RendererError::InvalidOperation(
                "submit rejected".into(),
            )),
            || features.committed(&token),
        );
        assert!(result.is_err());
        assert_eq!(features.previous_slot, Some(0));
        assert_eq!(features.pending.as_ref().unwrap().token, token);
        assert!(features.reset_requested);
        assert_eq!(features.emitters.states[handle.index() as usize].burst, 7);
        assert_eq!(
            features.emitters.states[handle.index() as usize].accumulator,
            0.5
        );
        assert_eq!(features.emitters.prepare(0.1, 64).1, 8);
        assert_eq!(features.total_emitted, 20);
        assert_eq!(features.frame_count, 5);
        assert!(features.slot_stats[token.slot()].is_none());
    }

    #[test]
    fn test_particle_graphics_accesses_match_selected_shader_entries() {
        use katla_gfx::renderer::PipelineStages;
        use katla_gfx::renderer::frame_bindings::PassBindings;
        use katla_gfx::renderer::graphics_interface::GraphicsInterface;

        let source = include_str!("../../../../resources/shaders/particles/particle_render.wgsl")
            .replace(
                "#include \"common.wgsl\"",
                include_str!("../../../../resources/shaders/particles/common.wgsl"),
            )
            .replace(
                "#include \"../common/frame_uniforms.wgsl\"",
                include_str!("../../../../resources/shaders/common/frame_uniforms.wgsl"),
            );
        let interface = GraphicsInterface::reflect(
            &source,
            &PipelineStages::Graphics {
                vertex_entry: "vs_main".into(),
                fragment_entry: Some("fs_main".into()),
            },
        )
        .unwrap();
        let bindings: Vec<_> = [(0, 0, 64), (0, 2, 4), (1, 0, 320)]
            .into_iter()
            .enumerate()
            .map(|(index, (group, binding, size))| BufferBinding {
                group,
                binding,
                resource: ResourceId(index as u32),
                range: BufferByteRange::new(0, size),
                stages: ShaderStages::VERTEX,
            })
            .collect();
        let accesses = particle_graphics_accesses(bindings.clone());
        let packet = PassBindings {
            buffers: bindings,
            ..Default::default()
        };
        interface
            .validate_buffer_accesses(&packet, &accesses)
            .unwrap();
        let mut invalid = accesses;
        invalid[2].usage = katla_gfx::render_graph::BufferUsage::Uniform;
        assert!(
            interface
                .validate_buffer_accesses(&packet, &invalid)
                .is_err()
        );
    }

    #[test]
    fn test_emitter_capacity_and_recycled_generation() {
        let mut emitters = Emitters::default();
        let handles: Vec<_> = (0..MAX_EMITTERS)
            .map(|_| emitters.create_emitter(EmitterConfig::default()).unwrap())
            .collect();
        assert!(emitters.create_emitter(EmitterConfig::default()).is_err());
        let old = handles[511];
        emitters.destroy_emitter(old, true);
        let replacement = emitters.create_emitter(EmitterConfig::default()).unwrap();
        assert_eq!(replacement.index(), old.index());
        assert_eq!(replacement.generation(), old.generation() + 1);
        assert!(emitters.burst(old, 10).is_err());
        emitters.destroy_emitter(old, true);
        assert!(emitters.live(replacement).is_some());
        assert!(
            emitters
                .burst(EmitterHandle::from_raw(MAX_EMITTERS, 0), 1)
                .is_err()
        );
    }

    #[test]
    fn test_active_indices_preserve_holes_and_zero_rate_bursts() {
        let mut emitters = Emitters::default();
        let first = emitters.create_emitter(EmitterConfig::default()).unwrap();
        let hole = emitters.create_emitter(EmitterConfig::default()).unwrap();
        let burst = emitters
            .create_emitter(EmitterConfig {
                emit_rate: 0.0,
                ..Default::default()
            })
            .unwrap();
        emitters.destroy_emitter(hole, true);
        emitters.burst(burst, 7).unwrap();
        assert_eq!(emitters.active_indices(), [first.index(), burst.index()]);
        let (prepared, count, bursts) = emitters.prepare(0.0, 64);
        assert_eq!((count, bursts), (7, 7));
        emitters.states = prepared;
        assert_eq!(emitters.active_indices(), [first.index()]);
    }

    #[test]
    fn test_generation_exhaustion_retires_slot() {
        let mut emitters = Emitters::default();
        let initial = emitters.create_emitter(EmitterConfig::default()).unwrap();
        emitters.generations[initial.index() as usize] = u32::MAX;
        emitters.destroy_emitter(EmitterHandle::from_raw(initial.index(), u32::MAX), true);
        let next = emitters.create_emitter(EmitterConfig::default()).unwrap();
        assert_ne!(next.index(), initial.index());
        assert!(
            emitters
                .live(EmitterHandle::from_raw(initial.index(), u32::MAX))
                .is_none()
        );
    }

    #[test]
    fn test_reset_retains_emitters_and_clears_pending_emission() {
        let mut emitters = Emitters::default();
        let handle = emitters
            .create_emitter(EmitterConfig {
                emit_rate: 10.0,
                ..Default::default()
            })
            .unwrap();
        emitters.burst(handle, u32::MAX).unwrap();
        emitters.burst(handle, 20).unwrap();
        assert_eq!(emitters.prepare(0.1, 64).1, 64);
        emitters.states[handle.index() as usize].accumulator = 0.75;
        emitters.reset();
        assert!(emitters.live(handle).is_some());
        assert_eq!(emitters.prepare(0.1, 64).1, 1);
        assert_eq!(emitters.states[handle.index() as usize].accumulator, 0.0);
    }

    #[test]
    fn test_completed_counter_bytes_reject_corrupt_counts() {
        let mut bytes = [0u8; 16];
        bytes[0..4].copy_from_slice(&7u32.to_ne_bytes());
        bytes[4..8].copy_from_slice(&57u32.to_ne_bytes());
        assert_eq!(completed_counts(&bytes, 64).unwrap(), (7, 57));
        assert!(completed_counts(&bytes[..12], 64).is_err());
        bytes[4..8].copy_from_slice(&65u32.to_ne_bytes());
        assert!(completed_counts(&bytes, 64).is_err());
        bytes[4..8].copy_from_slice(&64u32.to_ne_bytes());
        assert_eq!(completed_counts(&bytes, 64).unwrap(), (7, 64));
    }
    #[test]
    fn test_emitter_upload_zeroes_alignment_padding() {
        let config = EmitterConfig {
            position: [1.0, 2.0, 3.0],
            kill_all: 1,
            ..Default::default()
        };
        let bytes = emitter_bytes(&[config]);
        assert_eq!(bytes.len(), MAX_EMITTERS as usize * 160);
        assert_eq!(&bytes[84..96], &[0; 12]);
        assert_eq!(&bytes[140..144], &1u32.to_ne_bytes());
        assert_eq!(&bytes[160..320], &[0; 160]);
        assert_eq!(frame_bytes(&FrameUniforms::default()).len(), 320);
    }
    #[test]
    fn test_uncommitted_emission_preserves_burst_and_fraction() {
        let emitters = Emitters {
            configs: vec![EmitterConfig {
                emit_rate: 10.0,
                ..Default::default()
            }],
            states: vec![EmitterState {
                accumulator: 0.5,
                burst: 7,
            }],
            generations: vec![0],
            occupied: vec![true],
            free: vec![],
        };
        let (states, count, burst) = emitters.prepare(0.1, 64);
        assert_eq!(count, 8);
        assert_eq!(burst, 7);
        assert_eq!(states[0].accumulator, 0.5);
        assert_eq!(emitters.states[0].burst, 7);
        assert_eq!(emitters.prepare(0.1, 64).1, 8);
    }
}
