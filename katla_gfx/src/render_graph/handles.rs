//! Typed handle types for render graph passes and resources.

/// Opaque identity of one pass in its owning graph.
///
/// Obtain it from graph construction or `pass_id`. Inserting passes preserves
/// existing identities; another graph never accepts this handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PassId {
    pub(super) graph: u64,
    pub(super) slot: usize,
}

pub(super) fn next_graph_identity() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_GRAPH: AtomicU64 = AtomicU64::new(1);
    NEXT_GRAPH
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .expect("graph identities exhausted")
}

/// Typed handle identifying a resource within the frame graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResourceId(pub u32);
