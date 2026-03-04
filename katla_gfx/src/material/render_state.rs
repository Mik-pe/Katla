//! Render state configuration for materials.

/// Render state configuration
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RenderState {
    pub depth_test: bool,
    pub depth_write: bool,
    pub cull_backfaces: bool,
    pub alpha_blending: bool,
}
