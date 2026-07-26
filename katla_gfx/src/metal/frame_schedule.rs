//! Temporary compatibility schedule for the current fixed Metal frame encoder.
//!
//! Render-graph topology belongs to the application. This adapter must not require
//! editor-specific passes, resource names, or a particular output chain. It only
//! rejects graph shapes that the current handwritten Metal encoder cannot execute
//! faithfully yet. Once Metal executes compiled pass records directly (#56), this
//! compatibility layer should be deleted.

use crate::render_graph::{FrameGraph, PassDesc, PassKind, PassType, RenderGraphError};

use super::metal_renderer::MetalRenderer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MetalScheduledPass {
    pub(crate) pass_index: usize,
    pub(crate) kind: PassKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MetalFrameSchedule {
    passes: Vec<MetalScheduledPass>,
}

impl MetalFrameSchedule {
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
        let mut passes = Vec::with_capacity(order.len());
        let mut seen = Vec::new();
        let mut previous: Option<(usize, PassKind, String)> = None;

        for &pass_index in order {
            let pass = pass_at(pass_index).ok_or_else(|| {
                RenderGraphError::BackendError(format!(
                    "Metal execution schedule references missing pass index {pass_index}"
                ))
            })?;

            if pass.pass_type == PassType::Compute {
                return Err(RenderGraphError::BackendError(format!(
                    "Metal frame graph pass '{}' is compute; backend-neutral compute commands are not implemented",
                    pass.name
                )));
            }

            let kind = pass.kind.ok_or_else(|| {
                RenderGraphError::BackendError(format!(
                    "Metal frame graph pass '{}' has no executable semantic kind",
                    pass.name
                ))
            })?;
            let rank = Self::legacy_encoder_rank(kind).ok_or_else(|| {
                RenderGraphError::BackendError(format!(
                    "Metal frame graph pass '{}' uses semantic kind {:?}, which the current fixed encoder cannot execute",
                    pass.name, kind
                ))
            })?;

            if seen.contains(&kind) {
                return Err(RenderGraphError::BackendError(format!(
                    "Metal's current fixed encoder supports at most one {:?} pass; repeated semantic kinds require plan-driven execution",
                    kind
                )));
            }

            if let Some((previous_rank, previous_kind, previous_name)) = &previous
                && rank < *previous_rank
            {
                return Err(RenderGraphError::BackendError(format!(
                    "Metal's current fixed encoder cannot honor graph order: pass '{}' ({:?}) appears after '{}' ({:?})",
                    pass.name, kind, previous_name, previous_kind
                )));
            }

            seen.push(kind);
            previous = Some((rank, kind, pass.name.clone()));
            passes.push(MetalScheduledPass { pass_index, kind });
        }

        Ok(Self { passes })
    }

    /// Execution order baked into the handwritten encoder in `frame_render.rs`.
    ///
    /// This is a backend capability description, not an application render-pipeline
    /// contract. Every entry is optional. The ranking disappears with #56.
    fn legacy_encoder_rank(kind: PassKind) -> Option<usize> {
        match kind {
            PassKind::Shadow => Some(0),
            PassKind::DepthPrepass => Some(1),
            PassKind::Geometry => Some(2),
            PassKind::Outline => Some(3),
            PassKind::Fullscreen => Some(4),
            PassKind::Ui => Some(5),
            PassKind::Particles | PassKind::StencilIndicator | PassKind::Compositing => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test(kinds: &[PassKind]) -> Self {
        Self {
            passes: kinds
                .iter()
                .copied()
                .enumerate()
                .map(|(pass_index, kind)| MetalScheduledPass { pass_index, kind })
                .collect(),
        }
    }

    pub(crate) fn passes(&self) -> &[MetalScheduledPass] {
        &self.passes
    }

    pub(crate) fn contains(&self, kind: PassKind) -> bool {
        self.passes.iter().any(|pass| pass.kind == kind)
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
    ) -> Result<MetalFrameSchedule, RenderGraphError> {
        MetalFrameSchedule::compile_order(order, |index| passes.get(index))
    }

    #[test]
    fn preserves_compiled_semantic_order() {
        let passes = vec![
            pass("shadow", PassType::Graphics, Some(PassKind::Shadow)),
            pass(
                "depth_prepass",
                PassType::Graphics,
                Some(PassKind::DepthPrepass),
            ),
            pass("geometry", PassType::Graphics, Some(PassKind::Geometry)),
            pass("outline", PassType::Graphics, Some(PassKind::Outline)),
            pass("tonemap", PassType::Graphics, Some(PassKind::Fullscreen)),
            pass("ui", PassType::Graphics, Some(PassKind::Ui)),
        ];

        let schedule = compile(&passes, &[0, 1, 2, 3, 4, 5]).unwrap();
        assert_eq!(
            schedule
                .passes()
                .iter()
                .map(|pass| pass.kind)
                .collect::<Vec<_>>(),
            vec![
                PassKind::Shadow,
                PassKind::DepthPrepass,
                PassKind::Geometry,
                PassKind::Outline,
                PassKind::Fullscreen,
                PassKind::Ui,
            ]
        );
    }

    #[test]
    fn accepts_empty_schedule() {
        let passes = Vec::new();
        let schedule = compile(&passes, &[]).unwrap();
        assert!(schedule.passes().is_empty());
    }

    #[test]
    fn accepts_optional_builtin_pass_subsets() {
        let ui_only = vec![pass("ui", PassType::Graphics, Some(PassKind::Ui))];
        let schedule = compile(&ui_only, &[0]).unwrap();
        assert!(schedule.contains(PassKind::Ui));
        assert!(!schedule.contains(PassKind::Geometry));
        assert!(!schedule.contains(PassKind::Fullscreen));

        let scene_without_ui = vec![
            pass("geometry", PassType::Graphics, Some(PassKind::Geometry)),
            pass("tonemap", PassType::Graphics, Some(PassKind::Fullscreen)),
        ];
        let schedule = compile(&scene_without_ui, &[0, 1]).unwrap();
        assert!(schedule.contains(PassKind::Geometry));
        assert!(schedule.contains(PassKind::Fullscreen));
        assert!(!schedule.contains(PassKind::Ui));

        let geometry_only = vec![pass(
            "geometry",
            PassType::Graphics,
            Some(PassKind::Geometry),
        )];
        let schedule = compile(&geometry_only, &[0]).unwrap();
        assert!(schedule.contains(PassKind::Geometry));
        assert!(!schedule.contains(PassKind::Fullscreen));
        assert!(!schedule.contains(PassKind::Ui));
    }

    #[test]
    fn rejects_missing_executable_semantic_kind() {
        let passes = vec![pass("custom", PassType::Graphics, None)];
        let error = compile(&passes, &[0]).unwrap_err().to_string();
        assert!(error.contains("has no executable semantic kind"));
    }

    #[test]
    fn rejects_compute_passes_before_encoding() {
        let passes = vec![pass(
            "light_cull",
            PassType::Compute,
            Some(PassKind::Geometry),
        )];
        let error = compile(&passes, &[0]).unwrap_err().to_string();
        assert!(error.contains("backend-neutral compute commands are not implemented"));
    }

    #[test]
    fn rejects_repeated_kinds_that_fixed_encoder_cannot_represent() {
        let passes = vec![
            pass("geometry_a", PassType::Graphics, Some(PassKind::Geometry)),
            pass("geometry_b", PassType::Graphics, Some(PassKind::Geometry)),
        ];
        let error = compile(&passes, &[0, 1]).unwrap_err().to_string();
        assert!(error.contains("current fixed encoder supports at most one Geometry pass"));
        assert!(error.contains("plan-driven execution"));
    }

    #[test]
    fn rejects_kinds_the_fixed_encoder_cannot_execute() {
        let passes = vec![pass(
            "particles",
            PassType::Graphics,
            Some(PassKind::Particles),
        )];
        let error = compile(&passes, &[0]).unwrap_err().to_string();
        assert!(error.contains("current fixed encoder cannot execute"));
    }

    #[test]
    fn rejects_order_the_fixed_encoder_cannot_honor() {
        let passes = vec![
            pass("geometry", PassType::Graphics, Some(PassKind::Geometry)),
            pass("shadow", PassType::Graphics, Some(PassKind::Shadow)),
        ];
        let error = compile(&passes, &[0, 1]).unwrap_err().to_string();
        assert!(error.contains("current fixed encoder cannot honor graph order"));
    }
}
