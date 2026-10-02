pub(crate) fn stage_mask(
    stage: crate::render_graph::ResourceAccessStage,
) -> objc2_metal::MTLStages {
    use crate::render_graph::ResourceAccessStage::*;
    use objc2_metal::MTLStages;
    match stage {
        VertexInput | VertexShader => MTLStages::Vertex,
        FragmentShader | ColorAttachmentOutput | DepthStencil => MTLStages::Fragment,
        ComputeShader => MTLStages::Dispatch,
        Transfer | Host => MTLStages::Blit,
        DrawIndirect | Present | AllGraphics => MTLStages::All,
    }
}

fn compiled_scope(
    graph: &crate::render_graph::FrameGraph<super::metal_renderer::MetalRenderer>,
    pass: usize,
) -> (objc2_metal::MTLStages, objc2_metal::MTLStages, bool) {
    use crate::render_graph::{BufferSyncState, ImageSyncState};
    use objc2_metal::MTLStages;
    let mut before = MTLStages::empty();
    let mut after = MTLStages::empty();
    for op in graph.image_sync_ops(pass) {
        if let ImageSyncState::Access { stage, .. } = op.before {
            before |= stage_mask(stage);
        }
        if let ImageSyncState::Access { stage, .. } = op.after {
            after |= stage_mask(stage);
        }
    }
    for op in graph.buffer_sync_ops(pass) {
        if let BufferSyncState::Access { stage, .. } = op.before {
            before |= stage_mask(stage);
        }
        if let BufferSyncState::Access { stage, .. } = op.after {
            after |= stage_mask(stage);
        }
    }
    if let Some(boundary) = graph.pass_boundary(pass) {
        if boundary.external_upload_dependency {
            before |= MTLStages::Blit;
        }
        for predecessor in &boundary.predecessors {
            if let Some(boundary) = graph.pass_boundary(*predecessor) {
                before |= match boundary.encoder {
                    crate::render_graph::EncoderKind::Render => {
                        MTLStages::Vertex | MTLStages::Fragment
                    }
                    crate::render_graph::EncoderKind::Compute => MTLStages::Dispatch,
                    crate::render_graph::EncoderKind::Blit => MTLStages::Blit,
                };
            }
        }
    }
    let alias = graph.texture_alias_handoff_before(pass);
    if alias {
        before |= MTLStages::All;
    }
    (before, after, alias)
}

fn captured_scope(
    before: objc2_metal::MTLStages,
    after: objc2_metal::MTLStages,
    visibility: objc2_metal::MTL4VisibilityOptions,
) -> crate::render_graph::capture::CapturedNativeSyncScope {
    crate::render_graph::capture::CapturedNativeSyncScope {
        range: crate::render_graph::capture::CapturedNativeSyncRange::Global,
        source_stages: before.bits() as u64,
        destination_stages: after.bits() as u64,
        source_access: 0,
        destination_access: 0,
        old_layout: None,
        new_layout: None,
        visibility: visibility.bits() as u64,
    }
}

pub(crate) fn apply_compiled_boundary(
    encoder: &objc2::runtime::ProtocolObject<dyn objc2_metal::MTL4CommandEncoder>,
    graph: &crate::render_graph::FrameGraph<super::metal_renderer::MetalRenderer>,
    pass: usize,
    consumer: objc2_metal::MTLStages,
    resources: &super::encoding_resources::EncodingResources,
) {
    use objc2_metal::{MTL4CommandEncoder, MTL4VisibilityOptions};
    let (before, after, alias) = compiled_scope(graph, pass);
    let visibility = MTL4VisibilityOptions::Device
        | if alias {
            MTL4VisibilityOptions::ResourceAlias
        } else {
            MTL4VisibilityOptions::None
        };
    if !before.is_empty() {
        #[cfg(test)]
        resources.native_barriers(1);
        encoder.barrierAfterQueueStages_beforeStages_visibilityOptions(
            before,
            after | consumer,
            visibility,
        );
    }
    if !resources.capture_enabled() {
        return;
    }
    let boundary = format!("metal4.queue_to_encoder.pass.{pass}");
    let repeated = resources.captured_boundary(&boundary);
    let (required_before, required_after, required_alias) = compiled_scope(graph, pass);
    let required_visibility = MTL4VisibilityOptions::Device
        | if required_alias {
            MTL4VisibilityOptions::ResourceAlias
        } else {
            MTL4VisibilityOptions::None
        };
    let native_scope = vec![captured_scope(before, after | consumer, visibility)];
    let required_scope = vec![captured_scope(
        required_before,
        required_after | consumer,
        required_visibility,
    )];
    for op in graph.image_sync_ops(pass) {
        let mut operation =
            crate::render_graph::capture::CapturedSyncOperation::image(op, &boundary);
        if repeated {
            operation.origin = "backend".into();
        }
        if !before.is_empty() {
            operation.native_scope = native_scope.clone();
            operation.required_native_scope = required_scope.clone();
        }
        resources.record_sync(if before.is_empty() {
            operation.omitted("initial undefined image has no preceding native stage")
        } else {
            operation
        });
    }
    for op in graph.buffer_sync_ops(pass) {
        let mut operation =
            crate::render_graph::capture::CapturedSyncOperation::buffer(op, &boundary);
        if repeated {
            operation.origin = "backend".into();
        }
        if !before.is_empty() {
            operation.native_scope = native_scope.clone();
            operation.required_native_scope = required_scope.clone();
        }
        resources.record_sync(if before.is_empty() {
            operation.omitted("initial undefined buffer has no preceding native stage")
        } else {
            operation
        });
    }
}

pub(crate) fn apply_final_image_boundary(
    encoder: &objc2::runtime::ProtocolObject<dyn objc2_metal::MTL4CommandEncoder>,
    graph: &crate::render_graph::FrameGraph<super::metal_renderer::MetalRenderer>,
    resources: &super::encoding_resources::EncodingResources,
) {
    use crate::render_graph::ImageSyncState;
    use objc2_metal::{MTL4CommandEncoder, MTL4VisibilityOptions, MTLStages};
    let mut before = MTLStages::empty();
    let mut after = MTLStages::empty();
    for op in graph.final_image_sync_ops() {
        if let ImageSyncState::Access { stage, .. } = op.before {
            before |= stage_mask(stage);
        }
        if let ImageSyncState::Access { stage, .. } = op.after {
            after |= stage_mask(stage);
        }
    }
    if !before.is_empty() && !after.is_empty() {
        #[cfg(test)]
        resources.native_barriers(2);
        encoder.barrierAfterQueueStages_beforeStages_visibilityOptions(
            before,
            after,
            MTL4VisibilityOptions::Device,
        );
        encoder.barrierAfterStages_beforeQueueStages_visibilityOptions(
            after,
            after,
            MTL4VisibilityOptions::Device,
        );
    }
    if !resources.capture_enabled() {
        return;
    }
    let mut required_before = MTLStages::empty();
    let mut required_after = MTLStages::empty();
    for op in graph.final_image_sync_ops() {
        if let ImageSyncState::Access { stage, .. } = op.before {
            required_before |= stage_mask(stage);
        }
        if let ImageSyncState::Access { stage, .. } = op.after {
            required_after |= stage_mask(stage);
        }
    }
    for op in graph.final_image_sync_ops() {
        let mut operation =
            crate::render_graph::capture::CapturedSyncOperation::image(op, "metal4.final_output");
        if !before.is_empty() && !after.is_empty() {
            operation.native_scope = vec![
                captured_scope(before, after, MTL4VisibilityOptions::Device),
                captured_scope(after, after, MTL4VisibilityOptions::Device),
            ];
            operation.required_native_scope = vec![
                captured_scope(
                    required_before,
                    required_after,
                    MTL4VisibilityOptions::Device,
                ),
                captured_scope(
                    required_after,
                    required_after,
                    MTL4VisibilityOptions::Device,
                ),
            ];
        }
        resources.record_sync(if before.is_empty() || after.is_empty() {
            operation.omitted("final output contract has no native stage transition")
        } else {
            operation
        });
    }
}

/// Empty command lists and zero dispatches require no GPU encoder. Shared storage
/// becomes host-visible when the exact producing submission completes.
pub(crate) fn capture_skipped_boundary(
    graph: &crate::render_graph::FrameGraph<super::metal_renderer::MetalRenderer>,
    pass: usize,
    resources: &super::encoding_resources::EncodingResources,
) {
    if !resources.capture_enabled() {
        return;
    }
    for op in graph.image_sync_ops(pass) {
        resources.record_sync(
            crate::render_graph::capture::CapturedSyncOperation::image(op, "metal4.no_work")
                .omitted("no resource access is encoded for this pass"),
        );
    }
    for op in graph.buffer_sync_ops(pass) {
        resources.record_sync(crate::render_graph::capture::CapturedSyncOperation::buffer(op,"metal4.no_work").omitted("no encoder; shared host visibility is observed after exact submission completion"));
    }
}

pub(crate) enum NativeBoundary {
    UploadAcquire,
    UploadRelease,
    Transfers,
    BetweenCommands {
        previous_compute: bool,
        current_compute: bool,
    },
}

/// Record arguments only after the native call. The required scope comes from
/// the fixed native ownership contract, independently of those arguments.
pub(crate) fn capture_native_boundary(
    resources: &super::encoding_resources::EncodingResources,
    label: &str,
    boundary: NativeBoundary,
    before: objc2_metal::MTLStages,
    after: objc2_metal::MTLStages,
    visibility: objc2_metal::MTL4VisibilityOptions,
) {
    #[cfg(test)]
    resources.native_barriers(1);
    if !resources.capture_enabled() {
        return;
    }
    use objc2_metal::{MTL4VisibilityOptions, MTLStages};
    let (required_before, required_after) = match boundary {
        NativeBoundary::UploadAcquire => (MTLStages::All, MTLStages::Blit),
        NativeBoundary::UploadRelease => (MTLStages::Blit, MTLStages::All),
        NativeBoundary::Transfers => (MTLStages::Blit, MTLStages::Blit),
        NativeBoundary::BetweenCommands {
            previous_compute,
            current_compute,
        } => (
            if previous_compute {
                MTLStages::Dispatch
            } else {
                MTLStages::Blit
            },
            if current_compute {
                MTLStages::Dispatch
            } else {
                MTLStages::Blit
            },
        ),
    };
    resources.record_sync(crate::render_graph::capture::CapturedSyncOperation {
        resource: u32::MAX,
        origin: "backend".into(),
        resource_kind: "global".into(),
        producer: None,
        consumer: None,
        version: "native ownership".into(),
        range: "Global".into(),
        source: format!("{before:?}"),
        destination: format!("{after:?}"),
        reason: "native ownership dependency".into(),
        boundary: label.into(),
        native_scope: vec![captured_scope(before, after, visibility)],
        required_native_scope: vec![captured_scope(
            required_before,
            required_after,
            MTL4VisibilityOptions::Device,
        )],
        emitted: true,
        omission_reason: None,
    });
}
