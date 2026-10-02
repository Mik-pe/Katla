//! Metal executable pass records compiled from the backend-neutral render graph.
//!
//! The render graph owns topology and ordering. Metal owns native encoding, but it
//! consumes this ordered record stream directly instead of rebuilding an editor
//! pipeline from singleton semantic checks.

use crate::render_graph::{
    BufferAccess, BufferByteRange, BufferSyncOp, FrameGraph, ImageAccess, ImageSyncOp, PassDesc,
    PassKind, PassType, RenderGraphError, ResourceId,
};
use crate::render_pass::{ClearValue, LoadOp, StoreOp};
use crate::texture::ImageFormat;

use super::metal_renderer::MetalRenderer;

/// Graph-declared color attachment copied into a Metal executable record.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MetalColorAttachmentRecord {
    pub(crate) resource: ResourceId,
    /// Resource name, so the emitted trace names the same targets the
    /// compiled plan declares instead of opaque ids.
    pub(crate) name: String,
    pub(crate) format: ImageFormat,
    pub(crate) load_op: LoadOp,
    pub(crate) store_op: StoreOp,
    pub(crate) clear_value: ClearValue,
}

/// Graph-declared depth behavior copied into a Metal executable record.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MetalDepthAttachmentOps {
    pub(crate) resource: ResourceId,
    pub(crate) format: ImageFormat,
    pub(crate) stencil_ops: crate::render_pass::AttachmentOps,
    pub(crate) load_op: LoadOp,
    pub(crate) store_op: StoreOp,
    pub(crate) clear_value: ClearValue,
}

/// Stable executable identity and resource contract for one compiled pass.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MetalPassRecord {
    pub(crate) pass_index: usize,
    pub(crate) name: String,
    pub(crate) kind: PassKind,
    pub(crate) pass_type: PassType,
    pub(crate) commands: Vec<crate::render_graph::ComputeCommand>,
    pub(crate) reads: Vec<ResourceId>,
    pub(crate) writes: Vec<ResourceId>,
    pub(crate) image_accesses: Vec<ImageAccess>,
    pub(crate) buffer_accesses: Vec<BufferAccess>,
    pub(crate) color_attachments: Vec<MetalColorAttachmentRecord>,
    pub(crate) uses_depth: bool,
    pub(crate) depth_attachment: Option<MetalDepthAttachmentOps>,
    pub(crate) material: Option<crate::handle::MaterialHandle>,
    pub(crate) bindings: crate::renderer::frame_bindings::PassBindings,
}

impl MetalPassRecord {
    fn from_pass(
        pass_index: usize,
        pass: &PassDesc,
        format_at: &impl Fn(ResourceId) -> Option<ImageFormat>,
        name_at: &impl Fn(ResourceId) -> Option<String>,
    ) -> Result<Self, RenderGraphError> {
        let kind = pass.kind.unwrap_or(PassKind::Geometry);

        Ok(Self {
            pass_index,
            name: pass.name.clone(),
            kind,
            pass_type: pass.pass_type,
            commands: pass.commands.clone(),
            reads: pass.reads.clone(),
            writes: pass.writes.clone(),
            image_accesses: pass.image_accesses.clone(),
            buffer_accesses: pass.buffer_accesses.clone(),
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
                        name: name_at(resource).unwrap_or_else(|| resource.0.to_string()),
                        format,
                        load_op: ops.load,
                        store_op: ops.store,
                        clear_value: ops.clear_value,
                    })
                })
                .collect::<Result<Vec<_>, RenderGraphError>>()?,
            uses_depth: pass.uses_depth,
            depth_attachment: if pass.uses_depth {
                let resource = pass.depth_target.ok_or_else(|| {
                    RenderGraphError::BackendError(format!(
                        "Metal pass '{}' uses depth without a declared graph depth target",
                        pass.name
                    ))
                })?;
                let format = format_at(resource).ok_or_else(|| {
                    RenderGraphError::BackendError(format!(
                        "Metal pass '{}' has unresolved depth resource {}",
                        pass.name, resource.0
                    ))
                })?;
                if !matches!(
                    format,
                    ImageFormat::D32Sfloat
                        | ImageFormat::D32SfloatS8Uint
                        | ImageFormat::D24UnormS8Uint
                ) {
                    return Err(RenderGraphError::BackendError(format!(
                        "Metal pass '{}' has non-depth target {}",
                        pass.name, resource.0
                    )));
                }
                let ops = pass.depth_attachment.ok_or_else(|| {
                    RenderGraphError::BackendError(format!(
                        "Metal pass '{}' has no depth attachment operations",
                        pass.name
                    ))
                })?;
                Some(MetalDepthAttachmentOps {
                    resource,
                    format,
                    stencil_ops: ops.stencil,
                    load_op: ops.depth.load,
                    store_op: ops.depth.store,
                    clear_value: ops.depth.clear_value,
                })
            } else {
                None
            },
            material: pass.material,
            bindings: pass.bindings.clone(),
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
            self.pass_index,
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

/// Native stage barriers implement compiled image and buffer hazards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetalSyncCoverage {
    /// Compiled native queue-stage visibility orders the operation.
    ExplicitStageBarrier,
    /// The first use has no prior GPU access to order.
    NoPriorAccess,
}

/// Classification of one compiled synchronization operation for Metal encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MetalSyncRecord {
    /// Pass the operation precedes; `None` at frame end.
    pub(crate) pass: Option<usize>,
    pub(crate) resource: ResourceId,
    /// Declared byte scope for buffer operations; image scopes remain in the compiled image synchronization plan.
    pub(crate) buffer_range: Option<BufferByteRange>,
    pub(crate) coverage: MetalSyncCoverage,
}

impl MetalSyncRecord {
    fn classify(pass: Option<usize>, op: &ImageSyncOp) -> Self {
        Self {
            pass,
            resource: op.resource,
            buffer_range: None,
            coverage: MetalSyncCoverage::ExplicitStageBarrier,
        }
    }

    fn classify_buffer(pass: usize, op: &BufferSyncOp) -> Self {
        Self {
            pass: Some(pass),
            resource: op.resource,
            buffer_range: Some(op.range),
            coverage: if op.before == crate::render_graph::BufferSyncState::Undefined {
                MetalSyncCoverage::NoPriorAccess
            } else {
                MetalSyncCoverage::ExplicitStageBarrier
            },
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
        backbuffer_format: ImageFormat,
        renderer: Option<&MetalRenderer>,
    ) -> Result<Self, RenderGraphError> {
        let order = frame_graph.execution_order();
        // Transient resources carry their declared format; the imported
        // backbuffer is backend-owned and resolves to the drawable's format.
        let format_at = |id: ResourceId| {
            frame_graph.resource_format(id).or_else(|| {
                if frame_graph.resource_name(id) == Some("backbuffer") {
                    Some(backbuffer_format)
                } else {
                    frame_graph
                        .imported_images
                        .get(&id)
                        .and_then(|handle| {
                            renderer.and_then(|renderer| renderer.textures.get(*handle))
                        })
                        .map(|entry| {
                            use crate::backend::resource::GpuImage;
                            entry.texture.format()
                        })
                }
            })
        };
        let mut image_sync_ops = Vec::new();
        let mut buffer_sync_ops = Vec::new();
        for &pass_index in &order {
            for op in frame_graph.image_sync_ops(pass_index) {
                image_sync_ops.push((Some(pass_index), *op));
            }
            for op in frame_graph.buffer_sync_ops(pass_index) {
                buffer_sync_ops.push((pass_index, *op));
            }
        }
        for op in frame_graph.final_image_sync_ops() {
            image_sync_ops.push((None, *op));
        }
        Self::compile_order(
            &order,
            |index| frame_graph.pass(index),
            &format_at,
            &|id| frame_graph.resource_name(id).map(str::to_string),
            &image_sync_ops,
            &buffer_sync_ops,
        )
    }

    fn compile_order<'a>(
        order: &[usize],
        mut pass_at: impl FnMut(usize) -> Option<&'a PassDesc>,
        format_at: &impl Fn(ResourceId) -> Option<ImageFormat>,
        name_at: &impl Fn(ResourceId) -> Option<String>,
        image_sync_ops: &[(Option<usize>, ImageSyncOp)],
        buffer_sync_ops: &[(usize, BufferSyncOp)],
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
                MetalPassRecord::from_pass(pass_index, pass, format_at, name_at)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut sync = image_sync_ops
            .iter()
            .map(|(pass, op)| MetalSyncRecord::classify(*pass, op))
            .collect::<Vec<_>>();
        sync.extend(
            buffer_sync_ops
                .iter()
                .map(|(pass, op)| MetalSyncRecord::classify_buffer(*pass, op)),
        );
        sync.sort_by_key(|record| (record.pass.unwrap_or(usize::MAX), record.resource.0));

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
                    pass_index,
                    name: format!("pass_{pass_index}"),
                    kind,
                    pass_type: PassType::Graphics,
                    commands: Vec::new(),
                    reads: Vec::new(),
                    writes: Vec::new(),
                    image_accesses: Vec::new(),
                    buffer_accesses: Vec::new(),
                    color_attachments: Vec::new(),
                    uses_depth: matches!(
                        kind,
                        PassKind::DepthPrepass
                            | PassKind::Geometry
                            | PassKind::ObjectId
                            | PassKind::Outline
                    ),
                    depth_attachment: None,
                    material: None,
                    bindings: Default::default(),
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
        ImageSubresourceRange, ResourceAccessMode, ResourceAccessStage, ResourceAccessUsage,
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
        pass.uses_depth = false;
        pass
    }

    fn compile(
        passes: &[PassDesc],
        order: &[usize],
    ) -> Result<MetalExecutionPlan, RenderGraphError> {
        let format_at = |id: crate::render_graph::ResourceId| {
            Some(if id == ResourceId(99) {
                ImageFormat::D32SfloatS8Uint
            } else {
                ImageFormat::R16G16B16A16Sfloat
            })
        };
        MetalExecutionPlan::compile_order(
            order,
            |index| passes.get(index),
            &format_at,
            &|id| Some(format!("r{}", id.0)),
            &[],
            &[],
        )
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
                "0:geometry:Geometry:reads=[]:writes=[]:colors=[]:uses_depth=false:depth=none",
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
        assert_eq!(plan.passes()[0].pass_index, 0);
        assert_eq!(plan.passes()[1].pass_index, 1);
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
                ResourceAccessMode::ReadWrite,
                ResourceAccessUsage::ColorAttachment,
                ResourceAccessStage::ColorAttachmentOutput,
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
        geometry.uses_depth = true;
        geometry.depth_target = Some(ResourceId(99));
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
                    ResourceAccessMode::ReadWrite,
                    ResourceAccessUsage::ColorAttachment,
                    ResourceAccessStage::ColorAttachmentOutput,
                    ImageSubresourceRange::WHOLE_COLOR,
                ),
            ]
        );
        assert_eq!(
            record.color_attachments,
            vec![MetalColorAttachmentRecord {
                resource: ResourceId(7),
                name: "r7".to_string(),
                format: ImageFormat::R16G16B16A16Sfloat,
                load_op: LoadOp::Load,
                store_op: StoreOp::Store,
                clear_value: ClearValue::OPAQUE_BLACK,
            }]
        );
        assert_eq!(
            record.depth_attachment,
            Some(MetalDepthAttachmentOps {
                resource: ResourceId(99),
                format: ImageFormat::D32SfloatS8Uint,
                stencil_ops: crate::render_pass::AttachmentOps {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                    clear_value: ClearValue::DepthStencil {
                        depth: 0.0,
                        stencil: 1
                    }
                },
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
    fn test_classifies_compiled_sync_ops_as_explicit_stage_coverage() {
        let geometry = pass("geometry", PassType::Graphics, Some(PassKind::Geometry));
        let tonemap = pass("tonemap", PassType::Graphics, Some(PassKind::Fullscreen));
        let passes = [geometry, tonemap];

        // Attachment→sampled RAW before tonemap, plus a frame-end contract op.
        let attachment_write = ImageSyncOp {
            resource: ResourceId(3),
            range: ImageSubresourceRange::WHOLE_COLOR,
            before: crate::render_graph::ImageSyncState::Access {
                usage: ResourceAccessUsage::ColorAttachment,
                stage: ResourceAccessStage::ColorAttachmentOutput,
                mode: ResourceAccessMode::Write,
            },
            after: crate::render_graph::ImageSyncState::Access {
                usage: ResourceAccessUsage::Sampled,
                stage: ResourceAccessStage::FragmentShader,
                mode: ResourceAccessMode::Read,
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
        let name_at = |id: ResourceId| Some(format!("r{}", id.0));
        let plan = MetalExecutionPlan::compile_order(
            &[0, 1],
            |index| passes.get(index),
            &format_at,
            &name_at,
            &[(Some(1), attachment_write), (None, frame_end)],
            &[],
        )
        .unwrap();

        assert_eq!(
            plan.sync_records(),
            &[
                MetalSyncRecord {
                    pass: Some(1),
                    resource: ResourceId(3),
                    buffer_range: None,
                    coverage: MetalSyncCoverage::ExplicitStageBarrier,
                },
                MetalSyncRecord {
                    pass: None,
                    resource: ResourceId(3),
                    buffer_range: None,
                    coverage: MetalSyncCoverage::ExplicitStageBarrier,
                },
            ]
        );
    }

    #[test]
    fn test_accepts_neutral_compute_and_transfer_records() {
        let passes = vec![
            pass("compute", PassType::Compute, None),
            pass("transfer", PassType::Transfer, None),
        ];
        let plan = compile(&passes, &[0, 1]).unwrap();
        assert_eq!(plan.passes()[0].pass_type, PassType::Compute);
        assert_eq!(plan.passes()[1].pass_type, PassType::Transfer);
    }

    #[test]
    fn test_accepts_untagged_graphics_without_implicit_attachments() {
        let passes = vec![pass("custom", PassType::Graphics, None)];
        let plan = compile(&passes, &[0]).unwrap();
        assert_eq!(plan.passes()[0].kind, PassKind::Geometry);
        assert!(!plan.passes()[0].uses_depth);
        assert!(plan.passes()[0].depth_attachment.is_none());
        assert!(plan.passes()[0].color_attachments.is_empty());
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
