//! Render frame context for passing per-frame data to execution closures

use std::any::Any;

/// Trait for render frame context - provides access to per-frame data
/// without coupling katla_vulkan to application types.
pub trait RenderFrameContext: Any + Send + Sync {
    /// Get a reference to the context as Any for downcasting
    fn as_any(&self) -> &dyn Any;

    /// Get a mutable reference to the context as Any for downcasting
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// Default empty context implementation
pub struct EmptyRenderFrameContext;

impl RenderFrameContext for EmptyRenderFrameContext {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
