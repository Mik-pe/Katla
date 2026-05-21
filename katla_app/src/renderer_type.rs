//! Renderer type alias for dynamic backend selection.
//!
//! Uses `AnyRenderer` which wraps both Vulkan and Metal backends
//! and allows runtime selection.

pub type Renderer = katla_gfx::AnyRenderer;
pub type FrameGraph = katla_gfx::AnyFrameGraph;
