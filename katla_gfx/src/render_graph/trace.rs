//! Deterministic trace of the encoders a backend actually emitted.
//!
//! The compiled plan says what *should* run; this records what *did*. A backend
//! appends one [`ResourceExecutionTraceEntry`] per pass it encoded, in encode
//! order, carrying the pass identity, the declared attachment contract, and
//! backend-neutral counts. Nothing here names a native type, driver id, or
//! address, so a trace is comparable byte-for-byte across runs and machines.
//!
//! [`compare_with_compiled`] is the point of the trace: it fails when the
//! emitted pass order or the per-pass attachment contract diverges from the
//! compiled plan, which is the divergence a plan-only view cannot show.

use std::fmt;

use super::pass::{PassDesc, PassType};
use super::resource::GraphResourceDesc;

/// Why a compiled pass produced no encoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmittedPassOutcome {
    /// The backend created an encoder for the pass.
    Encoded,
    /// The pass had no work to encode, so the backend skipped it.
    SkippedNoWork,
}

impl fmt::Display for EmittedPassOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encoded => f.write_str("encoded"),
            Self::SkippedNoWork => f.write_str("skipped_no_work"),
        }
    }
}

/// One encoder a backend emitted for one compiled pass.
#[derive(Debug, Clone, PartialEq)]
pub struct ResourceExecutionTraceEntry {
    /// Declared pass index, so trace entries join the compiled plan directly.
    pub pass_index: usize,
    pub name: String,
    pub pass_type: PassType,
    /// Encoder order within the frame (0-based).
    pub encode_position: usize,
    pub outcome: EmittedPassOutcome,
    /// Draw calls encoded for this pass.
    pub draw_calls: usize,
    /// Object slots those draws covered.
    pub instances: usize,
    /// Color targets the encoder actually bound, in declaration order.
    pub color_targets: Vec<String>,
    /// Depth target the encoder actually bound, if any.
    pub depth_target: Option<String>,
    /// Operations supplied to the native color attachments, in binding order.
    pub color_attachment_ops: Vec<crate::render_pass::AttachmentOps>,
    /// Operations supplied to the native depth and stencil attachments.
    pub depth_attachment_ops: Option<crate::render_pass::DepthStencilAttachmentOps>,
}

/// A frame's emitted encoder trace.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResourceExecutionTrace {
    entries: Vec<ResourceExecutionTraceEntry>,
    /// Passive native observations from the same frame.
    pub backend: super::capture::BackendExecutionTrace,
}

impl ResourceExecutionTrace {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, entry: ResourceExecutionTraceEntry) {
        self.entries.push(entry);
    }

    pub fn entries(&self) -> &[ResourceExecutionTraceEntry] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Passes the backend created an encoder for, in encode order.
    pub fn encoded_passes(&self) -> impl Iterator<Item = &ResourceExecutionTraceEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.outcome == EmittedPassOutcome::Encoded)
    }
}

mod comparison;

#[cfg(test)]
mod tests;

pub(crate) use comparison::color_target_names;
pub use comparison::{TraceDivergence, compare_with_compiled};

impl fmt::Display for ResourceExecutionTrace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.entries.is_empty() {
            return writeln!(f, "no encoders emitted");
        }

        writeln!(f, "{} emitted encoder entries:", self.entries.len())?;
        for entry in &self.entries {
            let targets = if entry.color_targets.is_empty() {
                "none".to_string()
            } else {
                entry.color_targets.join(", ")
            };
            writeln!(
                f,
                "  [{}] pass {} ({}, {:?}) {} color [{}] depth {} draws {} instances {}",
                entry.encode_position,
                entry.pass_index,
                entry.name,
                entry.pass_type,
                entry.outcome,
                targets,
                entry.depth_target.as_deref().unwrap_or("none"),
                entry.draw_calls,
                entry.instances,
            )?;
            writeln!(
                f,
                "      color ops {:?} depth/stencil ops {:?}",
                entry.color_attachment_ops, entry.depth_attachment_ops,
            )?;
        }
        Ok(())
    }
}
