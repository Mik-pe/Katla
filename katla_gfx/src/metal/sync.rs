use std::sync::atomic::{AtomicBool, Ordering};

use crate::backend::resource::{GpuEvent, GpuFence};

pub(crate) struct MetalFence {
    signaled: AtomicBool,
}

#[cfg(test)]
impl MetalFence {
    fn new() -> Self {
        Self {
            signaled: AtomicBool::new(false),
        }
    }

    fn signal(&self) {
        self.signaled.store(true, Ordering::Release);
    }

    fn reset(&self) {
        self.signaled.store(false, Ordering::Release);
    }
}

impl GpuFence for MetalFence {
    fn is_signaled(&self) -> bool {
        self.signaled.load(Ordering::Acquire)
    }
}

pub(crate) struct MetalEvent {}

impl GpuEvent for MetalEvent {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn test_fence_initial_state_unsignaled() {
        let fence = MetalFence::new();
        assert!(!fence.is_signaled());
    }

    #[test]
    fn test_fence_signal() {
        let fence = MetalFence::new();
        fence.signal();
        assert!(fence.is_signaled());
    }

    #[test]
    fn test_fence_reset() {
        let fence = MetalFence::new();
        fence.signal();
        assert!(fence.is_signaled());
        fence.reset();
        assert!(!fence.is_signaled());
    }

    #[test]
    fn test_fence_signal_idempotent() {
        let fence = MetalFence::new();
        fence.signal();
        fence.signal();
        assert!(fence.is_signaled());
    }

    #[test]
    fn test_fence_reset_idempotent() {
        let fence = MetalFence::new();
        fence.reset();
        assert!(!fence.is_signaled());
    }

    #[test]
    fn test_fence_signal_reset_cycle() {
        let fence = MetalFence::new();

        fence.signal();
        assert!(fence.is_signaled());

        fence.reset();
        assert!(!fence.is_signaled());

        fence.signal();
        assert!(fence.is_signaled());
    }

    #[test]
    fn test_fence_ordering_semantics() {
        let fence = MetalFence::new();
        fence.signaled.store(true, Ordering::Release);
        assert!(fence.signaled.load(Ordering::Acquire));
    }
}

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

pub(crate) fn apply_compiled_boundary(
    encoder: &objc2::runtime::ProtocolObject<dyn objc2_metal::MTL4CommandEncoder>,
    graph: &crate::render_graph::FrameGraph<super::metal_renderer::MetalRenderer>,
    pass: usize,
    consumer: objc2_metal::MTLStages,
) {
    use crate::render_graph::{BufferSyncState, ImageSyncState};
    use objc2_metal::{MTL4CommandEncoder, MTL4VisibilityOptions, MTLStages};
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
        if graph.is_builtin_buffer(op.resource) {
            before |= MTLStages::All;
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
    if !before.is_empty() {
        let visibility = MTL4VisibilityOptions::Device
            | if alias {
                MTL4VisibilityOptions::ResourceAlias
            } else {
                MTL4VisibilityOptions::None
            };
        encoder.barrierAfterQueueStages_beforeStages_visibilityOptions(
            before,
            after | consumer,
            visibility,
        );
    }
}

pub(crate) fn apply_final_image_boundary(
    encoder: &objc2::runtime::ProtocolObject<dyn objc2_metal::MTL4CommandEncoder>,
    graph: &crate::render_graph::FrameGraph<super::metal_renderer::MetalRenderer>,
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
}
