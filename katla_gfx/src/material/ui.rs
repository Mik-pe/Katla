//! Low-level UI material primitives for immediate mode UI rendering.
//!
//! This module provides:
//! - Shader source code for UI rendering (`ui_shader` module)
//! - Configuration type for UI material setup (`UiMaterialConfig`)
//!
//! For a high-level convenience API, see `katla_ui::material::UiMaterialBuilder`.

use crate::vertex::VertexLayout;
use crate::vulkan::vertexbinding::VertexBinding;

use super::MaterialDomain;
use super::RenderState;

/// UI shader source code.
///
/// Exports vertex and fragment shader source strings for UI rendering.
pub mod ui_shader {
    /// UI vertex shader source (WGSL).
    ///
    /// NOTE: This is a legacy shader string that uses push constants.
    /// The actual UI shader (resources/shaders/ui/ui.wgsl) uses uniform buffers.
    /// This embedded string is kept for reference but is not used by the current pipeline.
    /// Uses an orthographic projection matrix passed via push constants.
    /// Transforms screen coordinates (pixels) to clip space.
    pub const VERTEX: &str = r#"
struct PushConstants {
    scale: vec2f,
    translate: vec2f,
}

var<push_constant> pc: PushConstants;

struct VertexInput {
    @location(0) position: vec2f,
    @location(1) uv: vec2f,
    @location(2) color: vec4f,
}

struct VertexOutput {
    @builtin(position) clip_pos: vec4f,
    @location(0) uv: vec2f,
    @location(1) color: vec4f,
}

@vertex
fn main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.clip_pos = vec4f(input.position * pc.scale + pc.translate, 0.0, 1.0);
    output.uv = input.uv;
    output.color = input.color;
    return output;
}
"#;

    /// UI fragment shader source (WGSL).
    ///
    /// NOTE: This is a legacy shader string that uses push constants.
    /// The actual UI shader (resources/shaders/ui/ui.wgsl) uses uniform buffers.
    /// This embedded string is kept for reference but is not used by the current pipeline.
    /// Samples from bindless texture array using texture index.
    /// Multiplies texture color with vertex color for tinting.
    pub const FRAGMENT: &str = r#"
struct PushConstants {
    scale: vec2f,
    translate: vec2f,
    texture_index: u32,
}

var<push_constant> pc: PushConstants;

@group(0) @binding(0) var textures: binding_array<texture_2d<f32>>;
@group(0) @binding(1) var samplers: binding_array<sampler>;

struct FragmentInput {
    @location(0) uv: vec2f,
    @location(1) color: vec4f,
}

@fragment
fn main(input: FragmentInput) -> @location(0) vec4f {
    let tex_color = textureSample(textures[pc.texture_index], samplers[0], input.uv);
    return input.color * tex_color;
}
"#;
}

/// Configuration for UI material creation.
///
/// Provides the low-level settings needed to create a UI material.
/// Use this with [`MaterialDefinition`] to build a custom UI material,
/// or use `katla_ui::material::UiMaterialBuilder` for a convenient default setup.
///
/// [`MaterialDefinition`]: super::MaterialDefinition
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

    #[test]
    fn test_ui_shader_source_not_empty() {
        assert!(!ui_shader::VERTEX.is_empty());
        assert!(!ui_shader::FRAGMENT.is_empty());
    }
}
