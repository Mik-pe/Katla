//! Metal executable pass records compiled from the backend-neutral render graph.
//!
//! The render graph owns topology and ordering. Metal owns native encoding, but it
//! consumes this ordered record stream directly instead of rebuilding an editor
//! pipeline from singleton semantic checks.

use crate::render_graph::{FrameGraph, PassDesc, PassId, PassKind, PassType, RenderGraphError};

use super::metal_renderer::MetalRenderer;

/// Stable executable identity for one compiled render-graph pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MetalPassRecord {
    pub(crate) pass_id: PassId,
    pub(crate) pass_index: usize,
    pub(crate) name: String,
    pub(crate) kind: PassKind,
}

impl MetalPassRecord {
    fn from_pass(pass_index: usize, pass: &PassDesc) -> Result<Self, RenderGraphError> {
        if pass.pass_type == PassType::Compute {
            return Err(RenderGraphError::BackendError(format!(
                "Metal pass '{}' is compute; backend-neutral compute commands are not implemented",
                pass.name
            )));
        }

        let kind = pass.kind.ok_or_else(|| {
            RenderGraphError::BackendError(format!(
                "Metal pass '{}' has no executable semantic kind",
                pass.name
            ))
        })?;

        match kind {
            PassKind::Shadow
            | PassKind::DepthPrepass
            | PassKind::Geometry
            | PassKind::ObjectId
            | PassKind::Outline
            | PassKind::Fullscreen
            | PassKind::Ui => {}
            PassKind::Particles | PassKind::StencilIndicator | PassKind::Compositing => {
                return Err(RenderGraphError::BackendError(format!(
                    "Metal has no executable handler for pass '{}' ({kind:?})",
                    pass.name
                )));
            }
        }

        Ok(Self {
            pass_id: PassId(pass_index as u32),
            pass_index,
            name: pass.name.clone(),
            kind,
        })
    }
}

/// Ordered Metal records derived from the graph compiler's canonical execution order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MetalExecutionPlan {
    passes: Vec<MetalPassRecord>,
}

impl MetalExecutionPlan {
    pub(crate) fn compile(
        frame_graph: &FrameGraph<MetalRenderer>,
    ) -> Result<Self, RenderGraphError> {
        let order = frame_graph.execution_order();
        Self::compile_order(&order, |index| frame_graph.pass(index))
    }

    fn compile_order<'a>(
        order: &[usize],
        mut pass_at: impl FnMut(usize) -> Option<&'a PassDesc>,
    ) -> Result<Self, RenderGraphError> {
        let passes = order
            .iter()
            .copied()
            .map(|pass_index| {
                let pass = pass_at(pass_index).ok_or_else(|| {
                    RenderGraphError::BackendError(format!(
                        "Metal execution plan references missing pass index {pass_index}"
                    ))
                })?;
                MetalPassRecord::from_pass(pass_index, pass)
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self { passes })
    }

    pub(crate) fn passes(&self) -> &[MetalPassRecord] {
        &self.passes
    }

    #[cfg(test)]
    pub(crate) fn for_test(kinds: &[PassKind]) -> Self {
        Self {
            passes: kinds
                .iter()
                .copied()
                .enumerate()
                .map(|(pass_index, kind)| MetalPassRecord {
                    pass_id: PassId(pass_index as u32),
                    pass_index,
                    name: format!("pass_{pass_index}"),
                    kind,
                })
                .collect(),
        }
    }

    /// Deterministic plan trace used by validation and regression tests.
    pub(crate) fn trace(&self) -> Vec<String> {
        self.passes
            .iter()
            .map(|pass| format!("{}:{}:{:?}", pass.pass_id.0, pass.name, pass.kind))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pass(name: &str, pass_type: PassType, kind: Option<PassKind>) -> PassDesc {
        let mut pass = PassDesc::new(name, pass_type, Vec::new(), Vec::new());
        pass.kind = kind;
        pass
    }

    fn compile(
        passes: &[PassDesc],
        order: &[usize],
    ) -> Result<MetalExecutionPlan, RenderGraphError> {
        MetalExecutionPlan::compile_order(order, |index| passes.get(index))
    }

    #[test]
    fn preserves_compiled_order_and_stable_pass_identity() {
        let passes = vec![
            pass("geometry", PassType::Graphics, Some(PassKind::Geometry)),
            pass("tonemap", PassType::Graphics, Some(PassKind::Fullscreen)),
            pass("ui", PassType::Graphics, Some(PassKind::Ui)),
        ];

        let plan = compile(&passes, &[0, 1, 2]).unwrap();
        assert_eq!(
            plan.trace(),
            vec!["0:geometry:Geometry", "1:tonemap:Fullscreen", "2:ui:Ui",]
        );
    }

    #[test]
    fn accepts_empty_graph() {
        assert!(compile(&[], &[]).unwrap().passes().is_empty());
    }

    #[test]
    fn accepts_repeated_semantic_categories() {
        let passes = vec![
            pass("gbuffer", PassType::Graphics, Some(PassKind::Geometry)),
            pass("decals", PassType::Graphics, Some(PassKind::Geometry)),
            pass("bloom", PassType::Graphics, Some(PassKind::Fullscreen)),
            pass("tonemap", PassType::Graphics, Some(PassKind::Fullscreen)),
        ];

        let plan = compile(&passes, &[0, 1, 2, 3]).unwrap();
        assert_eq!(plan.passes().len(), 4);
        assert_eq!(plan.passes()[0].pass_id, PassId(0));
        assert_eq!(plan.passes()[1].pass_id, PassId(1));
        assert_eq!(plan.passes()[2].kind, PassKind::Fullscreen);
        assert_eq!(plan.passes()[3].kind, PassKind::Fullscreen);
    }

    #[test]
    fn honors_non_editor_order_without_a_fixed_rank_table() {
        let passes = vec![
            pass("ui", PassType::Graphics, Some(PassKind::Ui)),
            pass("geometry", PassType::Graphics, Some(PassKind::Geometry)),
            pass("shadow", PassType::Graphics, Some(PassKind::Shadow)),
        ];

        let plan = compile(&passes, &[0, 1, 2]).unwrap();
        assert_eq!(
            plan.passes()
                .iter()
                .map(|pass| pass.kind)
                .collect::<Vec<_>>(),
            vec![PassKind::Ui, PassKind::Geometry, PassKind::Shadow]
        );
    }

    #[test]
    fn object_id_is_an_explicit_executable_record() {
        let passes = vec![pass(
            "object_id",
            PassType::Graphics,
            Some(PassKind::ObjectId),
        )];
        let plan = compile(&passes, &[0]).unwrap();
        assert_eq!(plan.passes()[0].kind, PassKind::ObjectId);
    }

    #[test]
    fn rejects_compute_before_command_buffer_creation() {
        let passes = vec![pass(
            "light_cull",
            PassType::Compute,
            Some(PassKind::Geometry),
        )];
        let error = compile(&passes, &[0]).unwrap_err().to_string();
        assert!(error.contains("backend-neutral compute commands"));
    }

    #[test]
    fn rejects_missing_executable_handler() {
        let passes = vec![pass("custom", PassType::Graphics, None)];
        let error = compile(&passes, &[0]).unwrap_err().to_string();
        assert!(error.contains("no executable semantic kind"));
    }

    #[test]
    fn rejects_unimplemented_handler_during_plan_compilation() {
        let passes = vec![pass(
            "particles",
            PassType::Graphics,
            Some(PassKind::Particles),
        )];
        let error = compile(&passes, &[0]).unwrap_err().to_string();
        assert!(error.contains("no executable handler"));
    }
}
