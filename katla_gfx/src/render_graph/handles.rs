//! Typed handle types for render graph passes and resources.

/// Typed handle identifying a render pass within the frame graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PassId(pub u32);

/// Typed handle identifying a resource within the frame graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResourceId(pub u32);
