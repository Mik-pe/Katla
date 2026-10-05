use super::*;

pub(super) struct BufferDiagnosticResource {
    pub(super) descriptor: BufferDesc,
    pub(super) origin: RenderGraphDiagnosticResourceOrigin,
}

impl RenderGraphDiagnostics {
    pub(super) fn from_parts(
        passes: &[PassDesc],
        resources: &[GraphResourceDesc],
        transient_resources: &[GraphResourceDesc],
        exported_resources: &BTreeSet<ResourceId>,
        imported_contracts: &BTreeMap<ResourceId, ImportedImageContract>,
        buffers: &BTreeMap<ResourceId, BufferDiagnosticResource>,
        plan: &ExecutionPlan,
    ) -> Self {
        let transient_by_name = transient_resources
            .iter()
            .map(|resource| (resource.name.as_str(), resource))
            .collect::<BTreeMap<_, _>>();
        let execution_positions = plan
            .sorted_passes
            .iter()
            .enumerate()
            .map(|(position, &pass)| (pass, position))
            .collect::<BTreeMap<_, _>>();
        let mut allocation_plan = TransientAllocationPlan::build(
            resources,
            transient_resources,
            exported_resources,
            &plan.resource_lifetimes,
            &plan.live_image_accesses,
        );

        allocation_plan.apply_attachment_storage(passes, &plan.resource_lifetimes);

        let diagnostic_resources = resources
            .iter()
            .enumerate()
            .map(|(index, namespace_resource)| {
                let descriptor = transient_by_name
                    .get(namespace_resource.name.as_str())
                    .copied();
                let buffer = buffers.get(&ResourceId(index as u32));
                let origin = if let Some(buffer) = buffer {
                    buffer.origin
                } else if namespace_resource.name == BACKBUFFER_NAME {
                    RenderGraphDiagnosticResourceOrigin::BuiltIn
                } else if descriptor.is_some() {
                    RenderGraphDiagnosticResourceOrigin::Transient
                } else {
                    RenderGraphDiagnosticResourceOrigin::Imported
                };

                RenderGraphDiagnosticResource {
                    id: index as u32,
                    name: namespace_resource.name.clone(),
                    origin,
                    kind: if buffer.is_some() {
                        Some("buffer".to_string())
                    } else {
                        descriptor.map(|resource| resource_kind(&resource.resource_type))
                    },
                    format: descriptor.map(|resource| format!("{:?}", resource.format)),
                    width: descriptor.map(|resource| resource.width),
                    height: descriptor.map(|resource| resource.height),
                    tracks_swapchain_size: descriptor
                        .map(|resource| resource.tracks_swapchain_size),
                    buffer: buffer.map(|buffer| buffer.descriptor.into()),
                    exported: exported_resources.contains(&ResourceId(index as u32)),
                    lifetime: plan
                        .resource_lifetimes
                        .get(&ResourceId(index as u32))
                        .copied()
                        .map(RenderGraphDiagnosticResourceLifetime::from),
                    live: plan
                        .resource_lifetimes
                        .contains_key(&ResourceId(index as u32))
                        || exported_resources.contains(&ResourceId(index as u32)),
                    cull_reason: (!plan
                        .resource_lifetimes
                        .contains_key(&ResourceId(index as u32))
                        && !exported_resources.contains(&ResourceId(index as u32)))
                    .then(|| "no live pass accesses this unexported resource".into()),
                    alias_predecessor: allocation_plan
                        .physical_allocation_id(ResourceId(index as u32))
                        .and_then(|id| allocation_plan.slots().iter().find(|slot| slot.id == id))
                        .and_then(|slot| {
                            slot.members
                                .iter()
                                .position(|member| member.0 == index as u32)
                                .and_then(|position| {
                                    position
                                        .checked_sub(1)
                                        .and_then(|prior| slot.members.get(prior))
                                })
                        })
                        .map(|id| id.0),
                    alias_successor: allocation_plan
                        .physical_allocation_id(ResourceId(index as u32))
                        .and_then(|id| allocation_plan.slots().iter().find(|slot| slot.id == id))
                        .and_then(|slot| {
                            slot.members
                                .iter()
                                .position(|member| member.0 == index as u32)
                                .and_then(|position| slot.members.get(position + 1))
                        })
                        .map(|id| id.0),
                    physical_allocation_id: allocation_plan
                        .physical_allocation_id(ResourceId(index as u32)),
                    persistence: allocation_plan.persistence(ResourceId(index as u32)).map(
                        |requirements| RenderGraphDiagnosticPersistence {
                            exported: requirements.exported,
                            crosses_render_pass: requirements.crosses_render_pass,
                            sampled: requirements.sampled,
                            storage: requirements.storage,
                            transfer_or_readback: requirements.transfer_or_readback,
                            tile_memory_eligible: requirements.tile_memory.is_eligible(),
                        },
                    ),
                    imported_contract: imported_contracts.get(&ResourceId(index as u32)).map(
                        |contract| RenderGraphDiagnosticImportedContract {
                            initial: format!("{:?}", contract.initial),
                            required_final: contract
                                .required_final
                                .map(|state| format!("{state:?}")),
                        },
                    ),
                }
            })
            .collect::<Vec<_>>();

        let diagnostic_passes = plan
            .dag
            .iter()
            .map(|node| {
                let pass = &passes[node.pass_index];
                RenderGraphDiagnosticPass {
                    index: node.pass_index,
                    name: pass.name.clone(),
                    pass_type: match pass.pass_type {
                        PassType::Graphics => RenderGraphDiagnosticPassType::Graphics,
                        PassType::Compute => RenderGraphDiagnosticPassType::Compute,
                        PassType::Transfer => RenderGraphDiagnosticPassType::Transfer,
                    },
                    queue: plan.sync.pass_boundaries[node.pass_index]
                        .as_ref()
                        .map(|boundary| format!("{:?}", boundary.queue).to_lowercase()),
                    encoder: plan.sync.pass_boundaries[node.pass_index]
                        .as_ref()
                        .map(|boundary| format!("{:?}", boundary.encoder).to_lowercase()),
                    external_upload_dependency: plan.sync.pass_boundaries[node.pass_index]
                        .as_ref()
                        .is_some_and(|boundary| boundary.external_upload_dependency),
                    alias_handoffs: resource_refs(
                        &plan.sync.alias_handoffs[node.pass_index],
                        resources,
                    ),
                    kind: pass.kind.map(|kind| format!("{kind:?}")),
                    reads: resource_refs(&node.reads, resources),
                    writes: resource_refs(&node.writes, resources),
                    image_accesses: pass
                        .image_accesses
                        .iter()
                        .copied()
                        .map(|access| diagnostic_image_access(access, resources))
                        .collect(),
                    buffer_accesses: pass
                        .buffer_accesses
                        .iter()
                        .copied()
                        .map(|access| diagnostic_buffer_access(access, resources))
                        .collect(),
                    color_attachments: pass
                        .color_attachments
                        .iter()
                        .map(|(resource, ops)| RenderGraphDiagnosticAttachmentOps {
                            resource: resource.0,
                            load: format!("{:?}", ops.load),
                            store: format!("{:?}", ops.store),
                            clear_value: match ops.clear_value {
                                crate::render_pass::ClearValue::Color(c) => format!("color {c:?}"),
                                crate::render_pass::ClearValue::DepthStencil { depth, stencil } => {
                                    format!("depth-stencil depth={depth} stencil={stencil}")
                                }
                            },
                        })
                        .collect(),
                    depth_attachment: pass.depth_attachment.map(|ops| {
                        RenderGraphDiagnosticDepthAttachmentOps {
                            depth_load: format!("{:?}", ops.depth.load),
                            depth_store: format!("{:?}", ops.depth.store),
                            stencil_load: format!("{:?}", ops.stencil.load),
                            stencil_store: format!("{:?}", ops.stencil.store),
                        }
                    }),
                    predecessors: node.predecessors.clone(),
                    successors: node.successors.clone(),
                    execution_position: execution_positions.get(&node.pass_index).copied(),
                    parallel_level: plan.live_passes[node.pass_index].then_some(node.level),
                    side_effect: pass.side_effect,
                    live: plan.live_passes[node.pass_index],
                    culled: !plan.live_passes[node.pass_index],
                    liveness_reason: if !plan.live_passes[node.pass_index] {
                        "not a producer of an exported resource or side-effect root".into()
                    } else if !plan.culling_enabled {
                        "culling disabled".into()
                    } else if pass.side_effect {
                        "side-effect root".into()
                    } else if plan
                        .final_export_writers
                        .values()
                        .any(|writers| writers.contains(&node.pass_index))
                    {
                        "final exported resource producer".into()
                    } else {
                        "required producer of a live pass".into()
                    },
                }
            })
            .collect::<Vec<_>>();

        let synchronization = transition_diagnostics(passes, resources, plan);
        let buffer_synchronization = buffer_sync_diagnostics(passes, resources, plan);
        let dependencies = dependency_diagnostics(passes, resources, plan);

        let transient_slots = allocation_plan
            .slots()
            .iter()
            .map(|slot| {
                let compatibility = slot.compatibility;
                let id = slot.id;
                RenderGraphDiagnosticAllocationSlot {
                    id,
                    resources: slot
                        .members
                        .iter()
                        .map(|&member| resource_ref(member, resources))
                        .collect(),
                    bytes: slot.bytes,
                    logical_bytes: allocation_plan.slot_logical_bytes(id).unwrap_or(slot.bytes),
                    saved_bytes: allocation_plan.slot_saved_bytes(id).unwrap_or(0),
                    compatibility: RenderGraphDiagnosticCompatibilityClass {
                        kind: compatibility.kind.label().to_string(),
                        format: format!("{:?}", compatibility.format),
                        width: compatibility.width,
                        height: compatibility.height,
                        tracks_swapchain_size: compatibility.tracks_swapchain_size,
                    },
                    tile_memory: RenderGraphDiagnosticTileMemory {
                        eligible: slot.tile_memory.is_eligible(),
                        reason: slot.tile_memory.reason().to_string(),
                    },
                    first_execution_position: slot.first_execution_position,
                    last_execution_position: slot.last_execution_position,
                }
            })
            .collect::<Vec<_>>();

        let summary = RenderGraphDiagnosticSummary {
            declared_passes: passes.len(),
            live_passes: plan.live_passes.iter().filter(|&&live| live).count(),
            culled_passes: plan.culled_passes.len(),
            resources: diagnostic_resources.len(),
            dependency_edges: dependencies.len(),
            synchronization_transitions: synchronization.len(),
            buffer_synchronization_ops: buffer_synchronization.len(),
            physical_transient_allocations: allocation_plan.physical_allocation_count(),
            logical_transient_bytes: allocation_plan.logical_bytes(),
            physical_transient_bytes: allocation_plan.physical_bytes(),
            transient_alias_savings_bytes: allocation_plan.saved_bytes(),
            tile_memory_eligible_bytes: allocation_plan.tile_memory_eligible_bytes(),
            parallel_levels: plan.parallel_groups.len(),
        };

        Self {
            schema_version: RENDER_GRAPH_DIAGNOSTICS_SCHEMA_VERSION,
            summary,
            culling_enabled: plan.culling_enabled,
            liveness_roots: plan.liveness_roots.clone(),
            resources: diagnostic_resources,
            passes: diagnostic_passes,
            external_image_producers: plan
                .sync
                .external_image_producers
                .iter()
                .map(|producer| RenderGraphDiagnosticExternalProducer {
                    queue: format!("{:?}", producer.queue).to_lowercase(),
                    encoder: format!("{:?}", producer.encoder).to_lowercase(),
                    access: diagnostic_image_access(producer.access, resources),
                })
                .collect(),
            dependencies,
            synchronization,
            buffer_synchronization,
            execution_order: plan.sorted_passes.clone(),
            parallel_groups: plan.parallel_groups.clone(),
            transient_slots,
            allocation_source: "compiler_projection".into(),
            native_allocations: Vec::new(),
        }
    }
}

fn diagnostic_image_access(
    access: ImageAccess,
    resources: &[GraphResourceDesc],
) -> RenderGraphDiagnosticImageAccess {
    let mode = diagnostic_access_mode(access.mode);
    let usage = match access.usage {
        ResourceAccessUsage::Sampled => RenderGraphDiagnosticResourceAccessUsage::Sampled,
        ResourceAccessUsage::ColorAttachment => {
            RenderGraphDiagnosticResourceAccessUsage::ColorAttachment
        }
        ResourceAccessUsage::DepthStencilAttachment => {
            RenderGraphDiagnosticResourceAccessUsage::DepthStencilAttachment
        }
        ResourceAccessUsage::Storage => RenderGraphDiagnosticResourceAccessUsage::Storage,
        ResourceAccessUsage::TransferSource => {
            RenderGraphDiagnosticResourceAccessUsage::TransferSource
        }
        ResourceAccessUsage::TransferDestination => {
            RenderGraphDiagnosticResourceAccessUsage::TransferDestination
        }
        ResourceAccessUsage::Present => RenderGraphDiagnosticResourceAccessUsage::Present,
    };
    let stage = diagnostic_access_stage(access.stage);

    RenderGraphDiagnosticImageAccess {
        resource: resource_ref(access.resource, resources),
        mode,
        usage,
        stage,
        range: diagnostic_subresource_range(access.range),
    }
}

fn diagnostic_access_mode(mode: ResourceAccessMode) -> RenderGraphDiagnosticResourceAccessMode {
    match mode {
        ResourceAccessMode::Read => RenderGraphDiagnosticResourceAccessMode::Read,
        ResourceAccessMode::Write => RenderGraphDiagnosticResourceAccessMode::Write,
        ResourceAccessMode::ReadWrite => RenderGraphDiagnosticResourceAccessMode::ReadWrite,
    }
}

fn diagnostic_access_stage(stage: ResourceAccessStage) -> RenderGraphDiagnosticImageStage {
    match stage {
        ResourceAccessStage::VertexInput => RenderGraphDiagnosticImageStage::VertexInput,
        ResourceAccessStage::DrawIndirect => RenderGraphDiagnosticImageStage::DrawIndirect,
        ResourceAccessStage::Host => RenderGraphDiagnosticImageStage::Host,
        ResourceAccessStage::VertexShader => RenderGraphDiagnosticImageStage::VertexShader,
        ResourceAccessStage::FragmentShader => RenderGraphDiagnosticImageStage::FragmentShader,
        ResourceAccessStage::ComputeShader => RenderGraphDiagnosticImageStage::ComputeShader,
        ResourceAccessStage::ColorAttachmentOutput => {
            RenderGraphDiagnosticImageStage::ColorAttachmentOutput
        }
        ResourceAccessStage::DepthStencil => RenderGraphDiagnosticImageStage::DepthStencil,
        ResourceAccessStage::Transfer => RenderGraphDiagnosticImageStage::Transfer,
        ResourceAccessStage::Present => RenderGraphDiagnosticImageStage::Present,
        ResourceAccessStage::AllGraphics => RenderGraphDiagnosticImageStage::AllGraphics,
    }
}

fn diagnostic_buffer_access(
    access: BufferAccess,
    resources: &[GraphResourceDesc],
) -> RenderGraphDiagnosticBufferAccess {
    let mode = diagnostic_access_mode(access.mode);
    let usage = match access.usage {
        BufferUsage::Uniform => RenderGraphDiagnosticBufferUsage::Uniform,
        BufferUsage::Storage => RenderGraphDiagnosticBufferUsage::Storage,
        BufferUsage::Vertex => RenderGraphDiagnosticBufferUsage::Vertex,
        BufferUsage::Index => RenderGraphDiagnosticBufferUsage::Index,
        BufferUsage::Indirect => RenderGraphDiagnosticBufferUsage::Indirect,
        BufferUsage::TransferSource => RenderGraphDiagnosticBufferUsage::TransferSource,
        BufferUsage::TransferDestination => RenderGraphDiagnosticBufferUsage::TransferDestination,
        BufferUsage::Readback => RenderGraphDiagnosticBufferUsage::Readback,
    };
    let stage = diagnostic_access_stage(access.stage);

    RenderGraphDiagnosticBufferAccess {
        resource: resource_ref(access.resource, resources),
        mode,
        usage,
        stage,
        range: RenderGraphDiagnosticBufferByteRange {
            offset: access.range.offset,
            size: access.range.size,
        },
    }
}

fn diagnostic_subresource_range(
    range: super::super::access::ImageSubresourceRange,
) -> RenderGraphDiagnosticImageSubresourceRange {
    RenderGraphDiagnosticImageSubresourceRange {
        aspects: range.aspects.names().map(str::to_string).collect(),
        base_mip_level: range.base_mip_level,
        mip_level_count: range.mip_level_count,
        base_array_layer: range.base_array_layer,
        array_layer_count: range.array_layer_count,
    }
}

fn transition_diagnostics(
    passes: &[PassDesc],
    resources: &[GraphResourceDesc],
    plan: &ExecutionPlan,
) -> Vec<RenderGraphDiagnosticTransition> {
    plan.sorted_passes
        .iter()
        .flat_map(|&pass_index| {
            plan.sync.pass_ops[pass_index]
                .iter()
                .map(move |op| diagnostic_transition(op, passes, resources, plan, Some(pass_index)))
        })
        .chain(
            plan.sync
                .final_ops
                .iter()
                .map(|op| diagnostic_transition(op, passes, resources, plan, None)),
        )
        .collect()
}

fn buffer_sync_diagnostics(
    passes: &[PassDesc],
    resources: &[GraphResourceDesc],
    plan: &ExecutionPlan,
) -> Vec<RenderGraphDiagnosticBufferSyncOp> {
    plan.sorted_passes
        .iter()
        .flat_map(|&pass_index| {
            plan.sync.pass_buffer_ops[pass_index].iter().map(move |op| {
                RenderGraphDiagnosticBufferSyncOp {
                    version: resource_version(op.resource, op.before_pass),
                    source_boundary: pass_boundary_label(plan, op.before_pass, "frame_start"),
                    destination_boundary: pass_boundary_label(plan, Some(pass_index), "frame_end"),
                    resource: resource_ref(op.resource, resources),
                    range: RenderGraphDiagnosticBufferByteRange {
                        offset: op.range.offset,
                        size: op.range.size,
                    },
                    before: buffer_sync_state_label(op.before),
                    after: buffer_sync_state_label(op.after),
                    before_pass: op.before_pass,
                    before_name: op
                        .before_pass
                        .and_then(|index| passes.get(index))
                        .map(|pass| pass.name.clone()),
                    pass: pass_index,
                    pass_name: passes
                        .get(pass_index)
                        .map(|pass| pass.name.clone())
                        .unwrap_or_default(),
                    hazard: match op.reason {
                        super::super::SyncReason::Hazard(kind) => Some(kind.into()),
                        _ => None,
                    },
                    reason: sync_reason_label(op.reason).to_string(),
                }
            })
        })
        .collect()
}

fn buffer_sync_state_label(state: BufferSyncState) -> String {
    match state {
        BufferSyncState::Undefined => "undefined".to_string(),
        BufferSyncState::Access { usage, stage, mode } => format!(
            "{} {usage:?} @ {stage:?}",
            match mode {
                ResourceAccessMode::Read => "read",
                ResourceAccessMode::Write => "write",
                ResourceAccessMode::ReadWrite => "read_write",
            }
        ),
    }
}

fn sync_reason_label(reason: super::super::SyncReason) -> &'static str {
    match reason {
        super::super::SyncReason::InitialUse => "initial_use",
        super::super::SyncReason::Hazard(_) => "hazard",
        super::super::SyncReason::StateChange => "state_change",
        super::super::SyncReason::ImportedFinal => "imported_final",
    }
}

fn resource_version(resource: ResourceId, producer: Option<usize>) -> String {
    producer.map_or_else(
        || format!("r{}.initial", resource.0),
        |pass| format!("r{}.access.{pass}", resource.0),
    )
}

pub(super) fn pass_boundary_label(
    plan: &ExecutionPlan,
    index: Option<usize>,
    absent: &str,
) -> String {
    index
        .and_then(|index| plan.sync.pass_boundaries.get(index))
        .and_then(Option::as_ref)
        .map(|boundary| format!("{:?}/{:?}", boundary.queue, boundary.encoder).to_lowercase())
        .unwrap_or_else(|| absent.into())
}

fn diagnostic_transition(
    op: &ImageSyncOp,
    passes: &[PassDesc],
    resources: &[GraphResourceDesc],
    plan: &ExecutionPlan,
    to_pass: Option<usize>,
) -> RenderGraphDiagnosticTransition {
    let hazard = match op.reason {
        super::super::SyncReason::Hazard(kind) => Some(kind.into()),
        _ => None,
    };
    let reason = match op.reason {
        super::super::SyncReason::InitialUse => RenderGraphDiagnosticSyncReason::InitialUse,
        super::super::SyncReason::Hazard(_) => RenderGraphDiagnosticSyncReason::Hazard,
        super::super::SyncReason::StateChange => RenderGraphDiagnosticSyncReason::StateChange,
        super::super::SyncReason::ImportedFinal => RenderGraphDiagnosticSyncReason::ImportedFinal,
    };

    RenderGraphDiagnosticTransition {
        version: resource_version(op.resource, op.before_pass),
        source_boundary: pass_boundary_label(plan, op.before_pass, "frame_start"),
        destination_boundary: pass_boundary_label(plan, to_pass, "frame_end"),
        resource: resource_ref(op.resource, resources),
        range: diagnostic_subresource_range(op.range),
        before_pass: op.before_pass,
        before_name: op.before_pass.map(|pass| passes[pass].name.clone()),
        to_pass,
        to_name: to_pass.map(|pass| passes[pass].name.clone()),
        before_state: diagnostic_sync_state(op.before),
        after_state: diagnostic_sync_state(op.after),
        hazard,
        reason,
    }
}

fn diagnostic_sync_state(state: ImageSyncState) -> RenderGraphDiagnosticSyncState {
    let usage = |usage: super::super::access::ResourceAccessUsage| match usage {
        ResourceAccessUsage::Sampled => RenderGraphDiagnosticResourceAccessUsage::Sampled,
        ResourceAccessUsage::ColorAttachment => {
            RenderGraphDiagnosticResourceAccessUsage::ColorAttachment
        }
        ResourceAccessUsage::DepthStencilAttachment => {
            RenderGraphDiagnosticResourceAccessUsage::DepthStencilAttachment
        }
        ResourceAccessUsage::Storage => RenderGraphDiagnosticResourceAccessUsage::Storage,
        ResourceAccessUsage::TransferSource => {
            RenderGraphDiagnosticResourceAccessUsage::TransferSource
        }
        ResourceAccessUsage::TransferDestination => {
            RenderGraphDiagnosticResourceAccessUsage::TransferDestination
        }
        ResourceAccessUsage::Present => RenderGraphDiagnosticResourceAccessUsage::Present,
    };
    let stage = diagnostic_access_stage;
    let mode = |mode: super::super::access::ResourceAccessMode| match mode {
        ResourceAccessMode::Read => RenderGraphDiagnosticResourceAccessMode::Read,
        ResourceAccessMode::Write => RenderGraphDiagnosticResourceAccessMode::Write,
        ResourceAccessMode::ReadWrite => RenderGraphDiagnosticResourceAccessMode::ReadWrite,
    };

    match state {
        ImageSyncState::Undefined => RenderGraphDiagnosticSyncState::Undefined,
        ImageSyncState::Access {
            usage: u,
            stage: st,
            mode: m,
        } => RenderGraphDiagnosticSyncState::Access {
            usage: usage(u),
            stage: stage(st),
            mode: mode(m),
        },
    }
}

fn dependency_diagnostics(
    passes: &[PassDesc],
    resources: &[GraphResourceDesc],
    plan: &ExecutionPlan,
) -> Vec<RenderGraphDiagnosticDependency> {
    let mut grouped = BTreeMap::<(usize, usize), Vec<RenderGraphDiagnosticHazard>>::new();

    // The DAG (not the sync plan) enumerates every edge: an intervening
    // access can subsume a transitive hazard in the sync operations while
    // the edge still orders the two passes.
    for node in &plan.dag {
        for &from_pass in &node.predecessors {
            let to_pass = node.pass_index;
            let from = &passes[from_pass];
            let to = &passes[to_pass];
            for resource in from.writes.iter().chain(&from.reads).chain(&to.writes) {
                let raw = from.writes.contains(resource) && to.reads.contains(resource);
                let war = from.reads.contains(resource) && to.writes.contains(resource);
                let waw = from.writes.contains(resource) && to.writes.contains(resource);
                let kinds = [
                    raw.then_some(RenderGraphHazardKind::Raw),
                    war.then_some(RenderGraphHazardKind::War),
                    waw.then_some(RenderGraphHazardKind::Waw),
                ];
                for kind in kinds.into_iter().flatten() {
                    grouped.entry((from_pass, to_pass)).or_default().push(
                        RenderGraphDiagnosticHazard {
                            kind,
                            resource: resource_ref(*resource, resources),
                        },
                    );
                }
            }
        }
    }

    grouped
        .into_iter()
        .map(|((from_pass, to_pass), mut hazards)| {
            hazards.sort_by(|left, right| {
                left.resource
                    .id
                    .cmp(&right.resource.id)
                    .then(left.kind.cmp(&right.kind))
            });
            hazards.dedup();
            RenderGraphDiagnosticDependency {
                from_pass,
                from_name: passes[from_pass].name.clone(),
                to_pass,
                to_name: passes[to_pass].name.clone(),
                hazards,
            }
        })
        .collect()
}

impl From<ResourceHazardKind> for RenderGraphHazardKind {
    fn from(hazard: ResourceHazardKind) -> Self {
        match hazard {
            ResourceHazardKind::ReadAfterWrite => Self::Raw,
            ResourceHazardKind::WriteAfterRead => Self::War,
            ResourceHazardKind::WriteAfterWrite => Self::Waw,
        }
    }
}

fn resource_refs(
    resource_ids: &[ResourceId],
    resources: &[GraphResourceDesc],
) -> Vec<RenderGraphDiagnosticResourceRef> {
    resource_ids
        .iter()
        .map(|resource| (resource.0, resource_ref(*resource, resources)))
        .collect::<BTreeMap<_, _>>()
        .into_values()
        .collect()
}

pub(super) fn resource_ref(
    resource: ResourceId,
    resources: &[GraphResourceDesc],
) -> RenderGraphDiagnosticResourceRef {
    RenderGraphDiagnosticResourceRef {
        id: resource.0,
        name: resources
            .get(resource.0 as usize)
            .map(|resource| resource.name.clone())
            .unwrap_or_else(|| format!("<resource:{}>", resource.0)),
    }
}

impl From<ResourceLifetime> for RenderGraphDiagnosticResourceLifetime {
    fn from(lifetime: ResourceLifetime) -> Self {
        Self {
            first_execution_position: lifetime.first_execution_position,
            first_pass: lifetime.first_pass,
            last_execution_position: lifetime.last_execution_position,
            last_pass: lifetime.last_pass,
        }
    }
}

fn resource_kind(resource_type: &GraphResourceType) -> String {
    match resource_type {
        GraphResourceType::ColorAttachment { .. } => "color_attachment",
        GraphResourceType::DepthAttachment { sampled: true, .. } => "sampled_depth_attachment",
        GraphResourceType::DepthAttachment { sampled: false, .. } => "depth_attachment",
        GraphResourceType::SampledImage => "sampled_image",
    }
    .to_string()
}
