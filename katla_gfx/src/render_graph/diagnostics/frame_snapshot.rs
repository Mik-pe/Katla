use super::projection::{BufferDiagnosticResource, pass_boundary_label, resource_ref};
use super::*;

impl<B: RenderGraphBackend> FrameGraph<B> {
    /// Joins the compiler snapshot with the last observed frame without waiting.
    pub fn capture(&self) -> Result<super::super::capture::RenderGraphCapture, RenderGraphError> {
        let plan = self.build_execution_plan()?;
        let mut sync = Vec::new();
        for &pass in &plan.sorted_passes {
            for operation in &plan.sync.pass_ops[pass] {
                sync.push(super::super::capture::CapturedSyncOperation::image(
                    operation,
                    &format!(
                        "{} -> {}",
                        pass_boundary_label(&plan, operation.before_pass, "frame_start"),
                        pass_boundary_label(&plan, Some(pass), "frame_end")
                    ),
                ));
            }
            for operation in &plan.sync.pass_buffer_ops[pass] {
                sync.push(super::super::capture::CapturedSyncOperation::buffer(
                    operation,
                    &format!(
                        "{} -> {}",
                        pass_boundary_label(&plan, operation.before_pass, "frame_start"),
                        pass_boundary_label(&plan, Some(pass), "frame_end")
                    ),
                ));
            }
        }
        for operation in &plan.sync.final_ops {
            sync.push(super::super::capture::CapturedSyncOperation::image(
                operation,
                &format!(
                    "{} -> frame_end",
                    pass_boundary_label(&plan, operation.before_pass, "frame_start")
                ),
            ));
        }
        let comparison = super::super::trace::compare_with_compiled(
            &self.resources,
            &self.passes,
            &plan.sorted_passes,
            self.last_execution_trace(),
        )
        .iter()
        .map(ToString::to_string)
        .collect();
        Ok(super::super::capture::RenderGraphCapture::join(
            self.diagnostics()?,
            sync,
            self.last_execution_trace(),
            comparison,
        ))
    }

    /// Build a deterministic diagnostics snapshot from the graph's canonical compiler.
    ///
    /// The compiler is pure, so diagnostics can be requested without allocating GPU
    /// resources or mutating frame execution state.
    pub fn diagnostics(&self) -> Result<RenderGraphDiagnostics, RenderGraphError> {
        let plan = self.build_execution_plan()?;
        let buffers = self
            .resources
            .iter()
            .enumerate()
            .filter_map(|(index, resource)| {
                self.buffer_desc(&resource.name).map(|descriptor| {
                    (
                        ResourceId(index as u32),
                        BufferDiagnosticResource {
                            descriptor,
                            origin: if self
                                .transient_buffers
                                .iter()
                                .any(|buffer| buffer.name == resource.name)
                            {
                                RenderGraphDiagnosticResourceOrigin::Transient
                            } else {
                                RenderGraphDiagnosticResourceOrigin::Imported
                            },
                        },
                    )
                })
            })
            .collect();
        let mut diagnostics = RenderGraphDiagnostics::from_parts(
            &self.passes,
            &self.resources,
            &self.transient_resources,
            &self.exported_resources,
            &self.imported_contracts,
            &buffers,
            &plan,
        );
        if !self.transient_aliasing {
            let mut standalone = Vec::new();
            for resource in &mut diagnostics.resources {
                let Some(id) = resource.physical_allocation_id else {
                    continue;
                };
                let Some(projected) = diagnostics
                    .transient_slots
                    .iter()
                    .find(|slot| slot.id == id)
                else {
                    continue;
                };
                let Some(lifetime) = resource.lifetime.as_ref() else {
                    continue;
                };
                let mut slot = projected.clone();
                slot.id = standalone.len() as u32;
                slot.resources = vec![RenderGraphDiagnosticResourceRef {
                    id: resource.id,
                    name: resource.name.clone(),
                }];
                slot.logical_bytes = slot.bytes;
                slot.saved_bytes = 0;
                slot.tile_memory = RenderGraphDiagnosticTileMemory {
                    eligible: false,
                    reason: "transient optimization disabled".into(),
                };
                slot.first_execution_position = lifetime.first_execution_position;
                slot.last_execution_position = lifetime.last_execution_position;
                resource.physical_allocation_id = Some(slot.id);
                resource.alias_predecessor = None;
                resource.alias_successor = None;
                standalone.push(slot);
            }
            diagnostics.transient_slots = standalone;
            diagnostics.summary.physical_transient_allocations = diagnostics.transient_slots.len();
            diagnostics.summary.physical_transient_bytes =
                diagnostics.summary.logical_transient_bytes;
            diagnostics.summary.transient_alias_savings_bytes = 0;
            diagnostics.summary.tile_memory_eligible_bytes = 0;
        }
        let mut identities = BTreeMap::new();
        for (frame_slot, textures) in self.transient_textures.iter().enumerate() {
            let mut textures = textures.iter().collect::<Vec<_>>();
            textures.sort_by_key(|(resource, _)| **resource);
            for (&resource, texture) in textures {
                let Some(native) = B::transient_allocation_info(texture) else {
                    continue;
                };
                let key = (frame_slot, native.identity, native.offset);
                let next_id = diagnostics.native_allocations.len() as u32;
                let id = *identities.entry(key).or_insert_with(|| {
                    diagnostics
                        .native_allocations
                        .push(RenderGraphDiagnosticNativeAllocation {
                            id: next_id,
                            frame_slot,
                            first_execution_position: None,
                            last_execution_position: None,
                            compatibility_class: String::new(),
                            resources: Vec::new(),
                            offset: native.offset,
                            bytes: native.bytes,
                            logical_bytes: 0,
                            strategy: native.strategy.into(),
                            alias_savings_bytes: 0,
                            tile_storage_savings_bytes: 0,
                            bandwidth_savings_estimate_bytes: None,
                        });
                    next_id
                });
                let allocation = &mut diagnostics.native_allocations[id as usize];
                allocation
                    .resources
                    .push(resource_ref(resource, &self.resources));
                allocation.logical_bytes = allocation
                    .logical_bytes
                    .saturating_add(native.logical_bytes);
                if native.strategy == "memoryless" {
                    allocation.tile_storage_savings_bytes = allocation.logical_bytes;
                } else {
                    allocation.alias_savings_bytes =
                        allocation.logical_bytes.saturating_sub(allocation.bytes);
                }
            }
        }
        for (frame_slot, buffers) in self.transient_buffers_by_frame.iter().enumerate() {
            let mut buffers = buffers.iter().collect::<Vec<_>>();
            buffers.sort_by_key(|(resource, _)| **resource);
            for (&resource, buffer) in buffers {
                let Some(native) = B::transient_buffer_allocation_info(buffer) else {
                    continue;
                };
                let id = diagnostics.native_allocations.len() as u32;
                diagnostics
                    .native_allocations
                    .push(RenderGraphDiagnosticNativeAllocation {
                        id,
                        frame_slot,
                        first_execution_position: None,
                        last_execution_position: None,
                        compatibility_class: String::new(),
                        resources: vec![resource_ref(resource, &self.resources)],
                        offset: native.offset,
                        bytes: native.bytes,
                        logical_bytes: native.logical_bytes,
                        strategy: native.strategy.into(),
                        alias_savings_bytes: 0,
                        tile_storage_savings_bytes: 0,
                        bandwidth_savings_estimate_bytes: None,
                    });
            }
        }
        for allocation in &mut diagnostics.native_allocations {
            allocation.resources.sort_by_key(|reference| {
                diagnostics
                    .resources
                    .get(reference.id as usize)
                    .and_then(|resource| resource.lifetime.as_ref())
                    .map(|lifetime| lifetime.first_execution_position)
                    .unwrap_or(usize::MAX)
            });
            let lifetimes: Vec<_> = allocation
                .resources
                .iter()
                .filter_map(|reference| {
                    diagnostics
                        .resources
                        .get(reference.id as usize)
                        .and_then(|resource| resource.lifetime.as_ref())
                })
                .collect();
            allocation.first_execution_position = lifetimes
                .iter()
                .map(|lifetime| lifetime.first_execution_position)
                .min();
            allocation.last_execution_position = lifetimes
                .iter()
                .map(|lifetime| lifetime.last_execution_position)
                .max();
            if let Some(resource) = allocation
                .resources
                .first()
                .and_then(|reference| diagnostics.resources.get(reference.id as usize))
            {
                allocation.compatibility_class = resource
                    .buffer
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| {
                        format!(
                            "{} {} {}x{}",
                            resource.kind.as_deref().unwrap_or("image"),
                            resource.format.as_deref().unwrap_or("unknown"),
                            resource.width.unwrap_or(0),
                            resource.height.unwrap_or(0)
                        )
                    });
            }
        }
        if !diagnostics.native_allocations.is_empty() {
            diagnostics.allocation_source = "native_frame_allocations".into();
            diagnostics.summary.physical_transient_allocations = diagnostics
                .native_allocations
                .iter()
                .filter(|allocation| allocation.bytes > 0)
                .count();
            diagnostics.summary.physical_transient_bytes = diagnostics
                .native_allocations
                .iter()
                .map(|allocation| allocation.bytes)
                .sum();
            diagnostics.summary.logical_transient_bytes = diagnostics
                .native_allocations
                .iter()
                .map(|allocation| allocation.logical_bytes)
                .sum();
            diagnostics.summary.transient_alias_savings_bytes = diagnostics
                .native_allocations
                .iter()
                .map(|allocation| allocation.alias_savings_bytes)
                .sum();
        }
        Ok(diagnostics)
    }
}
