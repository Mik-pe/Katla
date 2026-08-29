//! Structured GPU execution diagnostics for Metal command buffers.
//!
//! Validation mode creates command buffers through [`objc2_metal::MTLCommandBufferDescriptor`]
//! with encoder execution status enabled so a terminal GPU failure carries per-encoder
//! state. Release mode keeps the default (low-overhead) configuration unless diagnostics
//! are explicitly requested. The reporting structures here are plain data: no raw
//! Objective-C pointers or object identity enter logs or snapshots.

use objc2::rc::Retained;
use objc2_metal::{
    MTLCommandBufferDescriptor, MTLCommandBufferEncoderInfo, MTLCommandBufferErrorOption,
    MTLCommandEncoderErrorState,
};

/// Whether a command buffer records per-encoder execution status.
///
/// Enabling encoder status has measurable CPU/GPU/memory overhead on some
/// platforms, so it is reserved for validation builds and explicit opt-in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuDiagnosticsMode {
    /// Default Metal reporting configuration; suitable for release rendering.
    Release,
    /// Encoder execution status enabled; suitable for validation and debugging.
    Validation,
}

impl GpuDiagnosticsMode {
    /// Command-buffer creation path implied by the mode.
    pub(crate) fn enabled(self) -> bool {
        matches!(self, GpuDiagnosticsMode::Validation)
    }

    /// `MTLCommandBufferErrorOption` bits for the mode.
    pub(crate) fn error_options(self) -> MTLCommandBufferErrorOption {
        if self.enabled() {
            MTLCommandBufferErrorOption::EncoderExecutionStatus
        } else {
            MTLCommandBufferErrorOption::None
        }
    }
}

/// Terminal state of one encoder inside a failed command buffer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuEncoderDiagnostics {
    /// Debug label recorded on the encoder at submission time.
    pub label: String,
    /// Signposts inserted into the encoder, in insertion order.
    pub debug_signposts: Vec<String>,
    /// Metal's per-encoder error state.
    pub error_state: GpuEncoderErrorState,
}

/// Rust mirror of `MTLCommandEncoderErrorState` (stable, printable).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuEncoderErrorState {
    Unknown,
    Completed,
    Affected,
    Pending,
    Faulted,
}

impl GpuEncoderErrorState {
    fn from_mtl(state: MTLCommandEncoderErrorState) -> Self {
        match state {
            MTLCommandEncoderErrorState::Completed => Self::Completed,
            MTLCommandEncoderErrorState::Affected => Self::Affected,
            MTLCommandEncoderErrorState::Pending => Self::Pending,
            MTLCommandEncoderErrorState::Faulted => Self::Faulted,
            _ => Self::Unknown,
        }
    }
}

impl core::fmt::Display for GpuEncoderErrorState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let name = match self {
            Self::Unknown => "unknown",
            Self::Completed => "completed",
            Self::Affected => "affected",
            Self::Pending => "pending",
            Self::Faulted => "faulted",
        };
        f.write_str(name)
    }
}

/// Structured report of a failed command buffer, safe to log or attach to a
/// [`crate::renderer::RendererError`](crate::error::RendererError).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuCommandBufferDiagnostics {
    /// Deterministic label of the failed command buffer.
    pub command_buffer_label: String,
    /// Native error code (`NSError.code`).
    pub code: u64,
    /// Native error domain (`NSError.domain`).
    pub domain: String,
    /// Localized human-readable description.
    pub description: String,
    /// Per-encoder execution status, in submission order.
    pub encoders: Vec<GpuEncoderDiagnostics>,
}

impl GpuCommandBufferDiagnostics {
    /// Extract structured diagnostics from a failed command buffer's error.
    /// Returns `None` when the buffer carries no `NSError`.
    pub(crate) fn from_error(
        command_buffer_label: &str,
        error: &objc2_foundation::NSError,
    ) -> Option<Self> {
        let encoders = extract_encoder_diagnostics(error);
        Some(Self {
            command_buffer_label: command_buffer_label.to_owned(),
            code: error.code() as u64,
            domain: error.domain().to_string(),
            description: error.localizedDescription().to_string(),
            encoders,
        })
    }

    /// Render as deterministic multi-line text for logs and renderer errors.
    pub(crate) fn render(&self) -> String {
        let mut text = format!(
            "command buffer '{}' failed: code={} domain={} description={}",
            self.command_buffer_label, self.code, self.domain, self.description
        );
        if self.encoders.is_empty() {
            text.push_str("\n  encoders: (encoder execution status not recorded)");
        } else {
            text.push_str("\n  encoders:");
            for encoder in &self.encoders {
                text.push_str(&format!(
                    "\n    [{}] state={} signposts={}",
                    encoder.label,
                    encoder.error_state,
                    if encoder.debug_signposts.is_empty() {
                        "none".to_owned()
                    } else {
                        encoder.debug_signposts.join(",")
                    }
                ));
            }
        }
        text
    }

    /// The first faulted encoder, the usual prime suspect.
    pub(crate) fn faulted_encoder(&self) -> Option<&GpuEncoderDiagnostics> {
        self.encoders
            .iter()
            .find(|encoder| encoder.error_state == GpuEncoderErrorState::Faulted)
    }
}

/// Read the `MTLCommandBufferEncoderInfoErrorKey` array out of the error's
/// user info. Absent when encoder status was not enabled at creation.
fn extract_encoder_diagnostics(error: &objc2_foundation::NSError) -> Vec<GpuEncoderDiagnostics> {
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2_foundation::NSArray;

    let user_info = error.userInfo();
    // SAFETY: reading the well-known extern static key exported by Metal.
    let encoder_info_key = unsafe { objc2_metal::MTLCommandBufferEncoderInfoErrorKey };
    let Some(any) = user_info.objectForKeyedSubscript(encoder_info_key) else {
        return Vec::new();
    };
    // SAFETY: Metal documents the value under this key as
    // NSArray<MTLCommandBufferEncoderInfo>; the cast preserves the object.
    // SAFETY: Metal documents the value under this key as
    // NSArray<MTLCommandBufferEncoderInfo>; the cast preserves the object.
    let array = unsafe {
        Retained::cast_unchecked::<NSArray<ProtocolObject<dyn MTLCommandBufferEncoderInfo>>>(any)
    };

    array
        .iter()
        .map(|info| GpuEncoderDiagnostics {
            label: info.label().to_string(),
            debug_signposts: info
                .debugSignposts()
                .iter()
                .map(|signpost| signpost.to_string())
                .collect(),
            error_state: GpuEncoderErrorState::from_mtl(info.errorState()),
        })
        .collect()
}

/// Build a descriptor for command-buffer creation in the requested mode.
pub(crate) fn command_buffer_descriptor(
    mode: GpuDiagnosticsMode,
) -> Retained<MTLCommandBufferDescriptor> {
    let descriptor = MTLCommandBufferDescriptor::new();
    descriptor.setErrorOptions(mode.error_options());
    descriptor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_mode_enables_encoder_execution_status() {
        let mode = GpuDiagnosticsMode::Validation;
        assert!(mode.enabled());
        assert_eq!(
            mode.error_options(),
            MTLCommandBufferErrorOption::EncoderExecutionStatus
        );
        let descriptor = command_buffer_descriptor(mode);
        assert_eq!(descriptor.errorOptions(), mode.error_options());
    }

    #[test]
    fn test_release_mode_keeps_default_error_options() {
        let mode = GpuDiagnosticsMode::Release;
        assert!(!mode.enabled());
        assert_eq!(mode.error_options(), MTLCommandBufferErrorOption::None);
        let descriptor = command_buffer_descriptor(mode);
        assert_eq!(descriptor.errorOptions(), MTLCommandBufferErrorOption::None);
    }

    #[test]
    fn test_diagnostics_render_lists_encoder_states_in_order() {
        let diagnostics = GpuCommandBufferDiagnostics {
            command_buffer_label: "render_graph_frame_42".to_owned(),
            code: 1,
            domain: "Metal".to_owned(),
            description: "GPU hung".to_owned(),
            encoders: vec![
                GpuEncoderDiagnostics {
                    label: "geometry_pass".to_owned(),
                    debug_signposts: Vec::new(),
                    error_state: GpuEncoderErrorState::Completed,
                },
                GpuEncoderDiagnostics {
                    label: "shadow_cascade_1".to_owned(),
                    debug_signposts: vec!["draw_batch_7".to_owned()],
                    error_state: GpuEncoderErrorState::Faulted,
                },
                GpuEncoderDiagnostics {
                    label: "tonemap".to_owned(),
                    debug_signposts: Vec::new(),
                    error_state: GpuEncoderErrorState::Pending,
                },
            ],
        };

        let text = diagnostics.render();
        assert!(text.contains("command buffer 'render_graph_frame_42' failed"));
        assert!(text.contains("code=1 domain=Metal"));
        // Submission order preserved.
        let geometry = text.find("geometry_pass").unwrap();
        let shadow = text.find("shadow_cascade_1").unwrap();
        let tonemap = text.find("tonemap").unwrap();
        assert!(geometry < shadow && shadow < tonemap);
        assert!(text.contains("[shadow_cascade_1] state=faulted signposts=draw_batch_7"));
        assert!(text.contains("[tonemap] state=pending signposts=none"));

        let faulted = diagnostics.faulted_encoder().expect("one faulted encoder");
        assert_eq!(faulted.label, "shadow_cascade_1");
    }

    #[test]
    fn test_diagnostics_render_without_encoder_status_is_explicit() {
        let diagnostics = GpuCommandBufferDiagnostics {
            command_buffer_label: "shadow_pass".to_owned(),
            code: 8,
            domain: "Metal".to_owned(),
            description: "out of memory".to_owned(),
            encoders: Vec::new(),
        };

        let text = diagnostics.render();
        assert!(text.contains("encoders: (encoder execution status not recorded)"));
        assert!(diagnostics.faulted_encoder().is_none());
    }

    #[test]
    fn test_validation_mode_command_buffer_submits_cleanly() {
        use objc2_metal::{MTLCommandBuffer, MTLCommandBufferStatus};

        let ctx =
            crate::metal::context::MetalContext::init_headless().expect("headless Metal context");
        let cmd = ctx.create_command_buffer_with_diagnostics(GpuDiagnosticsMode::Validation);
        cmd.inner.commit();
        cmd.inner.waitUntilCompleted();
        assert_eq!(cmd.inner.status(), MTLCommandBufferStatus::Completed);
    }

    #[test]
    fn test_labeled_render_pass_encodes_and_submits() {
        use crate::backend::command::{GpuCommandBuffer, GpuRenderEncoder};
        use crate::render_pass::LoadOp;
        use crate::texture::{ImageFormat, TextureDescriptor, TextureUsage};

        let ctx =
            crate::metal::context::MetalContext::init_headless().expect("headless Metal context");
        let desc = TextureDescriptor::new(4, 4, ImageFormat::B8G8R8A8Srgb)
            .with_usage(TextureUsage::COLOR_ATTACHMENT);
        let (_texture, view) = ctx.create_texture(&desc).expect("target texture");

        let mut cmd = ctx.create_command_buffer_with_diagnostics(GpuDiagnosticsMode::Validation);
        cmd.begin();

        let pass_info = crate::backend::command::RenderPassInfo {
            color_attachments: vec![crate::backend::command::ColorAttachmentInfo {
                view,
                load_op: LoadOp::Clear,
                store_op: crate::render_pass::StoreOp::Store,
                clear_value: crate::render_pass::ClearValue::color(0.0, 0.0, 0.0, 1.0),
            }],
            depth_attachment: None,
            debug_label: Some("diag_label_smoke"),
        };

        {
            let mut encoder = cmd.begin_render_pass(pass_info);
            encoder.end_encoding();
        }

        cmd.end();
        cmd.submit(&ctx);
        cmd.inner.waitUntilCompleted();
        use objc2_metal::{MTLCommandBuffer, MTLCommandBufferStatus};
        assert_eq!(cmd.inner.status(), MTLCommandBufferStatus::Completed);
    }

    #[test]
    fn test_release_mode_command_buffer_submits_cleanly() {
        use objc2_metal::{MTLCommandBuffer, MTLCommandBufferStatus};

        let ctx =
            crate::metal::context::MetalContext::init_headless().expect("headless Metal context");
        let cmd = ctx.create_command_buffer_with_diagnostics(GpuDiagnosticsMode::Release);
        cmd.inner.commit();
        cmd.inner.waitUntilCompleted();
        assert_eq!(cmd.inner.status(), MTLCommandBufferStatus::Completed);
    }
}
