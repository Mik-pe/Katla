//! Material template for pipeline configuration.
//!
//! A MaterialTemplate defines the GPU pipeline configuration (shaders, descriptor layouts,
//! push constants) that can be shared across multiple MaterialInstances.

use crate::handle::PipelineHandle;
use crate::texture::ImageFormat;
use crate::vulkan::vertexbinding::VertexBinding;

use super::{RenderState, ShaderSource};

use super::MaterialDomain;

/// Push constant range definition.
#[derive(Clone, Debug)]
pub struct PushConstantRange {
    /// Shader stages that can access this range.
    pub stages: PushConstantStages,
    /// Offset in bytes from the start of the push constant block.
    pub offset: u32,
    /// Size in bytes of the push constant range.
    pub size: u32,
}

impl PushConstantRange {
    /// Create a new push constant range.
    pub fn new(stages: PushConstantStages, offset: u32, size: u32) -> Self {
        Self {
            stages,
            offset,
            size,
        }
    }
}

/// Shader stages that can access push constants.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PushConstantStages {
    /// Vertex shader access.
    pub vertex: bool,
    /// Fragment shader access.
    pub fragment: bool,
    /// Compute shader access.
    pub compute: bool,
}

impl PushConstantStages {
    /// Create stages with only vertex shader access.
    pub fn vertex_only() -> Self {
        Self {
            vertex: true,
            fragment: false,
            compute: false,
        }
    }

    /// Create stages with only fragment shader access.
    pub fn fragment_only() -> Self {
        Self {
            vertex: false,
            fragment: true,
            compute: false,
        }
    }

    /// Create stages with vertex and fragment shader access.
    pub fn vertex_fragment() -> Self {
        Self {
            vertex: true,
            fragment: true,
            compute: false,
        }
    }
}

/// Shader set containing vertex and fragment shaders.
#[derive(Clone, Debug, Default)]
pub struct ShaderSet {
    vertex_shader: Option<ShaderSource>,
    fragment_shader: Option<ShaderSource>,
}

impl ShaderSet {
    /// Create an empty shader set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a shader set with both vertex and fragment shaders.
    pub fn from_shaders(vertex: ShaderSource, fragment: ShaderSource) -> Self {
        Self {
            vertex_shader: Some(vertex),
            fragment_shader: Some(fragment),
        }
    }

    /// Get the vertex shader source.
    pub fn vertex_shader(&self) -> Option<&ShaderSource> {
        self.vertex_shader.as_ref()
    }

    /// Get the fragment shader source.
    pub fn fragment_shader(&self) -> Option<&ShaderSource> {
        self.fragment_shader.as_ref()
    }
}

/// Configuration for creating a MaterialTemplate.
///
/// Builder pattern for constructing material templates with
/// shaders, vertex bindings, render state, and descriptor layouts.
///
/// # Example
///
/// ```ignore
/// use katla_gfx::{MaterialDefinition, ShaderSource, RenderState, MaterialDomain};
///
/// let config = MaterialDefinition::new()
///     .with_shaders(
///         ShaderSource::WgslFile("vertex.wgsl".into()),
///         ShaderSource::WgslFile("fragment.wgsl".into()),
///     )
///     .with_render_state(RenderState::default())
///     .with_domain(MaterialDomain::Surface);
///
/// let template = config.build();
/// ```
#[derive(Clone, Debug)]
pub struct MaterialDefinition {
    vertex_shader: Option<ShaderSource>,
    fragment_shader: Option<ShaderSource>,
    vertex_binding: Option<VertexBinding>,
    render_state: RenderState,
    descriptor_layouts: Vec<DescriptorSetLayout>,
    push_constant_ranges: Vec<PushConstantRange>,
    domain: MaterialDomain,
    /// Uses skeletal animation (adds skeleton descriptor set)
    uses_skeleton: bool,
    /// Uses bindless textures (textures provided externally)
    uses_bindless: bool,
    /// Color attachment format for pipeline creation
    color_format: ImageFormat,
    /// Depth attachment format for pipeline creation
    depth_format: ImageFormat,
}

impl Default for MaterialDefinition {
    fn default() -> Self {
        Self {
            vertex_shader: None,
            fragment_shader: None,
            vertex_binding: None,
            render_state: RenderState::default(),
            descriptor_layouts: Vec::new(),
            push_constant_ranges: Vec::new(),
            domain: MaterialDomain::Surface,
            uses_skeleton: false,
            uses_bindless: false,
            color_format: ImageFormat::R16G16B16A16Sfloat,
            depth_format: ImageFormat::D32SfloatS8Uint,
        }
    }
}

impl MaterialDefinition {
    /// Create a new template config with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the vertex shader source.
    pub fn with_vertex_shader(mut self, shader: ShaderSource) -> Self {
        self.vertex_shader = Some(shader);
        self
    }

    /// Set the fragment shader source.
    pub fn with_fragment_shader(mut self, shader: ShaderSource) -> Self {
        self.fragment_shader = Some(shader);
        self
    }

    /// Set both vertex and fragment shaders at once.
    pub fn with_shaders(mut self, vertex: ShaderSource, fragment: ShaderSource) -> Self {
        self.vertex_shader = Some(vertex);
        self.fragment_shader = Some(fragment);
        self
    }

    /// Set the vertex binding description.
    pub fn with_vertex_binding(mut self, binding: VertexBinding) -> Self {
        self.vertex_binding = Some(binding);
        self
    }

    /// Set the render state configuration.
    pub fn with_render_state(mut self, state: RenderState) -> Self {
        self.render_state = state;
        self
    }

    /// Add a descriptor set layout.
    pub fn add_descriptor_layout(mut self, layout: DescriptorSetLayout) -> Self {
        self.descriptor_layouts.push(layout);
        self
    }

    /// Add multiple descriptor set layouts.
    pub fn with_descriptor_layouts(mut self, layouts: Vec<DescriptorSetLayout>) -> Self {
        self.descriptor_layouts = layouts;
        self
    }

    /// Add a push constant range.
    pub fn add_push_constant_range(mut self, range: PushConstantRange) -> Self {
        self.push_constant_ranges.push(range);
        self
    }

    /// Set the material domain.
    pub fn with_domain(mut self, domain: MaterialDomain) -> Self {
        self.domain = domain;
        self
    }

    /// Enable skeletal animation.
    pub fn with_skeleton(mut self) -> Self {
        self.uses_skeleton = true;
        self
    }

    /// Enable bindless textures.
    pub fn with_bindless(mut self) -> Self {
        self.uses_bindless = true;
        self
    }

    /// Set the color attachment format.
    pub fn with_color_format(mut self, format: ImageFormat) -> Self {
        self.color_format = format;
        self
    }

    /// Set the depth attachment format.
    pub fn with_depth_format(mut self, format: ImageFormat) -> Self {
        self.depth_format = format;
        self
    }

    // === Accessors ===

    /// Get the vertex shader source.
    pub fn vertex_shader(&self) -> Option<&ShaderSource> {
        self.vertex_shader.as_ref()
    }

    /// Get the fragment shader source.
    pub fn fragment_shader(&self) -> Option<&ShaderSource> {
        self.fragment_shader.as_ref()
    }

    /// Get the vertex binding.
    pub fn vertex_binding(&self) -> Option<&VertexBinding> {
        self.vertex_binding.as_ref()
    }

    /// Get the render state.
    pub fn render_state(&self) -> &RenderState {
        &self.render_state
    }

    /// Get the descriptor layouts.
    pub fn descriptor_layouts(&self) -> &[DescriptorSetLayout] {
        &self.descriptor_layouts
    }

    /// Get the material domain.
    pub fn domain(&self) -> MaterialDomain {
        self.domain
    }

    /// Check if this config uses skeletal animation.
    pub fn uses_skeleton(&self) -> bool {
        self.uses_skeleton
    }

    /// Check if this config uses bindless textures.
    pub fn uses_bindless(&self) -> bool {
        self.uses_bindless
    }

    /// Get the color attachment format.
    pub fn color_format(&self) -> ImageFormat {
        self.color_format
    }

    /// Get the depth attachment format.
    pub fn depth_format(&self) -> ImageFormat {
        self.depth_format
    }

    /// Build the MaterialTemplate from this configuration.
    pub fn build(self) -> MaterialTemplate {
        let shaders = ShaderSet::from_shaders(
            self.vertex_shader
                .unwrap_or(ShaderSource::WgslString(String::new())),
            self.fragment_shader
                .unwrap_or(ShaderSource::WgslString(String::new())),
        );

        MaterialTemplate {
            descriptor_set_layouts: self.descriptor_layouts,
            push_constant_ranges: self.push_constant_ranges,
            shaders,
            vertex_binding: self.vertex_binding,
            render_state: self.render_state,
            domain: self.domain,
            uses_skeleton: self.uses_skeleton,
            uses_bindless: self.uses_bindless,
            color_format: self.color_format,
            depth_format: self.depth_format,
            pipeline: PipelineHandle::NONE,
        }
    }
}

/// Wrapper for descriptor set layout.
#[derive(Clone, Debug)]
pub struct DescriptorSetLayout {
    set_index: u32,
}

impl DescriptorSetLayout {
    /// Get the set index for this layout.
    pub fn set_index(&self) -> u32 {
        self.set_index
    }
}

/// Material template that defines a pipeline configuration.
///
/// A template contains all the information needed to create a GPU pipeline:
/// - Descriptor set layouts for resource binding
/// - Push constant ranges for fast uniform updates
/// - Shader configuration
/// - Vertex binding description
/// - Render state configuration
/// - A reference to the compiled pipeline
///
/// Multiple MaterialInstances can share a single MaterialTemplate,
/// making it memory-efficient to have many materials with the same
/// shader but different textures/buffers.
pub struct MaterialTemplate {
    descriptor_set_layouts: Vec<DescriptorSetLayout>,
    push_constant_ranges: Vec<PushConstantRange>,
    shaders: ShaderSet,
    vertex_binding: Option<VertexBinding>,
    render_state: RenderState,
    domain: MaterialDomain,
    uses_skeleton: bool,
    uses_bindless: bool,
    color_format: ImageFormat,
    depth_format: ImageFormat,
    pipeline: PipelineHandle,
}

impl MaterialTemplate {
    /// Create a MaterialTemplate from configuration.
    pub fn new(config: MaterialDefinition) -> Self {
        config.build()
    }

    /// Get a descriptor set layout by set index.
    pub fn descriptor_set_layout(&self, set: u32) -> Option<&DescriptorSetLayout> {
        self.descriptor_set_layouts
            .iter()
            .find(|layout| layout.set_index() == set)
    }

    /// Get all descriptor set layouts.
    pub fn descriptor_set_layouts(&self) -> &[DescriptorSetLayout] {
        &self.descriptor_set_layouts
    }

    /// Get the push constant ranges.
    pub fn push_constant_ranges(&self) -> &[PushConstantRange] {
        &self.push_constant_ranges
    }

    /// Get the vertex binding.
    pub fn vertex_binding(&self) -> Option<&VertexBinding> {
        self.vertex_binding.as_ref()
    }

    /// Get the render state.
    pub fn render_state(&self) -> &RenderState {
        &self.render_state
    }

    /// Get the material domain.
    pub fn domain(&self) -> MaterialDomain {
        self.domain
    }

    /// Returns true if this template uses skeletal animation.
    pub fn uses_skeleton(&self) -> bool {
        self.uses_skeleton
    }

    /// Returns true if this template uses bindless textures.
    pub fn uses_bindless(&self) -> bool {
        self.uses_bindless
    }

    /// Get the color attachment format.
    pub fn color_format(&self) -> ImageFormat {
        self.color_format
    }

    /// Get the depth attachment format.
    pub fn depth_format(&self) -> ImageFormat {
        self.depth_format
    }

    /// Get the pipeline handle.
    pub fn pipeline(&self) -> PipelineHandle {
        self.pipeline
    }

    /// Get the shader set.
    pub fn shaders(&self) -> &ShaderSet {
        &self.shaders
    }
}

//=============================================================================
// Tests
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vulkan::vertexbinding::VertexFormat;

    fn create_test_vertex_binding() -> VertexBinding {
        VertexBinding {
            formats: vec![VertexFormat::RGB32f, VertexFormat::RG32f],
        }
    }

    #[test]
    fn test_push_constant_stages_vertex_only() {
        let stages = PushConstantStages::vertex_only();
        assert!(stages.vertex);
        assert!(!stages.fragment);
        assert!(!stages.compute);
    }

    #[test]
    fn test_push_constant_stages_fragment_only() {
        let stages = PushConstantStages::fragment_only();
        assert!(!stages.vertex);
        assert!(stages.fragment);
        assert!(!stages.compute);
    }

    #[test]
    fn test_push_constant_stages_vertex_fragment() {
        let stages = PushConstantStages::vertex_fragment();
        assert!(stages.vertex);
        assert!(stages.fragment);
        assert!(!stages.compute);
    }

    #[test]
    fn test_push_constant_range_creation() {
        let range = PushConstantRange::new(PushConstantStages::vertex_fragment(), 0, 64);
        assert_eq!(range.offset, 0);
        assert_eq!(range.size, 64);
        assert!(range.stages.vertex);
        assert!(range.stages.fragment);
    }

    #[test]
    fn test_shader_set_new() {
        let shaders = ShaderSet::new();
        assert!(shaders.vertex_shader().is_none());
        assert!(shaders.fragment_shader().is_none());
    }

    #[test]
    fn test_shader_set_from_shaders() {
        let vertex = ShaderSource::WgslString("vertex code".to_string());
        let fragment = ShaderSource::WgslString("fragment code".to_string());
        let shaders = ShaderSet::from_shaders(vertex, fragment);
        assert!(shaders.vertex_shader().is_some());
        assert!(shaders.fragment_shader().is_some());
    }

    mod material_template_config {
        use super::*;

        #[test]
        fn test_new() {
            let config = MaterialDefinition::new();
            assert!(config.vertex_shader.is_none());
            assert!(config.fragment_shader.is_none());
            assert!(config.vertex_binding.is_none());
            assert!(config.descriptor_layouts.is_empty());
            assert!(config.push_constant_ranges.is_empty());
            assert_eq!(config.domain, MaterialDomain::Surface);
        }

        #[test]
        fn test_with_vertex_shader() {
            let config = MaterialDefinition::new()
                .with_vertex_shader(ShaderSource::WgslString("vertex".to_string()));
            assert!(config.vertex_shader.is_some());
            assert!(config.fragment_shader.is_none());
        }

        #[test]
        fn test_with_fragment_shader() {
            let config = MaterialDefinition::new()
                .with_fragment_shader(ShaderSource::WgslString("fragment".to_string()));
            assert!(config.vertex_shader.is_none());
            assert!(config.fragment_shader.is_some());
        }

        #[test]
        fn test_with_shaders() {
            let config = MaterialDefinition::new().with_shaders(
                ShaderSource::WgslString("vertex".to_string()),
                ShaderSource::WgslString("fragment".to_string()),
            );
            assert!(config.vertex_shader.is_some());
            assert!(config.fragment_shader.is_some());
        }

        #[test]
        fn test_with_vertex_binding() {
            let binding = create_test_vertex_binding();
            let config = MaterialDefinition::new().with_vertex_binding(binding);
            assert!(config.vertex_binding.is_some());
        }

        #[test]
        fn test_with_render_state() {
            let state = RenderState {
                depth_test: false,
                depth_write: false,
                cull_backfaces: true,
                alpha_blending: true,
            };
            let config = MaterialDefinition::new().with_render_state(state.clone());
            assert_eq!(config.render_state(), &state);
        }

        #[test]
        fn test_with_domain() {
            let config = MaterialDefinition::new().with_domain(MaterialDomain::Ui);
            assert_eq!(config.domain(), MaterialDomain::Ui);
        }

        #[test]
        fn test_chained_builders() {
            let config = MaterialDefinition::new()
                .with_shaders(
                    ShaderSource::WgslString("vertex".to_string()),
                    ShaderSource::WgslString("fragment".to_string()),
                )
                .with_vertex_binding(create_test_vertex_binding())
                .with_render_state(RenderState::default())
                .with_domain(MaterialDomain::PostProcess);

            assert!(config.vertex_shader().is_some());
            assert!(config.fragment_shader().is_some());
            assert!(config.vertex_binding().is_some());
            assert_eq!(config.domain(), MaterialDomain::PostProcess);
        }
    }

    mod material_template {
        use super::*;

        #[test]
        fn test_new_from_config() {
            let config = MaterialDefinition::new()
                .with_shaders(
                    ShaderSource::WgslString("vertex".to_string()),
                    ShaderSource::WgslString("fragment".to_string()),
                )
                .with_vertex_binding(create_test_vertex_binding())
                .with_domain(MaterialDomain::Surface);

            let template = MaterialTemplate::new(config);

            assert!(template.descriptor_set_layouts().is_empty());
            assert!(template.push_constant_ranges().is_empty());
            assert!(template.vertex_binding().is_some());
            assert_eq!(template.domain(), MaterialDomain::Surface);
            assert!(template.pipeline().is_none());
        }

        #[test]
        fn test_build_from_config() {
            let template = MaterialDefinition::new()
                .with_shaders(
                    ShaderSource::WgslString("vertex".to_string()),
                    ShaderSource::WgslString("fragment".to_string()),
                )
                .with_render_state(RenderState {
                    depth_test: true,
                    depth_write: true,
                    cull_backfaces: false,
                    alpha_blending: true,
                })
                .build();

            assert_eq!(template.render_state().depth_test, true);
            assert_eq!(template.render_state().alpha_blending, true);
        }

        #[test]
        fn test_empty_config_build() {
            let template = MaterialDefinition::new().build();

            // Empty config should still produce a valid template
            assert!(template.descriptor_set_layouts().is_empty());
            assert!(template.push_constant_ranges().is_empty());
            assert!(template.vertex_binding().is_none());
            assert!(template.pipeline().is_none());
        }
    }
}
