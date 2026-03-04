//! High-level UI material builder.
//!
//! Provides a convenient builder for creating UI materials using the
//! low-level primitives from `katla_gfx::material::ui_shader` and
//! `katla_gfx::material::ui::UiMaterialConfig`.

use katla_gfx::material::{ui_shader, MaterialDefinition, ShaderSource, UiMaterialConfig};
use katla_gfx::MaterialDomain;

/// Builder for UI materials.
///
/// Creates a material configuration optimized for 2D immediate mode UI rendering.
///
/// # Example
///
/// ```ignore
/// use katla_ui::material::UiMaterialBuilder;
/// use katla_gfx::VulkanRenderer;
///
/// let material_def = UiMaterialBuilder::new().build();
/// let material_handle = renderer.register_material("ui", material_def);
/// ```
pub struct UiMaterialBuilder;

impl UiMaterialBuilder {
    /// Create a new UI material builder.
    pub fn new() -> Self {
        Self
    }

    /// Build the material definition.
    ///
    /// Returns a `MaterialDefinition` configured for UI rendering:
    /// - Orthographic projection via push constants
    /// - Bindless texture sampling
    /// - Alpha blending enabled
    /// - No depth testing
    /// - UI vertex layout (position, uv, color)
    pub fn build(self) -> MaterialDefinition {
        let config = UiMaterialConfig::default();

        MaterialDefinition::new()
            .with_shaders(
                ShaderSource::WgslString(ui_shader::VERTEX.to_string()),
                ShaderSource::WgslString(ui_shader::FRAGMENT.to_string()),
            )
            .with_vertex_binding(config.vertex_binding)
            .with_render_state(config.render_state)
            .with_domain(MaterialDomain::Ui)
            .with_bindless()
    }
}

impl Default for UiMaterialBuilder {
    fn default() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ui_material_builder() {
        let builder = UiMaterialBuilder::new();
        let def = builder.build();

        assert_eq!(def.domain(), MaterialDomain::Ui);
        assert!(def.uses_bindless());
        assert!(!def.render_state().depth_test);
        assert!(!def.render_state().depth_write);
        assert!(def.render_state().alpha_blending);
    }
}
