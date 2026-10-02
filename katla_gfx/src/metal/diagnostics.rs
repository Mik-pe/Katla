//! Plain diagnostic payload copied from Metal 4 commit feedback.
//!
//! The feedback dispatcher copies NSError data before waking submission waiters.
//! Metal 4 feedback reports queue failures and GPU timestamps; it does not expose
//! the legacy command-buffer per-encoder status array.

use crate::error::{GpuExecutionFailure, RendererError};
use objc2_foundation::NSError;

/// Native queue failure owned independently of the feedback callback.
#[derive(Clone, Debug)]
pub(crate) struct CommitError {
    pub(crate) code: i64,
    pub(crate) domain: String,
    pub(crate) description: String,
}

impl CommitError {
    /// Copies the error carried by MTL4CommitFeedback.
    pub(crate) fn from_native(error: &NSError) -> Self {
        Self {
            code: error.code() as i64,
            domain: error.domain().to_string(),
            description: error.localizedDescription().to_string(),
        }
    }

    /// Associates native feedback with the submission's deterministic label.
    pub(crate) fn renderer_error(&self, label: &str) -> RendererError {
        RendererError::GpuExecutionFailed(Box::new(GpuExecutionFailure {
            backend: "Metal4",
            label: label.into(),
            status: "CommitFailed".into(),
            code: Some(self.code),
            domain: Some(self.domain.clone()),
            description: Some(self.description.clone()),
            encoders: Vec::new(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::command::{GpuCommandBuffer, GpuRenderEncoder};
    use objc2_foundation::NSString;
    use objc2_metal::MTL4CommandBuffer;

    #[test]
    fn test_native_commit_error_preserves_signed_code_and_submission_label() {
        let domain = NSString::from_str("MTL4CommandQueueErrorDomain");
        // SAFETY: No user-info dictionary is supplied.
        let native = unsafe { NSError::errorWithDomain_code_userInfo(&domain, -17, None) };
        let error = CommitError::from_native(&native);
        drop(native);
        let RendererError::GpuExecutionFailed(failure) = error.renderer_error("slot.2.frame.5")
        else {
            panic!("typed Metal4 execution failure");
        };
        assert_eq!(failure.backend, "Metal4");
        assert_eq!(failure.label, "slot.2.frame.5");
        assert_eq!(failure.status, "CommitFailed");
        assert_eq!(failure.code, Some(-17));
        assert_eq!(
            failure.domain.as_deref(),
            Some("MTL4CommandQueueErrorDomain")
        );
        assert!(!failure.description.as_deref().unwrap().is_empty());
        assert!(failure.encoders.is_empty());
        let message = RendererError::GpuExecutionFailed(failure).to_string();
        assert!(message.contains("slot.2.frame.5"));
        assert!(message.contains("MTL4CommandQueueErrorDomain"));
    }

    #[test]
    fn test_metal4_command_buffer_receives_native_commit_feedback() {
        let ctx =
            crate::metal::context::MetalContext::init_headless().expect("headless Metal4 context");
        let mut cmd = ctx.create_command_buffer();
        cmd.inner
            .setLabel(Some(&NSString::from_str("feedback_smoke")));
        cmd.begin();
        cmd.end();
        cmd.submit(&ctx);
        let feedback = cmd.wait_until_completed().unwrap();
        assert!(cmd.completion.is_complete());
        assert!(feedback.error.is_none());
        assert!(feedback.gpu_start.is_finite());
        assert!(feedback.gpu_end >= feedback.gpu_start);
    }

    #[test]
    fn test_labeled_metal4_render_pass_receives_native_commit_feedback() {
        use crate::render_pass::{ClearValue, LoadOp, StoreOp};
        use crate::texture::{ImageFormat, TextureDescriptor, TextureUsage};

        let ctx =
            crate::metal::context::MetalContext::init_headless().expect("headless Metal4 context");
        let desc = TextureDescriptor::new(4, 4, ImageFormat::B8G8R8A8Srgb)
            .with_usage(TextureUsage::COLOR_ATTACHMENT);
        let (_texture, view) = ctx.create_texture(&desc).expect("target texture");
        let mut cmd = ctx.create_command_buffer();
        cmd.inner
            .setLabel(Some(&NSString::from_str("render_graph_frame.12")));
        cmd.begin();
        let encoder = cmd.begin_render_pass(crate::backend::command::RenderPassInfo {
            color_attachments: vec![crate::backend::command::ColorAttachmentInfo {
                view,
                load_op: LoadOp::Clear,
                store_op: StoreOp::Store,
                clear_value: ClearValue::OPAQUE_BLACK,
            }],
            depth_attachment: None,
            debug_label: Some("diag_label_smoke"),
        });
        encoder.end_encoding();
        cmd.end();
        cmd.submit(&ctx);
        let feedback = cmd.wait_until_completed().unwrap();
        assert!(feedback.error.is_none());
        assert!(feedback.gpu_end >= feedback.gpu_start);
        assert_eq!(
            cmd.inner.label().unwrap().to_string(),
            "render_graph_frame.12"
        );
    }
}
