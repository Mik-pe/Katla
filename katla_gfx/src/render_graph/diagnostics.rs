//! Deterministic diagnostics for compiled render graphs.
//!
//! Diagnostics intentionally contain only stable graph data: declaration indices,
//! resource names, access hazards, execution order, and parallel levels. Backend
//! pointers, device addresses, and hash-map iteration order never enter the output.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write as _};

use serde::Serialize;

use super::BACKBUFFER_NAME;
use super::backend::RenderGraphBackend;
use super::compiler::{ExecutionPlan, GraphCompiler};
use super::error::RenderGraphError;
use super::frame_graph::FrameGraph;
use super::handles::ResourceId;
use super::pass::{PassDesc, PassType};
use super::resource::{GraphResourceDesc, GraphResourceType};

/// Schema version for serialized render-graph diagnostics.
pub const RENDER_GRAPH_DIAGNOSTICS_SCHEMA_VERSION: u32 = 1;

/// Stable, backend-neutral snapshot of a render graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnostics {
    pub schema_version: u32,
    pub summary: RenderGraphDiagnosticSummary,
    pub resources: Vec<RenderGraphDiagnosticResource>,
    pub passes: Vec<RenderGraphDiagnosticPass>,
    pub dependencies: Vec<RenderGraphDiagnosticDependency>,
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
    pub lifetime: Option<RenderGraphDiagnosticResourceLifetime>,
    /// Populated once transient aliasing provides stable physical allocation IDs.
    pub physical_allocation_id: Option<u32>,
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

/// Pass information with canonical DAG metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderGraphDiagnosticPass {
    pub index: usize,
    pub name: String,
    pub pass_type: RenderGraphDiagnosticPassType,
    pub kind: Option<String>,
    pub reads: Vec<RenderGraphDiagnosticResourceRef>,
    pub writes: Vec<RenderGraphDiagnosticResourceRef>,
    pub predecessors: Vec<usize>,
    pub successors: Vec<usize>,
    pub execution_position: usize,
    pub parallel_level: usize,
    pub live: bool,
    pub culled: bool,
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

impl<B: RenderGraphBackend> FrameGraph<B> {
    /// Build a deterministic diagnostics snapshot from the graph's canonical compiler.
    ///
    /// The compiler is pure, so diagnostics can be requested without allocating GPU
    /// resources or mutating frame execution state.
    pub fn diagnostics(&self) -> Result<RenderGraphDiagnostics, RenderGraphError> {
        let plan = GraphCompiler::from_pass_descs(&self.passes).compile()?;
        Ok(RenderGraphDiagnostics::from_parts(
            &self.passes,
            &self.resources,
            &self.transient_resources,
            &plan,
        ))
    }
}

impl RenderGraphDiagnostics {
    fn from_parts(
        passes: &[PassDesc],
        resources: &[GraphResourceDesc],
        transient_resources: &[GraphResourceDesc],
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
        let lifetimes = resource_lifetimes(passes, &plan.sorted_passes);

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
                    lifetime: lifetimes.get(&(index as u32)).cloned(),
                    physical_allocation_id: None,
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
                    predecessors: node.predecessors.clone(),
                    successors: node.successors.clone(),
                    execution_position: execution_positions[&node.pass_index],
                    parallel_level: node.level,
                    live: true,
                    culled: false,
                }
            })
            .collect::<Vec<_>>();

        let dependencies = dependency_diagnostics(passes, resources, plan);
        let summary = RenderGraphDiagnosticSummary {
            declared_passes: passes.len(),
            live_passes: passes.len(),
            culled_passes: 0,
            resources: diagnostic_resources.len(),
            dependency_edges: dependencies.len(),
            parallel_levels: plan.parallel_groups.len(),
        };

        Self {
            schema_version: RENDER_GRAPH_DIAGNOSTICS_SCHEMA_VERSION,
            summary,
            resources: diagnostic_resources,
            passes: diagnostic_passes,
            dependencies,
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
            let _ = writeln!(
                output,
                "  r{} [shape=ellipse,label=\"{}: {}\\n{}\"];",
                resource.id,
                resource.id,
                escape_dot(&resource.name),
                origin
            );
        }

        for pass in &self.passes {
            let _ = writeln!(
                output,
                "  p{} [shape=box,label=\"{}: {}\\nlevel {}\"];",
                pass.index,
                pass.index,
                escape_dot(&pass.name),
                pass.parallel_level
            );

            for resource in &pass.reads {
                let _ = writeln!(
                    output,
                    "  r{} -> p{} [label=\"read\"];",
                    resource.id, pass.index
                );
            }
            for resource in &pass.writes {
                let _ = writeln!(
                    output,
                    "  p{} -> r{} [label=\"write\"];",
                    pass.index, resource.id
                );
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
            "{} passes, {} resources, {} dependency edges, {} parallel levels",
            self.summary.live_passes,
            self.summary.resources,
            self.summary.dependency_edges,
            self.summary.parallel_levels
        )?;

        for &pass_index in &self.execution_order {
            let pass = &self.passes[pass_index];
            writeln!(
                f,
                "  [{}] {} (level {}, reads {}, writes {})",
                pass.index,
                pass.name,
                pass.parallel_level,
                pass.reads.len(),
                pass.writes.len()
            )?;
        }

        Ok(())
    }
}

fn dependency_diagnostics(
    passes: &[PassDesc],
    resources: &[GraphResourceDesc],
    plan: &ExecutionPlan,
) -> Vec<RenderGraphDiagnosticDependency> {
    let mut dependencies = Vec::new();

    for node in &plan.dag {
        for &successor in &node.successors {
            let from = &passes[node.pass_index];
            let to = &passes[successor];
            let mut hazards = Vec::new();

            append_hazards(
                &mut hazards,
                &from.writes,
                &to.reads,
                RenderGraphHazardKind::Raw,
                resources,
            );
            append_hazards(
                &mut hazards,
                &from.reads,
                &to.writes,
                RenderGraphHazardKind::War,
                resources,
            );
            append_hazards(
                &mut hazards,
                &from.writes,
                &to.writes,
                RenderGraphHazardKind::Waw,
                resources,
            );
            hazards.sort_by(|left, right| {
                left.resource
                    .id
                    .cmp(&right.resource.id)
                    .then(left.kind.cmp(&right.kind))
            });
            hazards.dedup();

            dependencies.push(RenderGraphDiagnosticDependency {
                from_pass: node.pass_index,
                from_name: from.name.clone(),
                to_pass: successor,
                to_name: to.name.clone(),
                hazards,
            });
        }
    }

    dependencies.sort_by_key(|dependency| (dependency.from_pass, dependency.to_pass));
    dependencies
}

fn append_hazards(
    hazards: &mut Vec<RenderGraphDiagnosticHazard>,
    left: &[ResourceId],
    right: &[ResourceId],
    kind: RenderGraphHazardKind,
    resources: &[GraphResourceDesc],
) {
    let right = right
        .iter()
        .map(|resource| resource.0)
        .collect::<BTreeSet<_>>();
    for resource in left {
        if right.contains(&resource.0) {
            hazards.push(RenderGraphDiagnosticHazard {
                kind,
                resource: resource_ref(*resource, resources),
            });
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

fn resource_lifetimes(
    passes: &[PassDesc],
    execution_order: &[usize],
) -> BTreeMap<u32, RenderGraphDiagnosticResourceLifetime> {
    let mut accesses = BTreeMap::<u32, Vec<(usize, usize)>>::new();

    for (position, &pass_index) in execution_order.iter().enumerate() {
        let pass = &passes[pass_index];
        let resources = pass
            .reads
            .iter()
            .chain(&pass.writes)
            .map(|resource| resource.0)
            .collect::<BTreeSet<_>>();

        for resource in resources {
            accesses
                .entry(resource)
                .or_default()
                .push((position, pass_index));
        }
    }

    accesses
        .into_iter()
        .filter_map(|(resource, accesses)| {
            let &(first_execution_position, first_pass) = accesses.first()?;
            let &(last_execution_position, last_pass) = accesses.last()?;
            Some((
                resource,
                RenderGraphDiagnosticResourceLifetime {
                    first_execution_position,
                    first_pass,
                    last_execution_position,
                    last_pass,
                },
            ))
        })
        .collect()
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
        let plan = GraphCompiler::from_pass_descs(&passes).compile().unwrap();
        RenderGraphDiagnostics::from_parts(&passes, &resources, &transient_resources, &plan)
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
        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["execution_order"], serde_json::json!([0, 1, 2, 3]));
        assert_eq!(json["summary"]["dependency_edges"], 4);
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
    }

    #[test]
    fn dot_output_distinguishes_passes_resources_and_hazards() {
        let dot = diagnostics().to_dot();
        assert!(dot.starts_with("digraph render_graph"));
        assert!(dot.contains("r1 [shape=ellipse"));
        assert!(dot.contains("p0 [shape=box"));
        assert!(dot.contains("p0 -> r1 [label=\"write\"]"));
        assert!(dot.contains("r1 -> p1 [label=\"read\"]"));
        assert!(dot.contains("Waw color"));
        assert!(dot.contains("Raw post"));
    }

    #[test]
    fn dot_output_escapes_unstable_user_names() {
        assert_eq!(escape_dot("a\\b\"c\nd"), "a\\\\b\\\"c\\nd");
    }
}
