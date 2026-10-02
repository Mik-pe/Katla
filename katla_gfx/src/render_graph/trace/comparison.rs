use super::*;

/// A divergence between the compiled plan and the emitted trace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraceDivergence {
    /// A live compiled pass has no trace entry at all.
    MissingPass { pass_index: usize, name: String },
    /// The trace contains a pass index the compiled plan does not know.
    UnknownPass { pass_index: usize, name: String },
    /// A dispatch entry was recorded more than once for one pass.
    DuplicatePass { pass_index: usize, name: String },
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
    /// Native load/store/clear operations differ from the compiled contract.
    AttachmentOpsMismatch {
        pass_index: usize,
        name: String,
        aspect: &'static str,
        compiled: String,
        emitted: String,
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
            Self::DuplicatePass { pass_index, name } => write!(
                f,
                "pass {pass_index} ('{name}') has duplicate emitted dispatch entries"
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
            Self::AttachmentOpsMismatch {
                pass_index,
                name,
                aspect,
                compiled,
                emitted,
            } => write!(
                f,
                "pass {pass_index} ('{name}') encoded {aspect} operations {emitted} but declared {compiled}"
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
    for pair in seen.windows(2) {
        if pair[0] == pair[1] {
            divergences.push(TraceDivergence::DuplicatePass {
                pass_index: pair[0],
                name: passes
                    .get(pair[0])
                    .map(|pass| pass.name.clone())
                    .unwrap_or_else(|| "<out of range>".into()),
            });
        }
    }
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

        let color_ops = pass
            .color_attachments
            .iter()
            .map(|(_, ops)| *ops)
            .collect::<Vec<_>>();
        if color_ops != entry.color_attachment_ops {
            divergences.push(TraceDivergence::AttachmentOpsMismatch {
                pass_index: entry.pass_index,
                name: pass.name.clone(),
                aspect: "color",
                compiled: format!("{color_ops:?}"),
                emitted: format!("{:?}", entry.color_attachment_ops),
            });
        }
        if pass.depth_attachment != entry.depth_attachment_ops {
            divergences.push(TraceDivergence::AttachmentOpsMismatch {
                pass_index: entry.pass_index,
                name: pass.name.clone(),
                aspect: "depth/stencil",
                compiled: format!("{:?}", pass.depth_attachment),
                emitted: format!("{:?}", entry.depth_attachment_ops),
            });
        }
        let compiled_depth = pass
            .depth_target
            .and_then(|id| resources.get(id.0 as usize))
            .map(|resource| resource.name.clone());
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
