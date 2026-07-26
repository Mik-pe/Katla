//! Validated semantic schedule for Metal frame encoding.

use crate::render_graph::{
    BACKBUFFER_NAME, FrameGraph, PassDesc, PassKind, PassType, RenderGraphError, ResourceId,
};

use super::metal_renderer::MetalRenderer;

const HDR_COLOR_RESOURCE: &str = "hdr_color";
const VIEWPORT_RESOURCE: &str = "viewport_0";
const REQUIRED_PASS_KINDS: [PassKind; 3] = [
    PassKind::Geometry,
    PassKind::Fullscreen,
    PassKind::Ui,
];

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
        let schedule = Self::compile_order(&order, |index| frame_graph.pass(index))?;
        schedule.validate_resource_contract(
            |index| frame_graph.pass(index),
            |resource| frame_graph.resource_name(resource),
        )?;
        Ok(schedule)
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

        for required_kind in REQUIRED_PASS_KINDS {
            if !seen.contains(&required_kind) {
                return Err(RenderGraphError::BackendError(format!(
                    "Metal frame graph requires exactly one {:?} pass",
                    required_kind
                )));
            }
        }

        Ok(Self { passes })
    }

    fn validate_resource_contract<'a>(
        &self,
        mut pass_at: impl FnMut(usize) -> Option<&'a PassDesc>,
        mut resource_name: impl FnMut(ResourceId) -> Option<&'a str>,
    ) -> Result<(), RenderGraphError> {
        for scheduled_pass in &self.passes {
            let pass = pass_at(scheduled_pass.pass_index).ok_or_else(|| {
                RenderGraphError::BackendError(format!(
                    "Metal resource contract references missing pass index {}",
                    scheduled_pass.pass_index
                ))
            })?;

            match scheduled_pass.kind {
                PassKind::Shadow | PassKind::DepthPrepass => {}
                PassKind::Geometry => Self::require_named_access(
                    pass,
                    scheduled_pass.kind,
                    &pass.writes,
                    "write",
                    HDR_COLOR_RESOURCE,
                    &mut resource_name,
                )?,
                PassKind::Outline => {
                    Self::require_named_access(
                        pass,
                        scheduled_pass.kind,
                        &pass.reads,
                        "read",
                        HDR_COLOR_RESOURCE,
                        &mut resource_name,
                    )?;
                    Self::require_named_access(
                        pass,
                        scheduled_pass.kind,
                        &pass.writes,
                        "write",
                        HDR_COLOR_RESOURCE,
                        &mut resource_name,
                    )?;
                }
                PassKind::Fullscreen => {
                    Self::require_named_access(
                        pass,
                        scheduled_pass.kind,
                        &pass.reads,
                        "read",
                        HDR_COLOR_RESOURCE,
                        &mut resource_name,
                    )?;
                    Self::require_named_access(
                        pass,
                        scheduled_pass.kind,
                        &pass.writes,
                        "write",
                        VIEWPORT_RESOURCE,
                        &mut resource_name,
                    )?;
                }
                PassKind::Ui => {
                    Self::require_named_access(
                        pass,
                        scheduled_pass.kind,
                        &pass.reads,
                        "read",
                        VIEWPORT_RESOURCE,
                        &mut resource_name,
                    )?;
                    Self::require_named_access(
                        pass,
                        scheduled_pass.kind,
                        &pass.writes,
                        "write",
                        BACKBUFFER_NAME,
                        &mut resource_name,
                    )?;
                }
                unsupported => {
                    return Err(RenderGraphError::BackendError(format!(
                        "Metal resource contract received unsupported semantic kind {:?}",
                        unsupported
                    )));
                }
            }
        }

        Ok(())
    }

    fn require_named_access<'a>(
        pass: &PassDesc,
        kind: PassKind,
        resources: &[ResourceId],
        access: &str,
        required_resource: &str,
        resource_name: &mut impl FnMut(ResourceId) -> Option<&'a str>,
    ) -> Result<(), RenderGraphError> {
        if resources
            .iter()
            .copied()
            .any(|resource| resource_name(resource) == Some(required_resource))
        {
            return Ok(());
        }

        Err(RenderGraphError::BackendError(format!(
            "Metal frame graph pass '{}' ({:?}) must declare a {} access to resource '{}'",
            pass.name, kind, access, required_resource
        )))
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

    fn rid(value: u32) -> ResourceId {
        ResourceId(value)
    }

    fn pass(name: &str, pass_type: PassType, kind: Option<PassKind>) -> PassDesc {
        let mut pass = PassDesc::new(name, pass_type, Vec::new(), Vec::new());
        pass.kind = kind;
        pass
    }

    fn pass_with_accesses(
        name: &str,
        kind: PassKind,
        reads: Vec<ResourceId>,
        writes: Vec<ResourceId>,
    ) -> PassDesc {
        let mut pass = PassDesc::new(name, PassType::Graphics, reads, writes);
        pass.kind = Some(kind);
        pass
    }

    fn compile(
        passes: &[PassDesc],
        order: &[usize],
    ) -> Result<MetalFrameSchedule, RenderGraphError> {
        MetalFrameSchedule::compile_order(order, |index| passes.get(index))
    }

    fn contract_passes() -> Vec<PassDesc> {
        vec![
            pass_with_accesses("geometry", PassKind::Geometry, Vec::new(), vec![rid(0)]),
            pass_with_accesses("outline", PassKind::Outline, vec![rid(0)], vec![rid(0)]),
            pass_with_accesses(
                "tonemap",
                PassKind::Fullscreen,
                vec![rid(0)],
                vec![rid(1)],
            ),
            pass_with_accesses("ui", PassKind::Ui, vec![rid(1)], vec![rid(2)]),
        ]
    }

    fn compile_contract(passes: &[PassDesc]) -> Result<MetalFrameSchedule, RenderGraphError> {
        let resources = [HDR_COLOR_RESOURCE, VIEWPORT_RESOURCE, BACKBUFFER_NAME, "other"];
        let order: Vec<usize> = (0..passes.len()).collect();
        let schedule = compile(passes, &order)?;
        schedule.validate_resource_contract(
            |index| passes.get(index),
            |resource| resources.get(resource.0 as usize).copied(),
        )?;
        Ok(schedule)
    }

    fn assert_contract_error(passes: &[PassDesc], expected: &str) {
        let error = compile_contract(passes).unwrap_err().to_string();
        assert!(
            error.contains(expected),
            "expected error to contain '{expected}', got '{error}'"
        );
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
    fn rejects_missing_required_pipeline_passes() {
        let passes = vec![
            pass("tonemap", PassType::Graphics, Some(PassKind::Fullscreen)),
            pass("ui", PassType::Graphics, Some(PassKind::Ui)),
        ];
        let error = compile(&passes, &[0, 1]).unwrap_err().to_string();
        assert!(error.contains("exactly one Geometry pass"));

        let passes = vec![
            pass("geometry", PassType::Graphics, Some(PassKind::Geometry)),
            pass("ui", PassType::Graphics, Some(PassKind::Ui)),
        ];
        let error = compile(&passes, &[0, 1]).unwrap_err().to_string();
        assert!(error.contains("exactly one Fullscreen pass"));

        let passes = vec![
            pass("geometry", PassType::Graphics, Some(PassKind::Geometry)),
            pass("tonemap", PassType::Graphics, Some(PassKind::Fullscreen)),
        ];
        let error = compile(&passes, &[0, 1]).unwrap_err().to_string();
        assert!(error.contains("exactly one Ui pass"));
    }

    #[test]
    fn accepts_optional_passes_around_required_pipeline() {
        let passes = vec![
            pass("shadow", PassType::Graphics, Some(PassKind::Shadow)),
            pass("geometry", PassType::Graphics, Some(PassKind::Geometry)),
            pass("tonemap", PassType::Graphics, Some(PassKind::Fullscreen)),
            pass("ui", PassType::Graphics, Some(PassKind::Ui)),
        ];
        let schedule = compile(&passes, &[0, 1, 2, 3]).unwrap();
        assert!(schedule.contains(PassKind::Shadow));
        assert!(schedule.contains(PassKind::Geometry));
        assert!(schedule.contains(PassKind::Fullscreen));
        assert!(schedule.contains(PassKind::Ui));
        assert!(!schedule.contains(PassKind::DepthPrepass));
    }

    #[test]
    fn accepts_the_canonical_resource_contract() {
        let schedule = compile_contract(&contract_passes()).unwrap();
        assert!(schedule.contains(PassKind::Geometry));
        assert!(schedule.contains(PassKind::Fullscreen));
        assert!(schedule.contains(PassKind::Ui));
    }

    #[test]
    fn rejects_resource_contract_drift() {
        let mut passes = contract_passes();
        passes[0].writes = vec![rid(3)];
        assert_contract_error(&passes, "Geometry) must declare a write access to resource 'hdr_color'");

        let mut passes = contract_passes();
        passes[1].reads = vec![rid(3)];
        assert_contract_error(&passes, "Outline) must declare a read access to resource 'hdr_color'");

        let mut passes = contract_passes();
        passes[1].writes = vec![rid(3)];
        assert_contract_error(&passes, "Outline) must declare a write access to resource 'hdr_color'");

        let mut passes = contract_passes();
        passes[2].reads = vec![rid(3)];
        assert_contract_error(&passes, "Fullscreen) must declare a read access to resource 'hdr_color'");

        let mut passes = contract_passes();
        passes[2].writes = vec![rid(3)];
        assert_contract_error(&passes, "Fullscreen) must declare a write access to resource 'viewport_0'");

        let mut passes = contract_passes();
        passes[3].reads = vec![rid(3)];
        assert_contract_error(&passes, "Ui) must declare a read access to resource 'viewport_0'");

        let mut passes = contract_passes();
        passes[3].writes = vec![rid(3)];
        assert_contract_error(&passes, "Ui) must declare a write access to resource 'backbuffer'");
    }
}
