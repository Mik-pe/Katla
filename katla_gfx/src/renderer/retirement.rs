//! Backend-neutral retirement diagnostics.
//!
//! Destroyed or replaced GPU resources are not freed immediately: they wait
//! for the submissions that can still reference them to complete. This module
//! holds the portable snapshot type reported by
//! [`GpuRenderer`](crate::GpuRenderer) implementations; each backend owns its
//! own queue that produces it.

/// Diagnostics snapshot of a retirement queue.
///
/// `oldest_retired_at` is the frame counter of the oldest pending entry; a
/// pending entry whose age (`current_frame - oldest_retired_at`) far exceeds
/// the frames-in-flight count indicates retirement draining has stopped
/// (e.g. rendering ceased) rather than a leak.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetirementSnapshot {
    /// Pending replaced/destroyed vertex, index, and UI buffers.
    pub buffers: usize,
    /// Pending destroyed textures.
    pub textures: usize,
    /// Pending replaced/destroyed pipelines.
    pub pipelines: usize,
    /// Pending standalone descriptor set layouts.
    pub descriptor_set_layouts: usize,
    /// Pending skeleton joint-matrix buffers.
    pub skeleton_buffers: usize,
    /// Pending bindless slot releases.
    pub bindless_slots: usize,
    /// Device-memory bytes held by pending entries that know their size
    /// (buffers and skeleton buffers; textures and pipelines do not).
    pub pending_bytes: u64,
    /// Frame counter at which the oldest pending entry was retired.
    pub oldest_retired_at: Option<u64>,
}

impl RetirementSnapshot {
    /// Total number of pending retirement entries.
    pub fn total(&self) -> usize {
        self.buffers
            + self.textures
            + self.pipelines
            + self.descriptor_set_layouts
            + self.skeleton_buffers
            + self.bindless_slots
    }

    /// Human-readable per-kind summary for logs and diagnostics.
    pub fn summary(&self) -> String {
        let mut out = format!("pending retirements: {}", self.total());
        for (label, count) in [
            ("buffers", self.buffers),
            ("textures", self.textures),
            ("pipelines", self.pipelines),
            ("descriptor-set-layouts", self.descriptor_set_layouts),
            ("skeleton-buffers", self.skeleton_buffers),
            ("bindless-slots", self.bindless_slots),
        ] {
            if count > 0 {
                out.push_str(&format!(" {label}={count}"));
            }
        }
        if self.pending_bytes > 0 {
            out.push_str(&format!(" bytes={}", self.pending_bytes));
        }
        if let Some(retired_at) = self.oldest_retired_at {
            out.push_str(&format!(" oldest=frame:{retired_at}"));
        }
        out
    }
}
