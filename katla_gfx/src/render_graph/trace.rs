//! Deterministic trace of the encoders a backend actually emitted.
//!
//! The compiled plan says what *should* run; this records what *did*. A backend
//! appends one [`ResourceExecutionTraceEntry`] per pass it encoded, in encode
//! order, carrying the pass identity, the declared attachment contract, and
//! backend-neutral counts. Nothing here names a native type, driver id, or
//! address, so a trace is comparable byte-for-byte across runs and machines.
//!
//! [`compare_with_compiled`] is the point of the trace: it fails when the
//! emitted pass order or the per-pass attachment contract diverges from the
//! compiled plan, which is the divergence a plan-only view cannot show.

use std::fmt;

use super::pass::{PassDesc, PassType};
use super::resource::GraphResourceDesc;

/// Why a compiled pass produced no encoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmittedPassOutcome {
    /// The backend created an encoder for the pass.
    Encoded,
    /// The pass had no work to encode, so the backend skipped it.
    SkippedNoWork,
}

impl fmt::Display for EmittedPassOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encoded => f.write_str("encoded"),
            Self::SkippedNoWork => f.write_str("skipped_no_work"),
        }
    }
}

/// One encoder a backend emitted for one compiled pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceExecutionTraceEntry {
    /// Declared pass index, so trace entries join the compiled plan directly.
    pub pass_index: usize,
    pub name: String,
    pub pass_type: PassType,
    /// Encoder order within the frame (0-based).
    pub encode_position: usize,
    pub outcome: EmittedPassOutcome,
    /// Draw calls encoded for this pass.
    pub draw_calls: usize,
    /// Object slots those draws covered.
    pub instances: usize,
    /// Color targets the encoder actually bound, in declaration order.
    pub color_targets: Vec<String>,
    /// Depth target the encoder actually bound, if any.
    pub depth_target: Option<String>,
}

/// A frame's emitted encoder trace.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResourceExecutionTrace {
    entries: Vec<ResourceExecutionTraceEntry>,
}

impl ResourceExecutionTrace {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, entry: ResourceExecutionTraceEntry) {
        self.entries.push(entry);
    }

    pub fn entries(&self) -> &[ResourceExecutionTraceEntry] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Passes the backend created an encoder for, in encode order.
    pub fn encoded_passes(&self) -> impl Iterator<Item = &ResourceExecutionTraceEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.outcome == EmittedPassOutcome::Encoded)
    }
}

/// A divergence between the compiled plan and the emitted trace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraceDivergence {
    /// A live compiled pass has no trace entry at all.
    MissingPass { pass_index: usize, name: String },
    /// The trace contains a pass index the compiled plan does not know.
    UnknownPass { pass_index: usize, name: String },
    /// Encoded passes are out of compiled execution order.
    OrderMismatch {
        expected: Vec<String>,
        emitted: Vec<String>,
    },
    /// An encoded pass bound a different set of color targets than declared.
    ColorTargetsMismatch {
        pass_index: usize,
        name: String,
        compiled: Vec<String>,
        emitted: Vec<String>,
    },
    /// An encoded pass bound a different depth target than declared.
    DepthTargetMismatch {
        pass_index: usize,
        name: String,
        compiled: Option<String>,
        emitted: Option<String>,
    },
}

impl fmt::Display for TraceDivergence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPass { pass_index, name } => write!(
                f,
                "compiled pass {pass_index} ('{name}') has no emitted encoder entry"
            ),
            Self::UnknownPass { pass_index, name } => write!(
                f,
                "emitted encoder entry {pass_index} ('{name}') is not a compiled pass"
            ),
            Self::OrderMismatch { expected, emitted } => write!(
                f,
                "encoded pass order differs from the compiled plan: compiled [{}], emitted [{}]",
                expected.join(", "),
                emitted.join(", ")
            ),
            Self::ColorTargetsMismatch {
                pass_index,
                name,
                compiled,
                emitted,
            } => write!(
                f,
                "pass {pass_index} ('{name}') bound color targets [{}] but declared [{}]",
                emitted.join(", "),
                compiled.join(", ")
            ),
            Self::DepthTargetMismatch {
                pass_index,
                name,
                compiled,
                emitted,
            } => write!(
                f,
                "pass {pass_index} ('{name}') bound depth target {} but declared {}",
                emitted.as_deref().unwrap_or("none"),
                compiled.as_deref().unwrap_or("none"),
            ),
        }
    }
}

/// Label for the backend-owned frame depth texture.
///
/// A pass's depth contract names operations, not a graph resource: the depth
/// attachment is the frame's depth texture on both backends. The trace records
/// this stable label so an emitted depth binding is distinguishable from none.
pub const FRAME_DEPTH_TARGET: &str = "depth";

/// Resource names for a pass's declared color targets, in declaration order.
pub(crate) fn color_target_names(resources: &[GraphResourceDesc], pass: &PassDesc) -> Vec<String> {
    pass.color_attachments
        .iter()
        .filter_map(|(id, _)| resources.get(id.0 as usize))
        .map(|resource| resource.name.clone())
        .collect()
}

/// Compare an emitted trace against the compiled pass list.
///
/// Returns every divergence found rather than the first, so one run reports the
/// whole disagreement. An empty result means the backend emitted exactly the
/// compiled passes, in order, against the declared attachment contract.
///
/// Only encoded passes are compared for order and attachments: a pass the
/// backend deliberately skipped for lack of work is recorded but not treated as
/// a divergence.
pub fn compare_with_compiled(
    resources: &[GraphResourceDesc],
    passes: &[PassDesc],
    execution_order: &[usize],
    trace: &ResourceExecutionTrace,
) -> Vec<TraceDivergence> {
    let mut divergences = Vec::new();

    let live: Vec<usize> = execution_order
        .iter()
        .copied()
        .filter(|&index| index < passes.len())
        .collect();

    let mut seen: Vec<usize> = trace.entries.iter().map(|entry| entry.pass_index).collect();
    seen.sort_unstable();
    seen.dedup();

    for &pass_index in &live {
        if !seen.contains(&pass_index) {
            divergences.push(TraceDivergence::MissingPass {
                pass_index,
                name: passes[pass_index].name.clone(),
            });
        }
    }
    for &pass_index in &seen {
        if !live.contains(&pass_index) {
            divergences.push(TraceDivergence::UnknownPass {
                pass_index,
                name: passes
                    .get(pass_index)
                    .map(|pass| pass.name.clone())
                    .unwrap_or_else(|| "<out of range>".to_string()),
            });
        }
    }

    let encoded_order = trace
        .encoded_passes()
        .map(|entry| entry.pass_index)
        .collect::<Vec<_>>();
    // Compare against the compiled order restricted to passes that were encoded,
    // so a skipped pass does not read as an ordering divergence.
    let expected_order = live
        .iter()
        .copied()
        .filter(|index| encoded_order.contains(index))
        .collect::<Vec<_>>();
    if encoded_order != expected_order {
        divergences.push(TraceDivergence::OrderMismatch {
            expected: expected_order
                .iter()
                .map(|&index| passes[index].name.clone())
                .collect(),
            emitted: encoded_order
                .iter()
                .map(|&index| {
                    passes
                        .get(index)
                        .map(|pass| pass.name.clone())
                        .unwrap_or_else(|| "<out of range>".to_string())
                })
                .collect(),
        });
    }

    for entry in trace.encoded_passes() {
        let Some(pass) = passes.get(entry.pass_index) else {
            continue;
        };
        if pass.pass_type != PassType::Graphics {
            continue;
        }

        let compiled_colors = color_target_names(resources, pass);
        if compiled_colors != entry.color_targets {
            divergences.push(TraceDivergence::ColorTargetsMismatch {
                pass_index: entry.pass_index,
                name: pass.name.clone(),
                compiled: compiled_colors,
                emitted: entry.color_targets.clone(),
            });
        }

        // The declared depth fact is the pass's contract, not a graph
        // resource name: the depth attachment is the frame's depth texture.
        let compiled_depth = pass.uses_depth.then(|| FRAME_DEPTH_TARGET.to_string());
        if compiled_depth != entry.depth_target {
            divergences.push(TraceDivergence::DepthTargetMismatch {
                pass_index: entry.pass_index,
                name: pass.name.clone(),
                compiled: compiled_depth,
                emitted: entry.depth_target.clone(),
            });
        }
    }

    divergences
}

impl fmt::Display for ResourceExecutionTrace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.entries.is_empty() {
            return writeln!(f, "no encoders emitted");
        }

        writeln!(f, "{} emitted encoder entries:", self.entries.len())?;
        for entry in &self.entries {
            let targets = if entry.color_targets.is_empty() {
                "none".to_string()
            } else {
                entry.color_targets.join(", ")
            };
            writeln!(
                f,
                "  [{}] pass {} ({}, {:?}) {} color [{}] depth {} draws {} instances {}",
                entry.encode_position,
                entry.pass_index,
                entry.name,
                entry.pass_type,
                entry.outcome,
                targets,
                entry.depth_target.as_deref().unwrap_or("none"),
                entry.draw_calls,
                entry.instances,
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_graph::handles::ResourceId;
    use crate::render_graph::pass::PassKind;
    use crate::render_pass::AttachmentOps;
    use crate::texture::ImageFormat;

    fn resource(name: &str) -> GraphResourceDesc {
        GraphResourceDesc {
            name: name.to_string(),
            resource_type: super::super::resource::GraphResourceType::ColorAttachment {
                clear_value: None,
            },
            format: ImageFormat::R8G8B8A8Unorm,
            width: 64,
            height: 64,
            tracks_swapchain_size: false,
        }
    }

    fn graphics_pass(name: &str, colors: Vec<ResourceId>) -> PassDesc {
        let mut pass = PassDesc::new(name, PassType::Graphics, Vec::new(), colors.clone());
        pass.color_attachments = colors
            .into_iter()
            .map(|id| (id, AttachmentOps::load()))
            .collect();
        pass.uses_depth = false;
        pass
    }

    fn entry(pass_index: usize, name: &str, position: usize) -> ResourceExecutionTraceEntry {
        ResourceExecutionTraceEntry {
            pass_index,
            name: name.to_string(),
            pass_type: PassType::Graphics,
            encode_position: position,
            outcome: EmittedPassOutcome::Encoded,
            draw_calls: 1,
            instances: 1,
            color_targets: Vec::new(),
            depth_target: None,
        }
    }

    #[test]
    fn a_matching_trace_has_no_divergences() {
        let resources = vec![resource("color")];
        let passes = vec![graphics_pass("only", vec![ResourceId(0)])];
        let mut trace = ResourceExecutionTrace::new();
        let mut e = entry(0, "only", 0);
        e.color_targets = vec!["color".to_string()];
        trace.push(e);

        assert_eq!(
            compare_with_compiled(&resources, &passes, &[0], &trace),
            vec![]
        );
    }

    #[test]
    fn a_live_pass_the_backend_never_encoded_is_reported() {
        let resources = vec![resource("color")];
        let passes = vec![graphics_pass("skipped", vec![ResourceId(0)])];

        let divergences =
            compare_with_compiled(&resources, &passes, &[0], &ResourceExecutionTrace::new());

        assert_eq!(
            divergences,
            vec![TraceDivergence::MissingPass {
                pass_index: 0,
                name: "skipped".to_string()
            }]
        );
    }

    #[test]
    fn an_encoded_pass_outside_the_compiled_plan_is_reported() {
        let resources = vec![resource("color")];
        let passes = vec![graphics_pass("only", vec![ResourceId(0)])];
        let mut trace = ResourceExecutionTrace::new();
        trace.push(entry(7, "ghost", 0));

        let divergences = compare_with_compiled(&resources, &passes, &[0], &trace);

        assert!(divergences.contains(&TraceDivergence::UnknownPass {
            pass_index: 7,
            name: "<out of range>".to_string()
        }));
    }

    #[test]
    fn encoded_passes_in_the_wrong_order_are_reported() {
        let resources = vec![resource("color")];
        let passes = vec![
            graphics_pass("first", vec![ResourceId(0)]),
            graphics_pass("second", vec![ResourceId(0)]),
        ];
        let mut trace = ResourceExecutionTrace::new();
        let mut second = entry(1, "second", 0);
        second.color_targets = vec!["color".to_string()];
        let mut first = entry(0, "first", 1);
        first.color_targets = vec!["color".to_string()];
        trace.push(second);
        trace.push(first);

        let divergences = compare_with_compiled(&resources, &passes, &[0, 1], &trace);

        assert!(divergences.iter().any(|d| matches!(
            d,
            TraceDivergence::OrderMismatch { expected, emitted }
                if expected == &["first".to_string(), "second".to_string()]
                    && emitted == &["second".to_string(), "first".to_string()]
        )));
    }

    #[test]
    fn a_color_target_the_encoder_did_not_bind_is_reported() {
        let resources = vec![resource("declared")];
        let passes = vec![graphics_pass("pass", vec![ResourceId(0)])];
        let mut trace = ResourceExecutionTrace::new();
        let mut e = entry(0, "pass", 0);
        e.color_targets = vec!["other".to_string()];
        trace.push(e);

        let divergences = compare_with_compiled(&resources, &passes, &[0], &trace);

        assert!(
            divergences.contains(&TraceDivergence::ColorTargetsMismatch {
                pass_index: 0,
                name: "pass".to_string(),
                compiled: vec!["declared".to_string()],
                emitted: vec!["other".to_string()],
            })
        );
    }

    #[test]
    fn a_depth_binding_the_pass_did_not_declare_is_reported() {
        let resources = vec![resource("color")];
        let mut pass = graphics_pass("pass", vec![ResourceId(0)]);
        pass.uses_depth = false;
        let mut trace = ResourceExecutionTrace::new();
        let mut e = entry(0, "pass", 0);
        e.color_targets = vec!["color".to_string()];
        e.depth_target = Some(FRAME_DEPTH_TARGET.to_string());
        trace.push(e);

        let divergences = compare_with_compiled(&resources, &[pass], &[0], &trace);

        assert!(divergences.contains(&TraceDivergence::DepthTargetMismatch {
            pass_index: 0,
            name: "pass".to_string(),
            compiled: None,
            emitted: Some(FRAME_DEPTH_TARGET.to_string()),
        }));
    }

    #[test]
    fn a_skipped_pass_is_recorded_but_not_an_ordering_divergence() {
        let resources = vec![resource("color")];
        let passes = vec![
            graphics_pass("empty", vec![ResourceId(0)]),
            graphics_pass("real", vec![ResourceId(0)]),
        ];
        let mut trace = ResourceExecutionTrace::new();
        let mut first = entry(0, "empty", 0);
        first.outcome = EmittedPassOutcome::SkippedNoWork;
        first.color_targets = vec!["color".to_string()];
        let mut second = entry(1, "real", 1);
        second.color_targets = vec!["color".to_string()];
        trace.push(first);
        trace.push(second);

        assert_eq!(
            compare_with_compiled(&resources, &passes, &[0, 1], &trace),
            vec![]
        );
    }

    #[test]
    fn divergence_messages_name_the_pass_and_both_sides() {
        let divergence = TraceDivergence::OrderMismatch {
            expected: vec!["a".to_string()],
            emitted: vec!["b".to_string()],
        };
        let message = divergence.to_string();
        assert!(message.contains("compiled [a]"));
        assert!(message.contains("emitted [b]"));

        let missing = TraceDivergence::MissingPass {
            pass_index: 2,
            name: "shadow".to_string(),
        };
        assert!(missing.to_string().contains("pass 2 ('shadow')"));
    }

    #[test]
    fn text_export_lists_every_encoder_in_order() {
        let mut trace = ResourceExecutionTrace::new();
        let mut e = entry(0, "geometry", 0);
        e.color_targets = vec!["hdr".to_string()];
        trace.push(e);

        let text = trace.to_string();

        assert!(text.contains("1 emitted encoder entries"));
        assert!(text.contains("geometry"));
        assert!(text.contains("color [hdr]"));
        assert_eq!(
            ResourceExecutionTrace::new().to_string(),
            "no encoders emitted\n"
        );
    }

    #[test]
    fn the_kind_field_does_not_affect_comparison() {
        // Only declared attachments and order are compared, so a pass kind that
        // the encoders route differently is not itself a divergence.
        let resources = vec![resource("color")];
        let mut pass = graphics_pass("pass", vec![ResourceId(0)]);
        pass.kind = Some(PassKind::Ui);
        let mut trace = ResourceExecutionTrace::new();
        let mut e = entry(0, "pass", 0);
        e.color_targets = vec!["color".to_string()];
        trace.push(e);

        assert_eq!(
            compare_with_compiled(&resources, &[pass], &[0], &trace),
            vec![]
        );
    }
}
