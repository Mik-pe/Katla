//! Metal executable pass records compiled from the backend-neutral render graph.
//!
//! The render graph owns topology and ordering. Metal owns native encoding, but it
//! consumes this ordered record stream directly instead of rebuilding an editor
//! pipeline from singleton semantic checks.

use crate::render_graph::{
    FrameGraph, ImageAccess, PassDesc, PassId, PassKind, PassType, RenderGraphError, ResourceId,
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
            reads: pass.reads.clone(),
            writes: pass.writes.clone(),
            image_accesses: pass.image_accesses.clone(),
            color_attachments: pass
                .color_attachments
                .iter()
                .map(
                    |&(resource, format, load_op, store_op, clear_value)| {
                        MetalColorAttachmentRecord {
                            resource,
                            format,
                            load_op,
                            store_op,
                            clear_value,
                        }
                    },
                )
                .collect(),
            uses_depth: pass.uses_depth,
            depth_attachment: pass.depth_attachment.map(
                |(load_op, store_op, clear_value)| MetalDepthAttachmentOps {
                    load_op,
                    store_op,
                    clear_value,
                },
            ),
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

/// Ordered Metal records derived from the graph compiler's canonical execution order.
#[derive(Debug, Clone, PartialEq)]
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
            ImageFormat::R16G16B16A16Sfloat,
            LoadOp::Load,
            StoreOp::Store,
            ClearValue::OPAQUE_BLACK,
        ));
        geometry.depth_attachment = Some((
            LoadOp::Load,
            StoreOp::Store,
            ClearValue::DepthStencil {
                depth: 0.0,
                stencil: 1,
            },
        ));

        let plan = compile(&[geometry], &[0]).unwrap();
        let record = &plan.passes()[0];
        assert_eq!(record.reads, vec![ResourceId(4)]);
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
