//! Low-level UI material primitives for immediate mode UI rendering.
//!
//! This module is reserved for future UI material configuration.

#![allow(dead_code)]

use crate::vertex::VertexLayout;
use crate::vulkan::vertexbinding::VertexBinding;

use super::MaterialDomain;
use super::RenderState;

/// Configuration for UI material creation.
///
/// Provides the low-level settings needed to create a UI material.
/// Reserved for future use.
#[derive(Debug, Clone)]
pub struct UiMaterialConfig {
    /// Vertex binding configuration from the UI vertex layout.
    pub vertex_binding: VertexBinding,
    /// Render state for UI (blending, depth, culling).
    pub render_state: RenderState,
    /// Material domain (should be [`MaterialDomain::Ui`]).
    pub domain: MaterialDomain,
    /// Whether to use bindless texture sampling.
    pub uses_bindless: bool,
}

impl Default for UiMaterialConfig {
    fn default() -> Self {
        Self {
            vertex_binding: VertexBinding::from(&VertexLayout::ui()),
            render_state: RenderState {
                depth_test: false,
                depth_write: false,
                cull_backfaces: false,
                alpha_blending: true,
            },
            domain: MaterialDomain::Ui,
            uses_bindless: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ui_material_config_defaults() {
        let config = UiMaterialConfig::default();

        assert_eq!(config.domain, MaterialDomain::Ui);
        assert!(config.uses_bindless);
        assert!(!config.render_state.depth_test);
        assert!(!config.render_state.depth_write);
        assert!(config.render_state.alpha_blending);
        assert!(!config.render_state.cull_backfaces);
    }
}
