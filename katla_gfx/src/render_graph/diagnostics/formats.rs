use super::*;
use std::fmt::{self, Write as _};

impl fmt::Display for RenderGraphDiagnosticBufferDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} bytes, {:?}, usages {:?}",
            self.size, self.memory, self.usages
        )
    }
}

impl RenderGraphDiagnostics {
    /// Serialize the snapshot as stable, pretty-printed JSON.
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Export the snapshot as deterministic Graphviz DOT.
    pub fn to_dot(&self) -> String {
        let mut output = String::from("digraph render_graph {\n  rankdir=LR;\n");

        for resource in &self.resources {
            let origin = match resource.origin {
                RenderGraphDiagnosticResourceOrigin::BuiltIn => "built-in",
                RenderGraphDiagnosticResourceOrigin::Imported => "imported",
                RenderGraphDiagnosticResourceOrigin::Transient => "transient",
            };
            let exported = if resource.exported { "\\nexported" } else { "" };
            let contract = resource
                .imported_contract
                .as_ref()
                .map(|contract| {
                    format!(
                        "\\ninitial {}, final {}",
                        contract.initial,
                        contract
                            .required_final
                            .as_deref()
                            .unwrap_or("unconstrained")
                    )
                })
                .unwrap_or_default();
            let live = if resource.live { "live" } else { "culled" };
            let decision = resource
                .cull_reason
                .as_ref()
                .map(|reason| format!("\\n{}", escape_dot(reason)))
                .unwrap_or_default();
            let resource_style = if resource.live {
                ""
            } else {
                ",style=dashed,color=gray50"
            };
            let peripheries = if resource.exported { 2 } else { 1 };
            let buffer = resource
                .buffer
                .as_ref()
                .map(|buffer| format!("\\nbuffer {buffer}"))
                .unwrap_or_default();
            let _ = writeln!(
                output,
                "  r{} [shape=ellipse{},peripheries={},label=\"{}: {}\\n{}{}{}{}\\n{}{}\"];",
                resource.id,
                resource_style,
                peripheries,
                resource.id,
                escape_dot(&resource.name),
                origin,
                exported,
                contract,
                buffer,
                live,
                decision
            );
        }

        for pass in &self.passes {
            let mut status = match pass.parallel_level {
                Some(level) => format!("level {level}"),
                None => "culled".to_string(),
            };
            status.push_str(&format!("\\n{}", escape_dot(&pass.liveness_reason)));
            if pass.side_effect {
                status.push_str("\\nside-effect");
            }
            let node_style = if pass.culled {
                ",style=\"dashed\",color=\"gray50\",fontcolor=\"gray40\""
            } else {
                ""
            };
            let edge_style = if pass.culled {
                ",style=\"dotted\",color=\"gray60\""
            } else {
                ""
            };
            let _ = writeln!(
                output,
                "  p{} [shape=box{},label=\"{}: {}\\n{}\"];",
                pass.index,
                node_style,
                pass.index,
                escape_dot(&pass.name),
                status
            );

            for access in &pass.image_accesses {
                let label = escape_dot(&access.to_string());
                if matches!(
                    access.mode,
                    RenderGraphDiagnosticResourceAccessMode::Read
                        | RenderGraphDiagnosticResourceAccessMode::ReadWrite
                ) {
                    let _ = writeln!(
                        output,
                        "  r{} -> p{} [label=\"{}\"{}];",
                        access.resource.id, pass.index, label, edge_style
                    );
                }
                if matches!(
                    access.mode,
                    RenderGraphDiagnosticResourceAccessMode::Write
                        | RenderGraphDiagnosticResourceAccessMode::ReadWrite
                ) {
                    let _ = writeln!(
                        output,
                        "  p{} -> r{} [label=\"{}\"{}];",
                        pass.index, access.resource.id, label, edge_style
                    );
                }
            }

            for access in &pass.buffer_accesses {
                let label = escape_dot(&access.to_string());
                if matches!(
                    access.mode,
                    RenderGraphDiagnosticResourceAccessMode::Read
                        | RenderGraphDiagnosticResourceAccessMode::ReadWrite
                ) {
                    let _ = writeln!(
                        output,
                        "  r{} -> p{} [label=\"{}\"{}];",
                        access.resource.id, pass.index, label, edge_style
                    );
                }
                if matches!(
                    access.mode,
                    RenderGraphDiagnosticResourceAccessMode::Write
                        | RenderGraphDiagnosticResourceAccessMode::ReadWrite
                ) {
                    let _ = writeln!(
                        output,
                        "  p{} -> r{} [label=\"{}\"{}];",
                        pass.index, access.resource.id, label, edge_style
                    );
                }
            }
        }

        for dependency in &self.dependencies {
            let label = dependency
                .hazards
                .iter()
                .map(|hazard| format!("{:?} {}", hazard.kind, escape_dot(&hazard.resource.name)))
                .collect::<Vec<_>>()
                .join("\\n");
            let _ = writeln!(
                output,
                "  p{} -> p{} [style=dashed,label=\"{label}\"];",
                dependency.from_pass, dependency.to_pass
            );
        }

        // Physical allocation nodes are rendered apart from the logical
        // resource ellipses so aliasing is visible without changing the
        // logical dataflow view.
        for slot in &self.transient_slots {
            let compatibility = &slot.compatibility;
            let tile_memory = if slot.tile_memory.eligible {
                "tile memory: eligible".to_string()
            } else {
                format!("tile memory: no ({})", slot.tile_memory.reason)
            };
            let _ = writeln!(
                output,
                "  a{} [shape=cylinder,style=dashed,label=\"slot {}: {} bytes\\nsaves {} bytes\\n{} {} {}x{} {}\\n{}\\npositions {}-{}\"];",
                slot.id,
                slot.id,
                slot.bytes,
                slot.saved_bytes,
                compatibility.kind,
                compatibility.format,
                compatibility.width,
                compatibility.height,
                if compatibility.tracks_swapchain_size {
                    "swapchain-tracked"
                } else {
                    "fixed-size"
                },
                escape_dot(&tile_memory),
                slot.first_execution_position,
                slot.last_execution_position
            );
            for member in &slot.resources {
                let _ = writeln!(
                    output,
                    "  a{} -> r{} [arrowhead=none,style=dotted,color=gray50,label=\"alias\"];",
                    slot.id, member.id
                );
            }
        }

        let frame_boundary_transitions = self
            .synchronization
            .iter()
            .filter(|transition| transition.before_pass.is_none() || transition.to_pass.is_none())
            .collect::<Vec<_>>();
        if !frame_boundary_transitions.is_empty() {
            let _ = writeln!(
                output,
                "  frame_start [shape=box,style=\"rounded,dashed\",label=\"frame start\"];"
            );
            let _ = writeln!(
                output,
                "  frame_end [shape=box,style=\"rounded,dashed\",label=\"frame end\"];"
            );
        }
        for transition in &frame_boundary_transitions {
            let label = escape_dot(&transition_body_label(transition));
            match (transition.before_pass, transition.to_pass) {
                (None, Some(to_pass)) => {
                    let _ = writeln!(
                        output,
                        "  frame_start -> p{to_pass} [label=\"{label}\",style=\"dashed\",color=\"gray40\"];"
                    );
                }
                (Some(before_pass), None) => {
                    let _ = writeln!(
                        output,
                        "  p{before_pass} -> frame_end [label=\"{label}\",style=\"dashed\",color=\"gray40\"];"
                    );
                }
                // A frame-end op whose contents were never written this
                // frame runs straight from the contract's arrival state.
                (None, None) => {
                    let _ = writeln!(
                        output,
                        "  frame_start -> frame_end [label=\"{label}\",style=\"dashed\",color=\"gray40\"];"
                    );
                }
                (Some(_), Some(_)) => {}
            }
        }

        for allocation in &self.native_allocations {
            let _ = writeln!(
                output,
                "  native_{} [shape=box,label=\"frame {} {}\\noffset {} bytes {}\\nalias saved {} tile saved {}\"];",
                allocation.id,
                allocation.frame_slot,
                allocation.strategy,
                allocation.offset,
                allocation.bytes,
                allocation.alias_savings_bytes,
                allocation.tile_storage_savings_bytes
            );
            for resource in &allocation.resources {
                let _ = writeln!(
                    output,
                    "  native_{} -> r{} [style=dashed];",
                    allocation.id, resource.id
                );
            }
        }
        output.push_str("}\n");
        output
    }
}

impl fmt::Display for RenderGraphDiagnostics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{} declared passes ({} live, {} culled), {} resources, {} dependency edges, {} synchronization transitions, {} transient allocations ({} logical bytes, {} physical bytes, {} saved, {} tile-memory eligible), {} parallel levels",
            self.summary.declared_passes,
            self.summary.live_passes,
            self.summary.culled_passes,
            self.summary.resources,
            self.summary.dependency_edges,
            self.summary.synchronization_transitions,
            self.summary.physical_transient_allocations,
            self.summary.logical_transient_bytes,
            self.summary.physical_transient_bytes,
            self.summary.transient_alias_savings_bytes,
            self.summary.tile_memory_eligible_bytes,
            self.summary.parallel_levels
        )?;

        writeln!(
            f,
            "  culling enabled: {}, liveness roots {:?}",
            self.culling_enabled, self.liveness_roots
        )?;
        for producer in &self.external_image_producers {
            writeln!(
                f,
                "  external {} / {}: {}",
                producer.queue, producer.encoder, producer.access
            )?;
        }
        for pass in &self.passes {
            if let (Some(queue), Some(encoder)) = (&pass.queue, &pass.encoder) {
                writeln!(
                    f,
                    "  boundary p{}: {} / {}, predecessors {:?}, alias handoffs {:?}",
                    pass.index,
                    queue,
                    encoder,
                    pass.predecessors,
                    pass.alias_handoffs
                        .iter()
                        .map(|resource| resource.id)
                        .collect::<Vec<_>>()
                )?;
            }
        }
        writeln!(f, "  allocation source: {}", self.allocation_source)?;
        for allocation in &self.native_allocations {
            writeln!(
                f,
                "  native allocation {} frame {}: {} offset {} bytes {}, logical {}, alias saved {}, tile saved {}, resources {:?}",
                allocation.id,
                allocation.frame_slot,
                allocation.strategy,
                allocation.offset,
                allocation.bytes,
                allocation.logical_bytes,
                allocation.alias_savings_bytes,
                allocation.tile_storage_savings_bytes,
                allocation
                    .resources
                    .iter()
                    .map(|resource| resource.id)
                    .collect::<Vec<_>>()
            )?;
            writeln!(
                f,
                "      compatibility {}, live {:?}..{:?}",
                allocation.compatibility_class,
                allocation.first_execution_position,
                allocation.last_execution_position
            )?;
        }
        for resource in &self.resources {
            writeln!(
                f,
                "  r{} ({}) {}{} alias predecessor {:?}, successor {:?}",
                resource.id,
                resource.name,
                if resource.live { "live" } else { "culled" },
                resource
                    .cull_reason
                    .as_ref()
                    .map(|reason| format!(": {reason};"))
                    .unwrap_or_default(),
                resource.alias_predecessor,
                resource.alias_successor
            )?;
        }
        for resource in &self.resources {
            if let Some(buffer) = &resource.buffer {
                let origin = match resource.origin {
                    RenderGraphDiagnosticResourceOrigin::BuiltIn => "built-in",
                    RenderGraphDiagnosticResourceOrigin::Imported => "imported",
                    RenderGraphDiagnosticResourceOrigin::Transient => "transient",
                };
                writeln!(
                    f,
                    "  r{} ({}) {origin} buffer, {buffer}",
                    resource.id, resource.name
                )?;
            }
        }

        for resource in self
            .resources
            .iter()
            .filter(|resource| resource.imported_contract.is_some())
        {
            let contract = resource.imported_contract.as_ref().expect("filtered above");
            writeln!(
                f,
                "  r{} ({}) imported, initial {}, required final {}",
                resource.id,
                resource.name,
                contract.initial,
                contract
                    .required_final
                    .as_deref()
                    .unwrap_or("unconstrained")
            )?;
        }

        for &pass_index in &self.execution_order {
            let pass = &self.passes[pass_index];
            let level = pass
                .parallel_level
                .expect("live execution-order pass must have a parallel level");
            writeln!(
                f,
                "  [{}] {} (level {}, reads {}, writes {}{}){}{}",
                pass.index,
                pass.name,
                level,
                pass.reads.len(),
                pass.writes.len(),
                if pass.side_effect {
                    ", side-effect"
                } else {
                    ""
                },
                if pass.color_attachments.is_empty() {
                    String::new()
                } else {
                    let colors = pass
                        .color_attachments
                        .iter()
                        .map(|a| format!("r{} {}->{}", a.resource, a.load, a.store))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!(", colors [{colors}]")
                },
                if let Some(depth) = &pass.depth_attachment {
                    format!(
                        ", depth [{}->{}, stencil {}->{}]",
                        depth.depth_load,
                        depth.depth_store,
                        depth.stencil_load,
                        depth.stencil_store
                    )
                } else {
                    String::new()
                }
            )?;
            for access in &pass.image_accesses {
                writeln!(
                    f,
                    "    r{} ({}): {access}",
                    access.resource.id, access.resource.name
                )?;
            }
            for access in &pass.buffer_accesses {
                writeln!(
                    f,
                    "    r{} ({}): {access}",
                    access.resource.id, access.resource.name
                )?;
            }
        }

        for pass in self.passes.iter().filter(|pass| pass.culled) {
            writeln!(
                f,
                "  [{}] {} (culled): {}",
                pass.index, pass.name, pass.liveness_reason
            )?;
            for access in &pass.image_accesses {
                writeln!(
                    f,
                    "    r{} ({}): {access}",
                    access.resource.id, access.resource.name
                )?;
            }
            for access in &pass.buffer_accesses {
                writeln!(
                    f,
                    "    r{} ({}): {access}",
                    access.resource.id, access.resource.name
                )?;
            }
        }

        if !self.synchronization.is_empty() {
            writeln!(f, "  synchronization transitions:")?;
            for transition in &self.synchronization {
                writeln!(f, "    {transition}")?;
            }
        }

        if !self.buffer_synchronization.is_empty() {
            writeln!(f, "  buffer synchronization operations:")?;
            for op in &self.buffer_synchronization {
                writeln!(f, "    {op}")?;
            }
        }

        if !self.transient_slots.is_empty() {
            writeln!(f, "  transient allocation slots:")?;
            for slot in &self.transient_slots {
                let compatibility = &slot.compatibility;
                writeln!(
                    f,
                    "    slot {} ({} bytes, saves {}, positions {}-{}, {} {} {}x{}, {}, tile memory: {}): {}",
                    slot.id,
                    slot.bytes,
                    slot.saved_bytes,
                    slot.first_execution_position,
                    slot.last_execution_position,
                    compatibility.kind,
                    compatibility.format,
                    compatibility.width,
                    compatibility.height,
                    if compatibility.tracks_swapchain_size {
                        "swapchain-tracked"
                    } else {
                        "fixed-size"
                    },
                    if slot.tile_memory.eligible {
                        "eligible".to_string()
                    } else {
                        format!("no ({})", slot.tile_memory.reason)
                    },
                    slot.resources
                        .iter()
                        .map(|resource| format!("r{} ({})", resource.id, resource.name))
                        .collect::<Vec<_>>()
                        .join(" -> ")
                )?;
            }
        }

        Ok(())
    }
}

impl fmt::Display for RenderGraphDiagnosticImageAccess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mode = match self.mode {
            RenderGraphDiagnosticResourceAccessMode::Read => "read",
            RenderGraphDiagnosticResourceAccessMode::Write => "write",
            RenderGraphDiagnosticResourceAccessMode::ReadWrite => "read_write",
        };
        write!(
            f,
            "{mode} {:?} @ {:?}, {}",
            self.usage,
            self.stage,
            subresource_range_label(&self.range)
        )
    }
}

impl fmt::Display for RenderGraphDiagnosticBufferSyncOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let producer = match (self.before_pass, &self.before_name) {
            (Some(index), Some(name)) => format!("pass {index} ({name})"),
            _ => "frame start".to_string(),
        };
        let range = if self.range.size == u64::MAX {
            format!("bytes {}+unbounded", self.range.offset)
        } else {
            format!("bytes {}+{}", self.range.offset, self.range.size)
        };
        let cause = match self.hazard {
            Some(kind) => format!("hazard {kind:?}"),
            None => self.reason.clone(),
        };
        write!(
            f,
            "[{producer} -> pass {} ({})] r{} ({}), {range}: {} -> {} ({cause}); version {}, {} -> {}",
            self.pass,
            self.pass_name,
            self.resource.id,
            self.resource.name,
            self.before,
            self.after,
            self.version,
            self.source_boundary,
            self.destination_boundary,
        )
    }
}

impl fmt::Display for RenderGraphDiagnosticBufferAccess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mode = match self.mode {
            RenderGraphDiagnosticResourceAccessMode::Read => "read",
            RenderGraphDiagnosticResourceAccessMode::Write => "write",
            RenderGraphDiagnosticResourceAccessMode::ReadWrite => "read_write",
        };
        // A size of u64::MAX means "all remaining bytes", matching the
        // subresource convention (`count == u32::MAX`).
        let range = if self.range.size == u64::MAX {
            format!("bytes {}+unbounded", self.range.offset)
        } else {
            format!("bytes {}+{}", self.range.offset, self.range.size)
        };
        write!(f, "{mode} {:?} @ {:?}, {range}", self.usage, self.stage)
    }
}

impl fmt::Display for RenderGraphDiagnosticTransition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let producer = match (self.before_pass, &self.before_name) {
            (Some(index), Some(name)) => format!("pass {index} ({name})"),
            _ => "frame start".to_string(),
        };
        let consumer = match (self.to_pass, &self.to_name) {
            (Some(index), Some(name)) => format!("pass {index} ({name})"),
            _ => "frame end".to_string(),
        };
        write!(
            f,
            "[{producer} -> {consumer}] {}; version {}, {} -> {}",
            transition_body_label(self),
            self.version,
            self.source_boundary,
            self.destination_boundary
        )
    }
}

fn transition_body_label(transition: &RenderGraphDiagnosticTransition) -> String {
    let cause = match transition.reason {
        RenderGraphDiagnosticSyncReason::InitialUse => "initial_use",
        RenderGraphDiagnosticSyncReason::Hazard => "hazard",
        RenderGraphDiagnosticSyncReason::StateChange => "state_change",
        RenderGraphDiagnosticSyncReason::ImportedFinal => "imported_final",
    };
    let hazard = match transition.hazard {
        Some(kind) => format!("hazard {kind:?}"),
        None => cause.to_string(),
    };
    format!(
        "r{} ({}), {}: {} -> {} ({hazard})",
        transition.resource.id,
        transition.resource.name,
        subresource_range_label(&transition.range),
        sync_state_label(&transition.before_state),
        sync_state_label(&transition.after_state),
    )
}

fn subresource_range_label(range: &RenderGraphDiagnosticImageSubresourceRange) -> String {
    format!(
        "{}, mips {}+{}, layers {}+{}",
        range.aspects.join("|"),
        range.base_mip_level,
        range.mip_level_count,
        range.base_array_layer,
        range.array_layer_count
    )
}

fn sync_state_label(state: &RenderGraphDiagnosticSyncState) -> String {
    match state {
        RenderGraphDiagnosticSyncState::Undefined => "undefined".to_string(),
        RenderGraphDiagnosticSyncState::Access { usage, stage, mode } => format!(
            "{} {usage:?} @ {stage:?}",
            match mode {
                RenderGraphDiagnosticResourceAccessMode::Read => "read",
                RenderGraphDiagnosticResourceAccessMode::Write => "write",
                RenderGraphDiagnosticResourceAccessMode::ReadWrite => "read_write",
            }
        ),
    }
}

pub(super) fn escape_dot(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}
