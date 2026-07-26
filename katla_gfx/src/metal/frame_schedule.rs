//! Validated semantic schedule for Metal frame encoding.

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
                    "Metal frame graph pass '{}' has no semantic PassKind",
                    pass.name
                ))
            })?;
            let rank = Self::rank(kind).ok_or_else(|| {
                RenderGraphError::BackendError(format!(
                    "Metal frame graph pass '{}' uses unsupported semantic kind {:?}",
                    pass.name, kind
                ))
            })?;

            if seen.contains(&kind) {
                return Err(RenderGraphError::BackendError(format!(
                    "Metal frame graph contains duplicate singleton pass kind {:?}",
                    kind
                )));
            }

            if let Some((previous_rank, previous_kind, previous_name)) = &previous
                && rank < *previous_rank
            {
                return Err(RenderGraphError::BackendError(format!(
                    "Metal frame graph order is invalid: pass '{}' ({:?}) appears after '{}' ({:?})",
                    pass.name, kind, previous_name, previous_kind
                )));
            }

            seen.push(kind);
            previous = Some((rank, kind, pass.name.clone()));
            passes.push(MetalScheduledPass { pass_index, kind });
        }

        if !seen.contains(&PassKind::Geometry) {
            return Err(RenderGraphError::BackendError(
                "Metal frame graph requires exactly one Geometry pass".to_string(),
            ));
        }

        Ok(Self { passes })
    }

    fn rank(kind: PassKind) -> Option<usize> {
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
    fn rejects_missing_semantic_kind() {
        let passes = vec![pass("geometry", PassType::Graphics, None)];
        let error = compile(&passes, &[0]).unwrap_err().to_string();
        assert!(error.contains("has no semantic PassKind"));
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
    fn rejects_duplicate_singleton_kinds() {
        let passes = vec![
            pass("geometry_a", PassType::Graphics, Some(PassKind::Geometry)),
            pass("geometry_b", PassType::Graphics, Some(PassKind::Geometry)),
        ];
        let error = compile(&passes, &[0, 1]).unwrap_err().to_string();
        assert!(error.contains("duplicate singleton pass kind Geometry"));
    }

    #[test]
    fn rejects_unsupported_kinds() {
        let passes = vec![
            pass("geometry", PassType::Graphics, Some(PassKind::Geometry)),
            pass("particles", PassType::Graphics, Some(PassKind::Particles)),
        ];
        let error = compile(&passes, &[0, 1]).unwrap_err().to_string();
        assert!(error.contains("unsupported semantic kind Particles"));
    }

    #[test]
    fn rejects_backend_order_drift() {
        let passes = vec![
            pass("geometry", PassType::Graphics, Some(PassKind::Geometry)),
            pass("shadow", PassType::Graphics, Some(PassKind::Shadow)),
        ];
        let error = compile(&passes, &[0, 1]).unwrap_err().to_string();
        assert!(error.contains("order is invalid"));
    }

    #[test]
    fn accepts_optional_passes_around_required_geometry() {
        let passes = vec![
            pass("geometry", PassType::Graphics, Some(PassKind::Geometry)),
            pass("ui", PassType::Graphics, Some(PassKind::Ui)),
        ];
        let schedule = compile(&passes, &[0, 1]).unwrap();
        assert!(schedule.contains(PassKind::Geometry));
        assert!(schedule.contains(PassKind::Ui));
        assert!(!schedule.contains(PassKind::DepthPrepass));
    }
}
