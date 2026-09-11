//! Metal executable pass records compiled from the backend-neutral render graph.
//!
//! The render graph owns topology and ordering. Metal owns native encoding, but it
//! consumes this ordered record stream directly instead of rebuilding an editor
//! pipeline from singleton semantic checks.

use crate::render_graph::{
    FrameGraph, ImageAccess, ImageSyncOp, PassDesc, PassId, PassKind, PassType, RenderGraphError,
    ResourceId,
};
use crate::render_pass::{ClearValue, LoadOp, StoreOp};
use crate::texture::ImageFormat;

use super::metal_renderer::MetalRenderer;

/// Graph-declared color attachment copied into a Metal executable record.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MetalColorAttachmentRecord {
    pub(crate) resource: ResourceId,
    pub(crate) format: ImageFormat,
    pub(crate) load_op: LoadOp,
    pub(crate) store_op: StoreOp,
    pub(crate) clear_value: ClearValue,
}

/// Graph-declared depth behavior copied into a Metal executable record.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MetalDepthAttachmentOps {
    pub(crate) load_op: LoadOp,
    pub(crate) store_op: StoreOp,
    pub(crate) clear_value: ClearValue,
}

/// Stable executable identity and resource contract for one compiled pass.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MetalPassRecord {
    pub(crate) pass_id: PassId,
    pub(crate) pass_index: usize,
    pub(crate) name: String,
    pub(crate) kind: PassKind,
    pub(crate) reads: Vec<ResourceId>,
    pub(crate) writes: Vec<ResourceId>,
    pub(crate) image_accesses: Vec<ImageAccess>,
    pub(crate) color_attachments: Vec<MetalColorAttachmentRecord>,
    pub(crate) uses_depth: bool,
    pub(crate) depth_attachment: Option<MetalDepthAttachmentOps>,
}

impl MetalPassRecord {
    fn from_pass(
        pass_index: usize,
        pass: &PassDesc,
        format_at: &impl Fn(ResourceId) -> Option<ImageFormat>,
    ) -> Result<Self, RenderGraphError> {
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
            | PassKind::Ui
            | PassKind::Particles => {}
            PassKind::StencilIndicator | PassKind::Compositing => {
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
            reads: pass.reads.clone(),
            writes: pass.writes.clone(),
            image_accesses: pass.image_accesses.clone(),
            color_attachments: pass
                .color_attachments
                .iter()
                .map(|&(resource, ops)| {
                    let format = format_at(resource).ok_or_else(|| {
                        RenderGraphError::BackendError(format!(
                            "Metal pass '{}' targets resource {} with no declared format",
                            pass.name, resource.0
                        ))
                    })?;
                    Ok(MetalColorAttachmentRecord {
                        resource,
                        format,
                        load_op: ops.load,
                        store_op: ops.store,
                        clear_value: ops.clear_value,
                    })
                })
                .collect::<Result<Vec<_>, RenderGraphError>>()?,
            uses_depth: pass.uses_depth,
            depth_attachment: pass.depth_attachment.map(|ops| MetalDepthAttachmentOps {
                load_op: ops.depth.load,
                store_op: ops.depth.store,
                clear_value: ops.depth.clear_value,
            }),
        })
    }

    fn trace(&self) -> String {
        let reads = self
            .reads
            .iter()
            .map(|resource| resource.0.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let writes = self
            .writes
            .iter()
            .map(|resource| resource.0.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let color_attachments = self
            .color_attachments
            .iter()
            .map(|attachment| {
                format!(
                    "{}:{:?}:{:?}/{:?}:{:?}",
                    attachment.resource.0,
                    attachment.format,
                    attachment.load_op,
                    attachment.store_op,
                    attachment.clear_value
                )
            })
            .collect::<Vec<_>>()
            .join("|");
        let depth_attachment = self
            .depth_attachment
            .map(|attachment| {
                format!(
                    "{:?}/{:?}:{:?}",
                    attachment.load_op, attachment.store_op, attachment.clear_value
                )
            })
            .unwrap_or_else(|| "none".to_string());

        format!(
            "{}:{}:{:?}:reads=[{}]:writes=[{}]:colors=[{}]:uses_depth={}:depth={}",
            self.pass_id.0,
            self.name,
            self.kind,
            reads,
            writes,
            color_attachments,
            self.uses_depth,
            depth_attachment
        )
    }
}

/// How Metal realizes one compiled image synchronization operation.
///
/// Metal has no image layouts. Graph transients use private storage with
/// driver-tracked resources: the driver inserts the hazards between encoders
/// and attachment load/store actions realize render-target transitions, so
/// every image sync operation is covered by tracked-resource guarantees —
/// no explicit image barrier is required. (The hand-placed tonemap→UI fence
/// in frame_render predates the compiled plan and remains until queue and
/// encoder boundary requirements are modeled explicitly.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetalSyncCoverage {
    /// Driver-tracked hazards between encoders cover the operation.
    TrackedResource,
}

/// Classification of one compiled image sync operation for Metal encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MetalSyncRecord {
    /// Pass the operation precedes; `None` at frame end.
    pub(crate) pass: Option<usize>,
    pub(crate) resource: ResourceId,
    pub(crate) coverage: MetalSyncCoverage,
}

impl MetalSyncRecord {
    fn classify(pass: Option<usize>, op: &ImageSyncOp) -> Self {
        Self {
            pass,
            resource: op.resource,
            coverage: MetalSyncCoverage::TrackedResource,
        }
    }
}

/// Ordered Metal records derived from the graph compiler's canonical execution order.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MetalExecutionPlan {
    passes: Vec<MetalPassRecord>,
    /// The graph's compiled image sync operations, classified for Metal.
    sync: Vec<MetalSyncRecord>,
}

impl MetalExecutionPlan {
    pub(crate) fn compile(
        frame_graph: &FrameGraph<MetalRenderer>,
    ) -> Result<Self, RenderGraphError> {
        let order = frame_graph.execution_order();
        let format_at = |id: ResourceId| frame_graph.resource_format(id);
        let mut image_sync_ops = Vec::new();
        for &pass_index in &order {
            for op in frame_graph.image_sync_ops(pass_index) {
                image_sync_ops.push((Some(pass_index), *op));
            }
        }
        for op in frame_graph.final_image_sync_ops() {
            image_sync_ops.push((None, *op));
        }
        Self::compile_order(
            &order,
            |index| frame_graph.pass(index),
            &format_at,
            &image_sync_ops,
        )
    }

    fn compile_order<'a>(
        order: &[usize],
        mut pass_at: impl FnMut(usize) -> Option<&'a PassDesc>,
        format_at: &impl Fn(ResourceId) -> Option<ImageFormat>,
        image_sync_ops: &[(Option<usize>, ImageSyncOp)],
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
                MetalPassRecord::from_pass(pass_index, pass, format_at)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let sync = image_sync_ops
            .iter()
            .map(|(pass, op)| MetalSyncRecord::classify(*pass, op))
            .collect();

        Ok(Self { passes, sync })
    }

    pub(crate) fn passes(&self) -> &[MetalPassRecord] {
        &self.passes
    }

    pub(crate) fn sync_records(&self) -> &[MetalSyncRecord] {
        &self.sync
    }

    #[cfg(test)]
    pub(crate) fn for_test(kinds: &[PassKind]) -> Self {
        Self {
            sync: Vec::new(),
            passes: kinds
                .iter()
                .copied()
                .enumerate()
                .map(|(pass_index, kind)| MetalPassRecord {
                    pass_id: PassId(pass_index as u32),
                    pass_index,
                    name: format!("pass_{pass_index}"),
                    kind,
                    reads: Vec::new(),
                    writes: Vec::new(),
                    image_accesses: Vec::new(),
                    color_attachments: Vec::new(),
                    uses_depth: matches!(
                        kind,
                        PassKind::DepthPrepass
                            | PassKind::Geometry
                            | PassKind::ObjectId
                            | PassKind::Outline
                    ),
                    depth_attachment: None,
                })
                .collect(),
        }
    }

    /// Deterministic plan trace used by validation and regression tests.
    pub(crate) fn trace(&self) -> Vec<String> {
        self.passes.iter().map(MetalPassRecord::trace).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_graph::{
        ImageAccessMode, ImagePipelineStage, ImageSubresourceRange, ImageUsage,
    };

    fn pass(name: &str, pass_type: PassType, kind: Option<PassKind>) -> PassDesc {
        let mut pass = PassDesc::new(name, pass_type, Vec::new(), Vec::new());
        pass.kind = kind;
        pass.uses_depth = matches!(
            kind,
            Some(
                PassKind::DepthPrepass
                    | PassKind::Geometry
                    | PassKind::ObjectId
                    | PassKind::Outline
            )
        );
        pass
    }

    fn compile(
        passes: &[PassDesc],
        order: &[usize],
    ) -> Result<MetalExecutionPlan, RenderGraphError> {
        let format_at = |_: crate::render_graph::ResourceId| Some(ImageFormat::R16G16B16A16Sfloat);
        MetalExecutionPlan::compile_order(order, |index| passes.get(index), &format_at, &[])
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
            vec![
                "0:geometry:Geometry:reads=[]:writes=[]:colors=[]:uses_depth=true:depth=none",
                "1:tonemap:Fullscreen:reads=[]:writes=[]:colors=[]:uses_depth=false:depth=none",
                "2:ui:Ui:reads=[]:writes=[]:colors=[]:uses_depth=false:depth=none",
            ]
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
    fn copies_graph_resource_and_attachment_contracts() {
        let mut geometry = pass("geometry", PassType::Graphics, Some(PassKind::Geometry));
        geometry.set_image_accesses(vec![
            ImageAccess::sampled_read(ResourceId(4)),
            ImageAccess::new(
                ResourceId(7),
                ImageAccessMode::ReadWrite,
                ImageUsage::ColorAttachment,
                ImagePipelineStage::ColorAttachmentOutput,
                ImageSubresourceRange::WHOLE_COLOR,
            ),
        ]);
        geometry.color_attachments.push((
            ResourceId(7),
            crate::render_pass::AttachmentOps {
                load: LoadOp::Load,
                store: StoreOp::Store,
                clear_value: ClearValue::OPAQUE_BLACK,
            },
        ));
        geometry.depth_attachment = Some(crate::render_pass::DepthStencilAttachmentOps {
            depth: crate::render_pass::AttachmentOps {
                load: LoadOp::Load,
                store: StoreOp::Store,
                clear_value: ClearValue::DepthStencil {
                    depth: 0.0,
                    stencil: 1,
                },
            },
            stencil: crate::render_pass::AttachmentOps {
                load: LoadOp::Load,
                store: StoreOp::Store,
                clear_value: ClearValue::DepthStencil {
                    depth: 0.0,
                    stencil: 1,
                },
            },
        });

        let plan = compile(&[geometry], &[0]).unwrap();
        let record = &plan.passes()[0];
        assert_eq!(record.reads, vec![ResourceId(4), ResourceId(7)]);
        assert_eq!(record.writes, vec![ResourceId(7)]);
        assert_eq!(
            record.image_accesses,
            vec![
                ImageAccess::sampled_read(ResourceId(4)),
                ImageAccess::new(
                    ResourceId(7),
                    ImageAccessMode::ReadWrite,
                    ImageUsage::ColorAttachment,
                    ImagePipelineStage::ColorAttachmentOutput,
                    ImageSubresourceRange::WHOLE_COLOR,
                ),
            ]
        );
        assert_eq!(
            record.color_attachments,
            vec![MetalColorAttachmentRecord {
                resource: ResourceId(7),
                format: ImageFormat::R16G16B16A16Sfloat,
                load_op: LoadOp::Load,
                store_op: StoreOp::Store,
                clear_value: ClearValue::OPAQUE_BLACK,
            }]
        );
        assert_eq!(
            record.depth_attachment,
            Some(MetalDepthAttachmentOps {
                load_op: LoadOp::Load,
                store_op: StoreOp::Store,
                clear_value: ClearValue::DepthStencil {
                    depth: 0.0,
                    stencil: 1,
                },
            })
        );
    }

    #[test]
    fn classifies_compiled_sync_ops_as_tracked_resource_coverage() {
        let geometry = pass("geometry", PassType::Graphics, Some(PassKind::Geometry));
        let tonemap = pass("tonemap", PassType::Graphics, Some(PassKind::Fullscreen));
        let passes = vec![geometry, tonemap];

        // Attachment→sampled RAW before tonemap, plus a frame-end contract op.
        let attachment_write = ImageSyncOp {
            resource: ResourceId(3),
            range: ImageSubresourceRange::WHOLE_COLOR,
            before: crate::render_graph::ImageSyncState::Access {
                usage: ImageUsage::ColorAttachment,
                stage: ImagePipelineStage::ColorAttachmentOutput,
                mode: ImageAccessMode::Write,
            },
            after: crate::render_graph::ImageSyncState::Access {
                usage: ImageUsage::Sampled,
                stage: ImagePipelineStage::FragmentShader,
                mode: ImageAccessMode::Read,
            },
            before_pass: Some(0),
            pass: 1,
            reason: crate::render_graph::SyncReason::Hazard(
                crate::render_graph::ResourceHazardKind::ReadAfterWrite,
            ),
        };
        let frame_end = ImageSyncOp {
            resource: ResourceId(3),
            range: ImageSubresourceRange::WHOLE_COLOR,
            before: attachment_write.after,
            after: attachment_write.before,
            before_pass: Some(1),
            pass: usize::MAX,
            reason: crate::render_graph::SyncReason::ImportedFinal,
        };

        let format_at = |_: ResourceId| Some(ImageFormat::R16G16B16A16Sfloat);
        let plan = MetalExecutionPlan::compile_order(
            &[0, 1],
            |index| passes.get(index),
            &format_at,
            &[(Some(1), attachment_write), (None, frame_end)],
        )
        .unwrap();

        assert_eq!(
            plan.sync_records(),
            &[
                MetalSyncRecord {
                    pass: Some(1),
                    resource: ResourceId(3),
                    coverage: MetalSyncCoverage::TrackedResource,
                },
                MetalSyncRecord {
                    pass: None,
                    resource: ResourceId(3),
                    coverage: MetalSyncCoverage::TrackedResource,
                },
            ]
        );
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
    fn accepts_particles_handler_during_plan_compilation() {
        let passes = vec![pass(
            "particles",
            PassType::Graphics,
            Some(PassKind::Particles),
        )];
        assert!(compile(&passes, &[0]).is_ok());
    }
}
