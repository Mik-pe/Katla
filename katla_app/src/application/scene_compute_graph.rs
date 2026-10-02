//! Scene compute passes and their renderer-owned buffer dependencies.

use katla_gfx::render_graph::{
    BufferAccess, BufferByteRange, BufferDesc, BufferMemoryPolicy, BufferUsage, BufferUsages,
    BuiltinBuffer, BuiltinComputeKernel, ComputeBinding, ComputeCommand, ComputeDispatch,
    ComputeDispatchSize, ComputeKernel, PassDesc, PassType, ResourceAccessMode,
    ResourceAccessStage,
};

use super::Application;
use crate::AppError;

impl Application {
    pub(crate) fn install_scene_compute_graph(&mut self) -> Result<(), AppError> {
        use BuiltinBuffer::*;
        let mut buffers = std::collections::HashMap::new();
        for role in [
            AnimationParams,
            AnimationClips,
            AnimationChannels,
            AnimationTimes,
            AnimationValues,
            AnimationJoints,
            AnimationWorld,
            AnimationOutput,
            LightData,
            LightTiles,
            LightHeaders,
            LightFrame,
            ParticleData,
            ParticleDeadList,
            ParticleAliveRead,
            ParticleAliveWrite,
            ParticleCounters,
            ParticlePreviousCounters,
            ParticleIndirect,
            ParticleFrame,
            ParticleEmitters,
        ] {
            let usages = match role {
                LightFrame | ParticleFrame => BufferUsages::UNIFORM,
                ParticleIndirect => {
                    BufferUsages::STORAGE
                        | BufferUsages::INDIRECT
                        | BufferUsages::TRANSFER_DESTINATION
                }
                _ => {
                    BufferUsages::STORAGE
                        | BufferUsages::TRANSFER_SOURCE
                        | BufferUsages::TRANSFER_DESTINATION
                }
            };
            let id = self.frame_graph.import_builtin_buffer(
                format!("scene_{role:?}"),
                role,
                BufferDesc::new(u64::MAX, usages, BufferMemoryPolicy::DeviceLocal),
            );
            buffers.insert(role, id);
        }
        let id = |role| buffers[&role];
        let access = |role, mode, usage, stage| {
            BufferAccess::new(id(role), mode, usage, stage, BufferByteRange::WHOLE)
        };
        let dispatch = |kernel: BuiltinComputeKernel,
                        roles: &[(u32, u32, BuiltinBuffer)]|
         -> Result<PassDesc, AppError> {
            let dispatch = ComputeDispatch {
                kernel: ComputeKernel::Builtin(kernel),
                bindings: roles
                    .iter()
                    .map(|&(group, binding, role)| ComputeBinding {
                        group,
                        binding,
                        resource: id(role),
                        range: BufferByteRange::WHOLE,
                    })
                    .collect(),
                constants: Vec::new(),
                size: ComputeDispatchSize::Frame,
            };
            let accesses = dispatch
                .accesses()
                .map_err(|reason| AppError::RendererInitFailed { reason })?;
            Ok(PassDesc::new(
                match kernel {
                    BuiltinComputeKernel::AnimationPose => "animation_pose_eval",
                    BuiltinComputeKernel::LightCulling => "light_culling",
                    BuiltinComputeKernel::ParticleEmit => "particle_emit",
                    BuiltinComputeKernel::ParticleSimulate => "particle_simulate",
                    BuiltinComputeKernel::ParticleDrawCommand => "particle_draw_command",
                },
                PassType::Compute,
                vec![],
                vec![],
            )
            .with_buffer_accesses(accesses)
            .with_commands([ComputeCommand::Dispatch(dispatch)]))
        };
        let animation = dispatch(
            BuiltinComputeKernel::AnimationPose,
            &[
                (0, 0, AnimationParams),
                (0, 1, AnimationClips),
                (0, 2, AnimationChannels),
                (0, 3, AnimationTimes),
                (0, 4, AnimationValues),
                (0, 5, AnimationJoints),
                (0, 6, AnimationWorld),
                (0, 7, AnimationOutput),
            ],
        )?;
        let lights = dispatch(
            BuiltinComputeKernel::LightCulling,
            &[
                (0, 0, LightData),
                (0, 1, LightTiles),
                (0, 2, LightHeaders),
                (0, 3, LightFrame),
            ],
        )?;
        let particle_roles = [
            (0, 0, ParticleData),
            (0, 1, ParticleDeadList),
            (0, 2, ParticleAliveRead),
            (0, 3, ParticleAliveWrite),
            (0, 4, ParticleCounters),
            (1, 0, ParticleFrame),
            (1, 1, ParticleEmitters),
        ];
        let emit = dispatch(BuiltinComputeKernel::ParticleEmit, &particle_roles)?;
        let simulate = dispatch(BuiltinComputeKernel::ParticleSimulate, &particle_roles)?;
        let draw = dispatch(
            BuiltinComputeKernel::ParticleDrawCommand,
            &[(0, 0, ParticleCounters), (0, 1, ParticleIndirect)],
        )?;
        let rollover = PassDesc::new("particle_rollover", PassType::Transfer, vec![], vec![])
            .with_buffer_accesses([
                access(
                    ParticlePreviousCounters,
                    ResourceAccessMode::Read,
                    BufferUsage::TransferSource,
                    ResourceAccessStage::Transfer,
                ),
                access(
                    ParticleCounters,
                    ResourceAccessMode::Write,
                    BufferUsage::TransferDestination,
                    ResourceAccessStage::Transfer,
                ),
            ])
            .with_commands([
                ComputeCommand::CopyBuffer {
                    source: id(ParticlePreviousCounters),
                    destination: id(ParticleCounters),
                    source_offset: 0,
                    destination_offset: 8,
                    size: 4,
                },
                ComputeCommand::CopyBuffer {
                    source: id(ParticlePreviousCounters),
                    destination: id(ParticleCounters),
                    source_offset: 4,
                    destination_offset: 4,
                    size: 4,
                },
                ComputeCommand::FillBuffer {
                    resource: id(ParticleCounters),
                    range: BufferByteRange::new(0, 4),
                    value: 0,
                },
                ComputeCommand::FillBuffer {
                    resource: id(ParticleCounters),
                    range: BufferByteRange::new(12, 4),
                    value: 0,
                },
            ]);
        let light_clear = PassDesc::new("light_tiles_clear", PassType::Transfer, vec![], vec![])
            .with_buffer_accesses([access(
                LightHeaders,
                ResourceAccessMode::Write,
                BufferUsage::TransferDestination,
                ResourceAccessStage::Transfer,
            )])
            .with_commands([ComputeCommand::FillBuffer {
                resource: id(LightHeaders),
                range: BufferByteRange::WHOLE,
                value: 0,
            }]);
        let animation_copy = PassDesc::new(
            "animation_skeleton_copy",
            PassType::Transfer,
            vec![],
            vec![],
        )
        .with_commands([]);
        for pass in [
            animation,
            animation_copy,
            light_clear,
            lights,
            rollover,
            emit,
            simulate,
            draw,
        ]
        .into_iter()
        .rev()
        {
            self.frame_graph.insert_pass(0, pass);
        }
        if let Some(name) = &self.frame_graph_bindings.passes.geometry {
            self.frame_graph
                .extend_pass_buffer_accesses(
                    name,
                    vec![
                        access(
                            LightData,
                            ResourceAccessMode::Read,
                            BufferUsage::Storage,
                            ResourceAccessStage::FragmentShader,
                        ),
                        access(
                            LightTiles,
                            ResourceAccessMode::Read,
                            BufferUsage::Storage,
                            ResourceAccessStage::FragmentShader,
                        ),
                        access(
                            LightHeaders,
                            ResourceAccessMode::Read,
                            BufferUsage::Storage,
                            ResourceAccessStage::FragmentShader,
                        ),
                    ],
                )
                .map_err(graph_error)?;
        }
        if self.frame_graph.pass_id("particles").is_some() {
            self.frame_graph
                .extend_pass_buffer_accesses(
                    "particles",
                    vec![
                        access(
                            ParticleData,
                            ResourceAccessMode::Read,
                            BufferUsage::Storage,
                            ResourceAccessStage::VertexShader,
                        ),
                        access(
                            ParticleAliveWrite,
                            ResourceAccessMode::Read,
                            BufferUsage::Storage,
                            ResourceAccessStage::VertexShader,
                        ),
                        access(
                            ParticleCounters,
                            ResourceAccessMode::Read,
                            BufferUsage::Storage,
                            ResourceAccessStage::VertexShader,
                        ),
                        access(
                            ParticleFrame,
                            ResourceAccessMode::Read,
                            BufferUsage::Uniform,
                            ResourceAccessStage::VertexShader,
                        ),
                        access(
                            ParticleIndirect,
                            ResourceAccessMode::Read,
                            BufferUsage::Indirect,
                            ResourceAccessStage::DrawIndirect,
                        ),
                    ],
                )
                .map_err(graph_error)?;
        }
        self.pass_ids
            .refresh(&self.frame_graph, &self.frame_graph_bindings.passes)?;
        self.frame_graph
            .initialize_compute_pipelines(&mut self.renderer)
            .map_err(graph_error)?;
        Ok(())
    }
}

fn graph_error(error: katla_gfx::render_graph::RenderGraphError) -> AppError {
    AppError::RendererInitFailed {
        reason: error.to_string(),
    }
}
