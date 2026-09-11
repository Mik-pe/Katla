//! Deterministic diagnostics for compiled render graphs.
//!
//! Diagnostics intentionally contain only stable graph data: declaration indices,
//! resource names, access hazards, execution order, and parallel levels. Backend
//! pointers, device addresses, and hash-map iteration order never enter the output.
//!
//! Text and DOT access labels include mode, usage, pipeline stage, aspects, and
//! mip/layer ranges as `base+count`. A count of `u32::MAX` means all remaining
//! subresources, matching the typed graph declaration. Read-write accesses have
//! edges in both directions; dotted edges belong to culled passes.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write as _};

use serde::Serialize;

use super::BACKBUFFER_NAME;
use super::access::{ImageAccess, ImageAccessMode, ImagePipelineStage, ImageUsage};
use super::allocation_plan::TransientAllocationPlan;
use super::backend::RenderGraphBackend;
use super::compiler::{ExecutionPlan, ResourceLifetime};
use super::error::RenderGraphError;
use super::frame_graph::FrameGraph;
use super::handles::ResourceId;
use super::pass::{PassDesc, PassType};
use super::resource::{GraphResourceDesc, GraphResourceType, ImportedImageContract};
use super::{ImageSyncOp, ImageSyncState, ResourceHazardKind};

/// Schema version for serialized render-graph diagnostics.
pub const RENDER_GRAPH_DIAGNOSTICS_SCHEMA_VERSION: u32 = 7;

/// Stable, backend-neutral snapshot of a render graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnostics {
    pub schema_version: u32,
    pub summary: RenderGraphDiagnosticSummary,
    pub resources: Vec<RenderGraphDiagnosticResource>,
    pub passes: Vec<RenderGraphDiagnosticPass>,
    pub dependencies: Vec<RenderGraphDiagnosticDependency>,
    pub synchronization: Vec<RenderGraphDiagnosticTransition>,
    pub execution_order: Vec<usize>,
    pub parallel_groups: Vec<Vec<usize>>,
}

/// Aggregate counts for a diagnostics snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticSummary {
    pub declared_passes: usize,
    pub live_passes: usize,
    pub culled_passes: usize,
    pub resources: usize,
    pub dependency_edges: usize,
    pub synchronization_transitions: usize,
    pub physical_transient_allocations: usize,
    pub logical_transient_bytes: u64,
    pub physical_transient_bytes: u64,
    pub transient_alias_savings_bytes: u64,
    pub parallel_levels: usize,
}

/// Stable resource origin classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderGraphDiagnosticResourceOrigin {
    BuiltIn,
    Imported,
    Transient,
}

/// First and last scheduled access to a resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticResourceLifetime {
    pub first_execution_position: usize,
    pub first_pass: usize,
    pub last_execution_position: usize,
    pub last_pass: usize,
}

/// Imported-image initial/final state contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticImportedContract {
    pub initial: String,
    pub required_final: Option<String>,
}

/// Resource information resolved from the graph namespace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticResource {
    pub id: u32,
    pub name: String,
    pub origin: RenderGraphDiagnosticResourceOrigin,
    pub kind: Option<String>,
    pub format: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub tracks_swapchain_size: Option<bool>,
    pub exported: bool,
    pub lifetime: Option<RenderGraphDiagnosticResourceLifetime>,
    /// Stable backend-neutral physical allocation slot assigned by the alias planner.
    pub physical_allocation_id: Option<u32>,
    /// Declared initial/final state contract, present for imported images.
    pub imported_contract: Option<RenderGraphDiagnosticImportedContract>,
}

/// Pass type without backend-specific command data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderGraphDiagnosticPassType {
    Graphics,
    Compute,
}

/// Stable resource reference used by pass diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticResourceRef {
    pub id: u32,
    pub name: String,
}

/// Stable typed image access mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderGraphDiagnosticImageAccessMode {
    Read,
    Write,
    ReadWrite,
}

/// Stable typed image usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderGraphDiagnosticImageUsage {
    Sampled,
    ColorAttachment,
    DepthStencilAttachment,
    Storage,
    TransferSource,
    TransferDestination,
    Present,
}

/// Stable typed pipeline visibility for an image access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderGraphDiagnosticImageStage {
    VertexShader,
    FragmentShader,
    ComputeShader,
    ColorAttachmentOutput,
    DepthStencil,
    Transfer,
    Present,
    AllGraphics,
}

/// Stable image subresource range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticImageSubresourceRange {
    pub aspects: Vec<String>,
    pub base_mip_level: u32,
    pub mip_level_count: u32,
    pub base_array_layer: u32,
    pub array_layer_count: u32,
}

/// One typed image access declared by a pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticImageAccess {
    pub resource: RenderGraphDiagnosticResourceRef,
    pub mode: RenderGraphDiagnosticImageAccessMode,
    pub usage: RenderGraphDiagnosticImageUsage,
    pub stage: RenderGraphDiagnosticImageStage,
    pub range: RenderGraphDiagnosticImageSubresourceRange,
}

/// Pass information with canonical DAG metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticPass {
    pub index: usize,
    pub name: String,
    pub pass_type: RenderGraphDiagnosticPassType,
    pub kind: Option<String>,
    pub reads: Vec<RenderGraphDiagnosticResourceRef>,
    pub writes: Vec<RenderGraphDiagnosticResourceRef>,
    pub image_accesses: Vec<RenderGraphDiagnosticImageAccess>,
    /// Declared load/store operations per color target, in declaration order.
    pub color_attachments: Vec<RenderGraphDiagnosticAttachmentOps>,
    /// Declared depth/stencil operations, when the pass has a depth contract.
    pub depth_attachment: Option<RenderGraphDiagnosticDepthAttachmentOps>,
    pub predecessors: Vec<usize>,
    pub successors: Vec<usize>,
    pub execution_position: Option<usize>,
    pub parallel_level: Option<usize>,
    pub side_effect: bool,
    pub live: bool,
    pub culled: bool,
}

/// Declared load/store/clear operations for one color target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticAttachmentOps {
    pub resource: u32,
    pub load: String,
    pub store: String,
    pub clear_value: String,
}

/// Declared per-aspect operations for a depth-stencil target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticDepthAttachmentOps {
    pub depth_load: String,
    pub depth_store: String,
    pub stencil_load: String,
    pub stencil_store: String,
}

/// Resource hazard represented by a dependency edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum RenderGraphHazardKind {
    Raw,
    War,
    Waw,
}

/// One concrete hazard carried by a dependency edge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticHazard {
    pub kind: RenderGraphHazardKind,
    pub resource: RenderGraphDiagnosticResourceRef,
}

/// Dependency between two passes, including every resource hazard that created it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticDependency {
    pub from_pass: usize,
    pub from_name: String,
    pub to_pass: usize,
    pub to_name: String,
    pub hazards: Vec<RenderGraphDiagnosticHazard>,
}

/// Synchronization state one side of a transition is in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum RenderGraphDiagnosticSyncState {
    Undefined,
    Access {
        usage: RenderGraphDiagnosticImageUsage,
        stage: RenderGraphDiagnosticImageStage,
        mode: RenderGraphDiagnosticImageAccessMode,
    },
}

/// Why one compiled synchronization operation exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderGraphDiagnosticSyncReason {
    InitialUse,
    Hazard,
    StateChange,
    ImportedFinal,
}

/// One compiled synchronization operation between frame start, live passes,
/// and frame end.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticTransition {
    pub resource: RenderGraphDiagnosticResourceRef,
    /// Pass that established the `before` state; `None` at frame start.
    pub before_pass: Option<usize>,
    pub before_name: Option<String>,
    /// Pass the operation precedes; `None` at frame end.
    pub to_pass: Option<usize>,
    pub to_name: Option<String>,
    pub before_state: RenderGraphDiagnosticSyncState,
    pub after_state: RenderGraphDiagnosticSyncState,
    pub hazard: Option<RenderGraphHazardKind>,
    pub reason: RenderGraphDiagnosticSyncReason,
}

impl<B: RenderGraphBackend> FrameGraph<B> {
    /// Build a deterministic diagnostics snapshot from the graph's canonical compiler.
    ///
    /// The compiler is pure, so diagnostics can be requested without allocating GPU
    /// resources or mutating frame execution state.
    pub fn diagnostics(&self) -> Result<RenderGraphDiagnostics, RenderGraphError> {
        let plan = self.build_execution_plan()?;
        Ok(RenderGraphDiagnostics::from_parts(
            &self.passes,
            &self.resources,
            &self.transient_resources,
            &self.exported_resources,
            &self.imported_contracts,
            &plan,
        ))
    }
}

impl RenderGraphDiagnostics {
    fn from_parts(
        passes: &[PassDesc],
        resources: &[GraphResourceDesc],
        transient_resources: &[GraphResourceDesc],
        exported_resources: &BTreeSet<ResourceId>,
        imported_contracts: &BTreeMap<ResourceId, ImportedImageContract>,
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
        let allocation_plan = TransientAllocationPlan::build(
            resources,
            transient_resources,
            exported_resources,
            &plan.resource_lifetimes,
        );

        let diagnostic_resources = resources
            .iter()
            .enumerate()
            .map(|(index, namespace_resource)| {
                let descriptor = transient_by_name
                    .get(namespace_resource.name.as_str())
                    .copied();
                let origin = if namespace_resource.name == BACKBUFFER_NAME {
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
                    kind: descriptor.map(|resource| resource_kind(&resource.resource_type)),
                    format: descriptor.map(|resource| format!("{:?}", resource.format)),
                    width: descriptor.map(|resource| resource.width),
                    height: descriptor.map(|resource| resource.height),
                    tracks_swapchain_size: descriptor
                        .map(|resource| resource.tracks_swapchain_size),
                    exported: exported_resources.contains(&ResourceId(index as u32)),
                    lifetime: plan
                        .resource_lifetimes
                        .get(&ResourceId(index as u32))
                        .copied()
                        .map(RenderGraphDiagnosticResourceLifetime::from),
                    physical_allocation_id: allocation_plan
                        .physical_allocation_id(ResourceId(index as u32)),
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
                    },
                    kind: pass.kind.map(|kind| format!("{kind:?}")),
                    reads: resource_refs(&node.reads, resources),
                    writes: resource_refs(&node.writes, resources),
                    image_accesses: pass
                        .image_accesses
                        .iter()
                        .copied()
                        .map(|access| diagnostic_image_access(access, resources))
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
                }
            })
            .collect::<Vec<_>>();

        let synchronization = transition_diagnostics(passes, resources, plan);
        let dependencies = dependency_diagnostics(passes, resources, plan);
        let summary = RenderGraphDiagnosticSummary {
            declared_passes: passes.len(),
            live_passes: plan.live_passes.iter().filter(|&&live| live).count(),
            culled_passes: plan.culled_passes.len(),
            resources: diagnostic_resources.len(),
            dependency_edges: dependencies.len(),
            synchronization_transitions: synchronization.len(),
            physical_transient_allocations: allocation_plan.physical_allocation_count(),
            logical_transient_bytes: allocation_plan.logical_bytes(),
            physical_transient_bytes: allocation_plan.physical_bytes(),
            transient_alias_savings_bytes: allocation_plan.saved_bytes(),
            parallel_levels: plan.parallel_groups.len(),
        };

        Self {
            schema_version: RENDER_GRAPH_DIAGNOSTICS_SCHEMA_VERSION,
            summary,
            resources: diagnostic_resources,
            passes: diagnostic_passes,
            dependencies,
            synchronization,
            execution_order: plan.sorted_passes.clone(),
            parallel_groups: plan.parallel_groups.clone(),
        }
    }

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
            let peripheries = if resource.exported { 2 } else { 1 };
            let _ = writeln!(
                output,
                "  r{} [shape=ellipse,peripheries={},label=\"{}: {}\\n{}{}{}\"];",
                resource.id,
                peripheries,
                resource.id,
                escape_dot(&resource.name),
                origin,
                exported,
                contract
            );
        }

        for pass in &self.passes {
            let mut status = match pass.parallel_level {
                Some(level) => format!("level {level}"),
                None => "culled".to_string(),
            };
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
                    RenderGraphDiagnosticImageAccessMode::Read
                        | RenderGraphDiagnosticImageAccessMode::ReadWrite
                ) {
                    let _ = writeln!(
                        output,
                        "  r{} -> p{} [label=\"{}\"{}];",
                        access.resource.id, pass.index, label, edge_style
                    );
                }
                if matches!(
                    access.mode,
                    RenderGraphDiagnosticImageAccessMode::Write
                        | RenderGraphDiagnosticImageAccessMode::ReadWrite
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
                "  p{} -> p{} [style=dashed,label=\"{}\"];",
                dependency.from_pass, dependency.to_pass, label
            );
        }

        output.push_str("}\n");
        output
    }
}

impl fmt::Display for RenderGraphDiagnostics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{} declared passes ({} live, {} culled), {} resources, {} dependency edges, {} synchronization transitions, {} transient allocations ({} logical bytes, {} physical bytes, {} saved), {} parallel levels",
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
            self.summary.parallel_levels
        )?;

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
        }

        for pass in self.passes.iter().filter(|pass| pass.culled) {
            writeln!(f, "  [{}] {} (culled)", pass.index, pass.name)?;
            for access in &pass.image_accesses {
                writeln!(
                    f,
                    "    r{} ({}): {access}",
                    access.resource.id, access.resource.name
                )?;
            }
        }

        Ok(())
    }
}

impl fmt::Display for RenderGraphDiagnosticImageAccess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mode = match self.mode {
            RenderGraphDiagnosticImageAccessMode::Read => "read",
            RenderGraphDiagnosticImageAccessMode::Write => "write",
            RenderGraphDiagnosticImageAccessMode::ReadWrite => "read_write",
        };
        write!(
            f,
            "{mode} {:?} @ {:?}, {}, mips {}+{}, layers {}+{}",
            self.usage,
            self.stage,
            self.range.aspects.join("|"),
            self.range.base_mip_level,
            self.range.mip_level_count,
            self.range.base_array_layer,
            self.range.array_layer_count
        )
    }
}

fn diagnostic_image_access(
    access: ImageAccess,
    resources: &[GraphResourceDesc],
) -> RenderGraphDiagnosticImageAccess {
    let mode = match access.mode {
        ImageAccessMode::Read => RenderGraphDiagnosticImageAccessMode::Read,
        ImageAccessMode::Write => RenderGraphDiagnosticImageAccessMode::Write,
        ImageAccessMode::ReadWrite => RenderGraphDiagnosticImageAccessMode::ReadWrite,
    };
    let usage = match access.usage {
        ImageUsage::Sampled => RenderGraphDiagnosticImageUsage::Sampled,
        ImageUsage::ColorAttachment => RenderGraphDiagnosticImageUsage::ColorAttachment,
        ImageUsage::DepthStencilAttachment => {
            RenderGraphDiagnosticImageUsage::DepthStencilAttachment
        }
        ImageUsage::Storage => RenderGraphDiagnosticImageUsage::Storage,
        ImageUsage::TransferSource => RenderGraphDiagnosticImageUsage::TransferSource,
        ImageUsage::TransferDestination => RenderGraphDiagnosticImageUsage::TransferDestination,
        ImageUsage::Present => RenderGraphDiagnosticImageUsage::Present,
    };
    let stage = match access.stage {
        ImagePipelineStage::VertexShader => RenderGraphDiagnosticImageStage::VertexShader,
        ImagePipelineStage::FragmentShader => RenderGraphDiagnosticImageStage::FragmentShader,
        ImagePipelineStage::ComputeShader => RenderGraphDiagnosticImageStage::ComputeShader,
        ImagePipelineStage::ColorAttachmentOutput => {
            RenderGraphDiagnosticImageStage::ColorAttachmentOutput
        }
        ImagePipelineStage::DepthStencil => RenderGraphDiagnosticImageStage::DepthStencil,
        ImagePipelineStage::Transfer => RenderGraphDiagnosticImageStage::Transfer,
        ImagePipelineStage::Present => RenderGraphDiagnosticImageStage::Present,
        ImagePipelineStage::AllGraphics => RenderGraphDiagnosticImageStage::AllGraphics,
    };

    RenderGraphDiagnosticImageAccess {
        resource: resource_ref(access.resource, resources),
        mode,
        usage,
        stage,
        range: RenderGraphDiagnosticImageSubresourceRange {
            aspects: access.range.aspects.names().map(str::to_string).collect(),
            base_mip_level: access.range.base_mip_level,
            mip_level_count: access.range.mip_level_count,
            base_array_layer: access.range.base_array_layer,
            array_layer_count: access.range.array_layer_count,
        },
    }
}

fn transition_diagnostics(
    passes: &[PassDesc],
    resources: &[GraphResourceDesc],
    plan: &ExecutionPlan,
) -> Vec<RenderGraphDiagnosticTransition> {
    plan.sync
        .pass_ops
        .iter()
        .enumerate()
        .flat_map(|(pass_index, ops)| {
            ops.iter()
                .map(move |op| diagnostic_transition(op, passes, resources, Some(pass_index)))
        })
        .chain(
            plan.sync
                .final_ops
                .iter()
                .map(|op| diagnostic_transition(op, passes, resources, None)),
        )
        .collect()
}

fn diagnostic_transition(
    op: &ImageSyncOp,
    passes: &[PassDesc],
    resources: &[GraphResourceDesc],
    to_pass: Option<usize>,
) -> RenderGraphDiagnosticTransition {
    let hazard = match op.reason {
        super::SyncReason::Hazard(kind) => Some(kind.into()),
        _ => None,
    };
    let reason = match op.reason {
        super::SyncReason::InitialUse => RenderGraphDiagnosticSyncReason::InitialUse,
        super::SyncReason::Hazard(_) => RenderGraphDiagnosticSyncReason::Hazard,
        super::SyncReason::StateChange => RenderGraphDiagnosticSyncReason::StateChange,
        super::SyncReason::ImportedFinal => RenderGraphDiagnosticSyncReason::ImportedFinal,
    };

    RenderGraphDiagnosticTransition {
        resource: resource_ref(op.resource, resources),
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
    let usage = |usage: super::access::ImageUsage| match usage {
        ImageUsage::Sampled => RenderGraphDiagnosticImageUsage::Sampled,
        ImageUsage::ColorAttachment => RenderGraphDiagnosticImageUsage::ColorAttachment,
        ImageUsage::DepthStencilAttachment => {
            RenderGraphDiagnosticImageUsage::DepthStencilAttachment
        }
        ImageUsage::Storage => RenderGraphDiagnosticImageUsage::Storage,
        ImageUsage::TransferSource => RenderGraphDiagnosticImageUsage::TransferSource,
        ImageUsage::TransferDestination => RenderGraphDiagnosticImageUsage::TransferDestination,
        ImageUsage::Present => RenderGraphDiagnosticImageUsage::Present,
    };
    let stage = |stage: super::access::ImagePipelineStage| match stage {
        ImagePipelineStage::VertexShader => RenderGraphDiagnosticImageStage::VertexShader,
        ImagePipelineStage::FragmentShader => RenderGraphDiagnosticImageStage::FragmentShader,
        ImagePipelineStage::ComputeShader => RenderGraphDiagnosticImageStage::ComputeShader,
        ImagePipelineStage::ColorAttachmentOutput => {
            RenderGraphDiagnosticImageStage::ColorAttachmentOutput
        }
        ImagePipelineStage::DepthStencil => RenderGraphDiagnosticImageStage::DepthStencil,
        ImagePipelineStage::Transfer => RenderGraphDiagnosticImageStage::Transfer,
        ImagePipelineStage::Present => RenderGraphDiagnosticImageStage::Present,
        ImagePipelineStage::AllGraphics => RenderGraphDiagnosticImageStage::AllGraphics,
    };
    let mode = |mode: super::access::ImageAccessMode| match mode {
        ImageAccessMode::Read => RenderGraphDiagnosticImageAccessMode::Read,
        ImageAccessMode::Write => RenderGraphDiagnosticImageAccessMode::Write,
        ImageAccessMode::ReadWrite => RenderGraphDiagnosticImageAccessMode::ReadWrite,
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

fn resource_ref(
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

fn escape_dot(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::render_graph::compiler::GraphCompiler;
    use crate::render_graph::resource::ResourceState;
    use crate::texture::ImageFormat;

    fn namespace_resource(name: &str) -> GraphResourceDesc {
        GraphResourceDesc {
            name: name.to_string(),
            resource_type: GraphResourceType::SampledImage,
            format: ImageFormat::R8G8B8A8Unorm,
            width: 0,
            height: 0,
            tracks_swapchain_size: false,
        }
    }

    fn transient_resource(name: &str) -> GraphResourceDesc {
        GraphResourceDesc {
            name: name.to_string(),
            resource_type: GraphResourceType::ColorAttachment { clear_value: None },
            format: ImageFormat::R8G8B8A8Unorm,
            width: 128,
            height: 64,
            tracks_swapchain_size: true,
        }
    }

    fn pass(name: &str, reads: Vec<ResourceId>, writes: Vec<ResourceId>) -> PassDesc {
        PassDesc::new(name, PassType::Graphics, reads, writes)
    }

    fn diagnostics() -> RenderGraphDiagnostics {
        let resources = vec![
            namespace_resource(BACKBUFFER_NAME),
            namespace_resource("color"),
            namespace_resource("post"),
        ];
        let transient_resources = vec![transient_resource("color"), transient_resource("post")];
        let passes = vec![
            pass("geometry", Vec::new(), vec![ResourceId(1)]),
            pass("post", vec![ResourceId(1)], vec![ResourceId(2)]),
            pass("feedback", vec![ResourceId(2)], vec![ResourceId(1)]),
            pass("present", vec![ResourceId(1)], vec![ResourceId(0)]),
        ];
        let exported_resources = BTreeSet::from([ResourceId(0)]);
        let plan = GraphCompiler::from_pass_descs_with_exports(
            &passes,
            exported_resources.iter().copied(),
            BTreeMap::new(),
        )
        .compile()
        .unwrap();
        RenderGraphDiagnostics::from_parts(
            &passes,
            &resources,
            &transient_resources,
            &exported_resources,
            &BTreeMap::new(),
            &plan,
        )
    }

    #[test]
    fn pass_traces_expose_declared_attachment_ops() {
        let resources = vec![
            namespace_resource(BACKBUFFER_NAME),
            namespace_resource("color"),
        ];
        let transient_resources = vec![transient_resource("color")];
        let mut geometry = pass("geometry", Vec::new(), vec![ResourceId(1)]);
        geometry.color_attachments.push((
            ResourceId(1),
            crate::render_pass::AttachmentOps {
                load: crate::render_pass::LoadOp::Clear,
                store: crate::render_pass::StoreOp::Store,
                clear_value: crate::render_pass::ClearValue::OPAQUE_BLACK,
            },
        ));
        geometry.depth_attachment =
            Some(crate::render_pass::DepthStencilAttachmentOps::reverse_z_default());
        let mut present = pass("present", vec![ResourceId(1)], vec![ResourceId(0)]);
        present
            .color_attachments
            .push((ResourceId(0), crate::render_pass::AttachmentOps::load()));
        let passes = vec![geometry, present];

        let exported_resources = BTreeSet::from([ResourceId(0)]);
        let plan = GraphCompiler::from_pass_descs_with_exports(
            &passes,
            exported_resources.iter().copied(),
            BTreeMap::new(),
        )
        .compile()
        .unwrap();
        let diagnostics = RenderGraphDiagnostics::from_parts(
            &passes,
            &resources,
            &transient_resources,
            &exported_resources,
            &BTreeMap::new(),
            &plan,
        );

        let trace = diagnostics.to_string();
        assert!(
            trace.contains("colors [r1 Clear->Store]"),
            "geometry trace missing declared color ops: {trace}"
        );
        assert!(
            trace.contains("depth [Clear->Store, stencil Clear->DontCare]"),
            "geometry trace missing declared depth ops: {trace}"
        );
        assert!(
            trace.contains("colors [r0 Load->Store]"),
            "present trace missing declared color ops: {trace}"
        );
    }

    #[test]
    fn diagnostics_are_deterministic_and_machine_readable() {
        let first = diagnostics();
        let expected_json = first.to_json_pretty().unwrap();
        let expected_dot = first.to_dot();

        for _ in 0..32 {
            let current = diagnostics();
            assert_eq!(current, first);
            assert_eq!(current.to_json_pretty().unwrap(), expected_json);
            assert_eq!(current.to_dot(), expected_dot);
        }

        let json: Value = serde_json::from_str(&expected_json).unwrap();
        assert_eq!(
            json["schema_version"],
            RENDER_GRAPH_DIAGNOSTICS_SCHEMA_VERSION
        );
        assert_eq!(json["execution_order"], serde_json::json!([0, 1, 2, 3]));
        assert_eq!(json["passes"][0]["image_accesses"][0]["mode"], "write");
        assert_eq!(json["passes"][1]["image_accesses"][0]["usage"], "sampled");
        assert_eq!(json["summary"]["dependency_edges"], 4);
        assert_eq!(json["summary"]["synchronization_transitions"], 9);
        assert_eq!(json["summary"]["physical_transient_allocations"], 2);
        assert_eq!(json["summary"]["logical_transient_bytes"], 65536);
        assert_eq!(json["summary"]["physical_transient_bytes"], 65536);
        assert_eq!(json["summary"]["transient_alias_savings_bytes"], 0);
        assert_eq!(json["summary"]["parallel_levels"], 4);
    }

    #[test]
    fn diagnostics_name_every_raw_war_and_waw_hazard() {
        let diagnostics = diagnostics();
        let feedback = diagnostics
            .dependencies
            .iter()
            .find(|dependency| dependency.from_pass == 1 && dependency.to_pass == 2)
            .unwrap();

        assert_eq!(
            feedback.hazards,
            vec![
                RenderGraphDiagnosticHazard {
                    kind: RenderGraphHazardKind::War,
                    resource: RenderGraphDiagnosticResourceRef {
                        id: 1,
                        name: "color".to_string(),
                    },
                },
                RenderGraphDiagnosticHazard {
                    kind: RenderGraphHazardKind::Raw,
                    resource: RenderGraphDiagnosticResourceRef {
                        id: 2,
                        name: "post".to_string(),
                    },
                },
            ]
        );

        let geometry_to_feedback = diagnostics
            .dependencies
            .iter()
            .find(|dependency| dependency.from_pass == 0 && dependency.to_pass == 2)
            .unwrap();
        assert_eq!(
            geometry_to_feedback.hazards[0].kind,
            RenderGraphHazardKind::Waw
        );
    }

    #[test]
    fn diagnostics_expose_stable_physical_allocation_ids_and_memory_totals() {
        let resources = vec![
            namespace_resource(BACKBUFFER_NAME),
            transient_resource("early"),
            transient_resource("late"),
        ];
        let transient_resources = vec![transient_resource("early"), transient_resource("late")];
        let passes = vec![
            pass("write_early", Vec::new(), vec![ResourceId(1)]),
            pass("consume_early", vec![ResourceId(1)], vec![ResourceId(0)]),
            pass("write_late", vec![ResourceId(0)], vec![ResourceId(2)]),
            pass("consume_late", vec![ResourceId(2)], vec![ResourceId(0)]),
        ];
        let exported_resources = BTreeSet::from([ResourceId(0)]);
        let plan = GraphCompiler::from_pass_descs_with_exports(
            &passes,
            exported_resources.iter().copied(),
            BTreeMap::new(),
        )
        .compile()
        .unwrap();
        let diagnostics = RenderGraphDiagnostics::from_parts(
            &passes,
            &resources,
            &transient_resources,
            &exported_resources,
            &BTreeMap::new(),
            &plan,
        );

        assert_eq!(diagnostics.resources[1].physical_allocation_id, Some(0));
        assert_eq!(diagnostics.resources[2].physical_allocation_id, Some(0));
        assert_eq!(diagnostics.summary.physical_transient_allocations, 1);
        assert_eq!(diagnostics.summary.logical_transient_bytes, 65536);
        assert_eq!(diagnostics.summary.physical_transient_bytes, 32768);
        assert_eq!(diagnostics.summary.transient_alias_savings_bytes, 32768);
    }

    #[test]
    fn diagnostics_expose_compiler_owned_synchronization_transitions() {
        let diagnostics = diagnostics();
        let raw = diagnostics
            .synchronization
            .iter()
            .find(|transition| {
                transition.before_pass == Some(0)
                    && transition.to_pass == Some(1)
                    && transition.resource.id == 1
            })
            .unwrap();

        assert_eq!(raw.hazard, Some(RenderGraphHazardKind::Raw));
        assert_eq!(raw.reason, RenderGraphDiagnosticSyncReason::Hazard);
        // The fixture's coarse write infers a whole-resource storage access.
        assert_eq!(
            raw.before_state,
            RenderGraphDiagnosticSyncState::Access {
                usage: RenderGraphDiagnosticImageUsage::Storage,
                stage: RenderGraphDiagnosticImageStage::AllGraphics,
                mode: RenderGraphDiagnosticImageAccessMode::Write,
            }
        );
        assert_eq!(
            raw.after_state,
            RenderGraphDiagnosticSyncState::Access {
                usage: RenderGraphDiagnosticImageUsage::Sampled,
                stage: RenderGraphDiagnosticImageStage::FragmentShader,
                mode: RenderGraphDiagnosticImageAccessMode::Read,
            }
        );
        assert_eq!(raw.before_name.as_deref(), Some("geometry"));
        assert_eq!(raw.to_name.as_deref(), Some("post"));
    }

    #[test]
    fn diagnostics_include_resource_lifetimes_and_origins() {
        let diagnostics = diagnostics();
        assert_eq!(
            diagnostics.resources[0].origin,
            RenderGraphDiagnosticResourceOrigin::BuiltIn
        );
        assert_eq!(
            diagnostics.resources[1].origin,
            RenderGraphDiagnosticResourceOrigin::Transient
        );
        assert_eq!(
            diagnostics.resources[1].lifetime,
            Some(RenderGraphDiagnosticResourceLifetime {
                first_execution_position: 0,
                first_pass: 0,
                last_execution_position: 3,
                last_pass: 3,
            })
        );
        assert_eq!(diagnostics.resources[1].width, Some(128));
        assert!(diagnostics.resources[0].exported);
        assert!(!diagnostics.resources[1].exported);
    }

    #[test]
    fn dot_output_distinguishes_passes_resources_and_hazards() {
        let dot = diagnostics().to_dot();
        assert!(dot.starts_with("digraph render_graph"));
        assert!(dot.contains("r1 [shape=ellipse"));
        assert!(dot.contains("p0 [shape=box"));
        assert!(dot.contains("p0 -> r1 [label=\"write Storage @ AllGraphics"));
        assert!(dot.contains("r1 -> p1 [label=\"read Sampled @ FragmentShader"));
        assert!(dot.contains("Waw color"));
        assert!(dot.contains("Raw post"));
    }

    #[test]
    fn diagnostics_expose_imported_state_contracts() {
        let resources = vec![
            namespace_resource(BACKBUFFER_NAME),
            namespace_resource("external"),
            namespace_resource("color"),
        ];
        let transient_resources = vec![transient_resource("color")];
        let passes = vec![
            pass("paint", Vec::new(), vec![ResourceId(2)]),
            pass("present", Vec::new(), vec![ResourceId(0)]),
        ];
        let exported_resources = BTreeSet::from([ResourceId(0)]);
        let imported_contracts = BTreeMap::from([
            (
                ResourceId(0),
                ImportedImageContract::arrives_in(ResourceState::ColorAttachment)
                    .must_end_in(ResourceState::PresentSrc),
            ),
            (
                ResourceId(1),
                ImportedImageContract::arrives_in(ResourceState::ShaderRead),
            ),
        ]);
        let plan = GraphCompiler::from_pass_descs_with_exports(
            &passes,
            exported_resources.iter().copied(),
            BTreeMap::new(),
        )
        .compile()
        .unwrap();
        let diagnostics = RenderGraphDiagnostics::from_parts(
            &passes,
            &resources,
            &transient_resources,
            &exported_resources,
            &imported_contracts,
            &plan,
        );

        assert_eq!(
            diagnostics.resources[0].imported_contract,
            Some(RenderGraphDiagnosticImportedContract {
                initial: "ColorAttachment".to_string(),
                required_final: Some("PresentSrc".to_string()),
            })
        );
        assert_eq!(
            diagnostics.resources[1].imported_contract,
            Some(RenderGraphDiagnosticImportedContract {
                initial: "ShaderRead".to_string(),
                required_final: None,
            })
        );
        assert_eq!(diagnostics.resources[2].imported_contract, None);

        let json = serde_json::to_value(&diagnostics).unwrap();
        assert_eq!(
            json["resources"][0]["imported_contract"]["required_final"],
            "PresentSrc"
        );

        let text = diagnostics.to_string();
        assert!(text.contains("r0 (backbuffer) imported, initial ColorAttachment"));
        assert!(text.contains("required final PresentSrc"));
        assert!(text.contains("r1 (external) imported, initial ShaderRead"));
        assert!(text.contains("required final unconstrained"));

        let dot = diagnostics.to_dot();
        assert!(dot.contains("initial ColorAttachment, final PresentSrc"));
    }

    #[test]
    fn diagnostics_expose_live_culled_exported_and_side_effect_state() {
        let resources = vec![
            namespace_resource(BACKBUFFER_NAME),
            namespace_resource("dead"),
            namespace_resource("telemetry_input"),
        ];
        let transient_resources = vec![
            transient_resource("dead"),
            transient_resource("telemetry_input"),
        ];
        let mut telemetry = pass("telemetry", vec![ResourceId(2)], Vec::new());
        telemetry.side_effect = true;
        let passes = vec![
            pass("present", Vec::new(), vec![ResourceId(0)]),
            pass("dead_branch", Vec::new(), vec![ResourceId(1)]),
            pass("telemetry_source", Vec::new(), vec![ResourceId(2)]),
            telemetry,
        ];
        let exported_resources = BTreeSet::from([ResourceId(0)]);
        let plan = GraphCompiler::from_pass_descs_with_exports(
            &passes,
            exported_resources.iter().copied(),
            BTreeMap::new(),
        )
        .compile()
        .unwrap();
        let diagnostics = RenderGraphDiagnostics::from_parts(
            &passes,
            &resources,
            &transient_resources,
            &exported_resources,
            &BTreeMap::new(),
            &plan,
        );

        assert_eq!(diagnostics.summary.declared_passes, 4);
        assert_eq!(diagnostics.summary.live_passes, 3);
        assert_eq!(diagnostics.summary.culled_passes, 1);
        assert!(diagnostics.passes[0].live);
        assert!(diagnostics.passes[1].culled);
        assert_eq!(diagnostics.passes[1].execution_position, None);
        assert_eq!(diagnostics.passes[1].parallel_level, None);
        assert!(diagnostics.passes[3].side_effect);
        assert!(diagnostics.resources[0].exported);

        let dot = diagnostics.to_dot();
        assert!(dot.contains("exported"));
        assert!(dot.contains("culled"));
        assert!(dot.contains("side-effect"));
        assert!(diagnostics.to_string().contains("3 live, 1 culled"));
    }

    #[test]
    fn test_typed_access_exports_preserve_ranges_and_culled_passes() {
        use crate::render_graph::access::{ImageAspects, ImageSubresourceRange};

        let resources = vec![namespace_resource("depth\"atlas")];
        let access = ImageAccess::storage_read_write(ResourceId(0)).with_range(
            ImageSubresourceRange::new(ImageAspects::DEPTH | ImageAspects::STENCIL, 2, 3, 4, 5),
        );
        let mut live = pass("live", vec![], vec![]).with_image_accesses([access]);
        live.side_effect = true;
        let passes = vec![
            live,
            pass("unused", vec![], vec![]).with_image_accesses([access]),
        ];
        let plan = GraphCompiler::from_pass_descs_with_exports(&passes, [], BTreeMap::new())
            .compile()
            .unwrap();
        let diagnostics = RenderGraphDiagnostics::from_parts(
            &passes,
            &resources,
            &[],
            &BTreeSet::new(),
            &BTreeMap::new(),
            &plan,
        );
        let label = "read_write Storage @ AllGraphics, depth|stencil, mips 2+3, layers 4+5";
        let text = diagnostics.to_string();
        assert_eq!(text.matches(label).count(), 2);
        assert!(text.contains("[1] unused (culled)"));
        let dot = diagnostics.to_dot();
        for edge in ["r0 -> p0", "p0 -> r0"] {
            assert!(dot.contains(&format!("{edge} [label=\"{label}\"];")));
        }
        for edge in ["r0 -> p1", "p1 -> r0"] {
            assert!(dot.contains(&format!(
                "{edge} [label=\"{label}\",style=\"dotted\",color=\"gray60\"];"
            )));
        }
        assert!(dot.contains("depth\\\"atlas"));
        let json: Value = serde_json::from_str(&diagnostics.to_json_pretty().unwrap()).unwrap();
        assert_eq!(
            json["passes"][0]["image_accesses"][0]["range"],
            serde_json::json!({
                "aspects": ["depth", "stencil"],
                "base_mip_level": 2, "mip_level_count": 3,
                "base_array_layer": 4, "array_layer_count": 5,
            })
        );
        assert_eq!(diagnostics.execution_order, vec![0]);
    }

    #[test]
    fn dot_output_escapes_unstable_user_names() {
        assert_eq!(escape_dot("a\\b\"c\nd"), "a\\\\b\\\"c\\nd");
    }
}
