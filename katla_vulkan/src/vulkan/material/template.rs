//! Material template and instance system.
//!
//! This module provides memory-efficient material instancing where multiple
//! material instances can share a single pipeline while having different
//! parameters and textures.

use super::{
    MaterialDescriptor, MaterialParameters, MaterialPipeline, MaterialValue, PbrTextureSet,
    ShaderReflection,
};
use crate::{Texture, VertexBinding};
use ash::vk;
use std::{cell::RefCell, collections::HashMap, rc::Rc};

/// Errors that can occur with material instances
#[derive(Debug)]
pub enum InstanceError {
    TemplateNotFound(String),
    ParameterNotFound(String),
    TypeMismatch(String),
    UpdateFailed(String),
}

impl std::fmt::Display for InstanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstanceError::TemplateNotFound(name) => {
                write!(f, "Material template '{}' not found", name)
            }
            InstanceError::ParameterNotFound(name) => {
                write!(f, "Parameter '{}' not found", name)
            }
            InstanceError::TypeMismatch(msg) => {
                write!(f, "Type mismatch: {}", msg)
            }
            InstanceError::UpdateFailed(msg) => {
                write!(f, "Failed to update material: {}", msg)
            }
        }
    }
}

impl std::error::Error for InstanceError {}

/// A material template that contains the pipeline and shared data
///
/// Multiple material instances can reference this template, making it
/// memory-efficient to have many materials with the same shader but
/// different parameters.
///
/// The pipeline is wrapped in Rc<RefCell<>> so that hot reload updates
/// are shared across all materials using this template.
///
/// The descriptor set layout is stored separately from the pipeline so
/// it can be preserved across hot reloads, ensuring that material instances'
/// descriptor sets remain valid.
pub struct MaterialTemplate {
    name: String,
    descriptor: MaterialDescriptor,
    pipeline: Rc<RefCell<MaterialPipeline>>,
    /// The descriptor set layout for set 0 (uniforms).
    /// This will be preserved across hot reloads.
    desc_layout: vk::DescriptorSetLayout,
    /// The descriptor set layout for set 1 (textures) in storage buffer mode.
    /// None for legacy mode (textures are in set 0).
    texture_set_layout: Option<vk::DescriptorSetLayout>,
    reflection: ShaderReflection,
    default_parameters: MaterialParameters,
}

impl MaterialTemplate {
    /// Create a new material template from a descriptor
    pub fn new(
        name: String,
        descriptor: MaterialDescriptor,
        reflection: ShaderReflection,
        pipeline: MaterialPipeline,
    ) -> Self {
        // Extract the descriptor set layout from the pipeline
        // This will be preserved across hot reloads
        let desc_layout = pipeline
            .desc_layout
            .expect("Pipeline created without descriptor set layout");

        // Extract texture set layout for storage buffer mode
        let texture_set_layout = pipeline.texture_set_layout;

        let default_parameters = MaterialParameters::new(descriptor.clone(), reflection.clone());

        Self {
            name,
            descriptor,
            pipeline: Rc::new(RefCell::new(pipeline)),
            desc_layout,
            texture_set_layout,
            reflection,
            default_parameters,
        }
    }

    /// Create a material template from a cached pipeline.
    ///
    /// This is used with MaterialPipelineCache where the pipeline
    /// is already wrapped in Rc<RefCell<>>.
    pub fn from_cached_pipeline(
        name: String,
        descriptor: MaterialDescriptor,
        reflection: ShaderReflection,
        cached_pipeline: Rc<RefCell<MaterialPipeline>>,
    ) -> Self {
        // Extract layouts from the cached pipeline
        let (desc_layout, texture_set_layout) = {
            let pipeline = cached_pipeline.borrow();
            let desc_layout = pipeline
                .desc_layout
                .expect("Pipeline created without descriptor set layout");
            (desc_layout, pipeline.texture_set_layout)
        };

        let default_parameters = MaterialParameters::new(descriptor.clone(), reflection.clone());

        Self {
            name,
            descriptor,
            pipeline: cached_pipeline,
            desc_layout,
            texture_set_layout,
            reflection,
            default_parameters,
        }
    }

    /// Get the template name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the descriptor
    pub fn descriptor(&self) -> &MaterialDescriptor {
        &self.descriptor
    }

    /// Get the pipeline
    pub fn pipeline(&self) -> Rc<RefCell<MaterialPipeline>> {
        Rc::clone(&self.pipeline)
    }

    /// Get the reflection data
    pub fn reflection(&self) -> &ShaderReflection {
        &self.reflection
    }

    /// Get the default parameters
    pub fn default_parameters(&self) -> &MaterialParameters {
        &self.default_parameters
    }

    /// Get the descriptor set layout for this template.
    ///
    /// This is preserved across hot reloads to ensure that material
    /// instances' descriptor sets remain valid.
    pub fn desc_layout(&self) -> vk::DescriptorSetLayout {
        self.desc_layout
    }

    /// Get the texture set layout for storage buffer mode.
    ///
    /// Returns None for legacy mode where textures are in set 0.
    pub fn texture_set_layout(&self) -> Option<vk::DescriptorSetLayout> {
        self.texture_set_layout
    }

    /// Check if this template uses storage buffer mode (modern rendering)
    pub fn is_storage(&self) -> bool {
        self.texture_set_layout.is_some()
    }

    /// Get the pipeline as a mutable RefCell borrow (for hot reload)
    ///
    /// This allows updating the pipeline in-place through the RefCell,
    /// which is shared with all materials using this template.
    pub fn pipeline_mut(&self) -> std::cell::RefMut<'_, MaterialPipeline> {
        self.pipeline.borrow_mut()
    }

    /// Update the pipeline (e.g., after hot reload)
    ///
    /// This affects all instances that reference this template
    pub fn update_pipeline(&mut self, pipeline: MaterialPipeline) {
        // Destroy the old pipeline
        if let Ok(mut old_pipeline) = self.pipeline.try_borrow_mut() {
            old_pipeline.destroy();
        }

        // Replace the pipeline inside the existing RefCell
        // This ensures all materials holding this Rc see the updated pipeline
        *self.pipeline.borrow_mut() = pipeline;
    }

    /// Get the descriptor layout and context for creating new uniform buffers
    ///
    /// This allows materials to create their own uniform buffers while
    /// sharing the pipeline from this template.
    ///
    /// Uses the stored descriptor set layout which is preserved across hot reloads.
    pub fn get_uniform_layout_info(
        &self,
    ) -> (
        vk::DescriptorSetLayout,
        Rc<crate::VulkanContext>,
        super::UniformLayout,
    ) {
        let pipeline = self.pipeline.borrow();
        (
            self.desc_layout,
            pipeline.context.clone(),
            pipeline.uniform.layout().clone(),
        )
    }

    /// Create a new uniform buffer for a material instance
    ///
    /// Each material should call this to get its own uniform buffer,
    /// avoiding conflicts when multiple materials share the same template.
    ///
    /// For storage mode (when texture_set_layout is Some), creates a minimal
    /// uniform handle without a buffer - texture info only.
    ///
    /// Uses separate texture and sampler bindings for WGSL shaders.
    pub fn create_uniform(&self) -> super::UniformHandle {
        let (desc_layout, context, _layout) = self.get_uniform_layout_info();

        // Always use storage mode for bindless - uniform data comes from StorageUniformManager
        // Textures are accessed via ObjectUniforms.texture_indices
        super::UniformHandle::new_storage(&context, &desc_layout)
    }

    /// Destroy the template's resources.
    ///
    /// This destroys the pipeline and the descriptor set layouts.
    /// This should be called when the template is no longer needed.
    pub fn destroy(&self, context: &Rc<crate::VulkanContext>) {
        // Destroy the pipeline
        if let Ok(mut pipeline) = self.pipeline.try_borrow_mut() {
            pipeline.destroy();
        }

        // Destroy the descriptor set layout
        unsafe {
            context
                .device
                .destroy_descriptor_set_layout(self.desc_layout, None);
        }

        // Destroy the texture set layout if present (storage mode)
        if let Some(texture_layout) = self.texture_set_layout {
            unsafe {
                context
                    .device
                    .destroy_descriptor_set_layout(texture_layout, None);
            }
        }
    }
}

//=============================================================================
// Unified Material Type
//=============================================================================

/// The unified material type for the application layer.
///
/// This type combines pipeline reference, textures, and bindless texture indices
/// into a single material that can be registered with the renderer.
///
/// # Creation
///
/// Materials can be created from:
/// - Template name: `Material::new("gltf_pbr_bindless")` (resolved during registration)
/// - Existing template: `Material::from_template(template)`
/// - Cached pipeline: `Material::from_cached_pipeline(pipeline, vertex_binding)`
///
/// # Builder Pattern
///
/// ```ignore
/// let material = Material::new("gltf_pbr_bindless")
///     .with_pbr_textures(pbr_textures, texture_refs)
///     .with_bindless_indices([0, 1, 2, 3], 4);
///
/// let handle = renderer.register_material(&mut material)?;
/// ```
#[derive(Clone)]
pub struct Material {
    /// Optional template reference (None until resolved)
    template: Option<Rc<MaterialTemplate>>,
    /// Template name for lazy resolution
    template_name: Option<String>,
    /// Cached pipeline for materials created from existing pipelines
    cached_pipeline: Option<Rc<RefCell<MaterialPipeline>>>,
    /// Instance-specific parameter values
    parameters: HashMap<String, MaterialValue>,
    /// Instance-specific textures by binding name
    textures: HashMap<String, Rc<Texture>>,
    /// Vertex binding for this material's geometry
    vertex_binding: Option<VertexBinding>,
    /// PBR texture set for full PBR materials
    pbr_textures: Option<PbrTextureSet>,
    /// Texture references to keep alive
    pbr_texture_refs: Option<Vec<Rc<Texture>>>,
    /// Bindless texture indices: [albedo, normal, metallic_roughness, ao]
    pub texture_indices: [u32; 4],
    /// Emission texture index for bindless
    pub emission_index: u32,
    /// Optional base color tint
    pub base_color: Option<[f32; 4]>,
}

impl Material {
    /// Create a new material by template name.
    ///
    /// The template will be resolved when the material is registered
    /// with the renderer via `register_material()`.
    pub fn new(template_name: impl Into<String>) -> Self {
        Self {
            template: None,
            template_name: Some(template_name.into()),
            cached_pipeline: None,
            parameters: HashMap::new(),
            textures: HashMap::new(),
            vertex_binding: None,
            pbr_textures: None,
            pbr_texture_refs: None,
            texture_indices: [0; 4],
            emission_index: 0,
            base_color: None,
        }
    }

    /// Create a material from an existing template.
    pub fn from_template(template: Rc<MaterialTemplate>) -> Self {
        Self {
            template: Some(template),
            template_name: None,
            cached_pipeline: None,
            parameters: HashMap::new(),
            textures: HashMap::new(),
            vertex_binding: None,
            pbr_textures: None,
            pbr_texture_refs: None,
            texture_indices: [0; 4],
            emission_index: 0,
            base_color: None,
        }
    }

    /// Create a material from a template reference.
    ///
    /// This is a convenience method that clones the Rc.
    pub fn from_template_ref(template: &Rc<MaterialTemplate>) -> Self {
        Self::from_template(Rc::clone(template))
    }

    /// Create a material from a template with optional texture.
    ///
    /// This is a convenience factory method for the common case of
    /// creating a material with just an albedo texture.
    pub fn from_template_with_optional_texture(
        template: &Rc<MaterialTemplate>,
        texture: Option<Rc<Texture>>,
        _color: Option<[f32; 4]>,
    ) -> Self {
        let mut material = Self::from_template(Rc::clone(template));
        if let Some(tex) = texture {
            material = material.with_texture("albedo", tex);
        }
        material
    }

    /// Create a material from a template with vertex binding.
    pub fn from_template_with_binding(
        template: Rc<MaterialTemplate>,
        vertex_binding: VertexBinding,
    ) -> Self {
        Self {
            template: Some(template),
            template_name: None,
            cached_pipeline: None,
            parameters: HashMap::new(),
            textures: HashMap::new(),
            vertex_binding: Some(vertex_binding),
            pbr_textures: None,
            pbr_texture_refs: None,
            texture_indices: [0; 4],
            emission_index: 0,
            base_color: None,
        }
    }

    /// Create a material from a cached pipeline.
    pub fn from_cached_pipeline(
        pipeline: Rc<RefCell<MaterialPipeline>>,
        vertex_binding: VertexBinding,
    ) -> Self {
        Self {
            template: None,
            template_name: None,
            cached_pipeline: Some(pipeline),
            parameters: HashMap::new(),
            textures: HashMap::new(),
            vertex_binding: Some(vertex_binding),
            pbr_textures: None,
            pbr_texture_refs: None,
            texture_indices: [0; 4],
            emission_index: 0,
            base_color: None,
        }
    }

    // === Builder Methods ===

    /// Set the vertex binding.
    pub fn with_vertex_binding(mut self, binding: VertexBinding) -> Self {
        self.vertex_binding = Some(binding);
        self
    }

    /// Set a single texture by binding name.
    pub fn with_texture(mut self, name: &str, texture: Rc<Texture>) -> Self {
        self.textures.insert(name.to_string(), texture);
        self
    }

    /// Set PBR textures with texture references.
    ///
    /// The `pbr_textures` contains the Vulkan handles for rendering,
    /// while `texture_refs` keeps the Texture objects alive.
    pub fn with_pbr_textures(
        mut self,
        pbr_textures: PbrTextureSet,
        texture_refs: Vec<Rc<Texture>>,
    ) -> Self {
        self.pbr_textures = Some(pbr_textures);
        self.pbr_texture_refs = Some(texture_refs);
        self
    }

    /// Set bindless texture indices.
    ///
    /// Indices: [albedo, normal, metallic_roughness, ao]
    pub fn with_bindless_indices(mut self, indices: [u32; 4], emission: u32) -> Self {
        self.texture_indices = indices;
        self.emission_index = emission;
        self
    }

    /// Set a parameter value.
    pub fn with_parameter(mut self, name: &str, value: MaterialValue) -> Self {
        self.parameters.insert(name.to_string(), value);
        self
    }

    /// Set the base color tint.
    pub fn with_base_color(mut self, color: [f32; 4]) -> Self {
        self.base_color = Some(color);
        self
    }

    // === Accessors ===

    /// Get the template name (for lazy resolution).
    pub fn template_name(&self) -> Option<&str> {
        self.template_name.as_deref()
    }

    /// Check if the template is resolved.
    pub fn is_resolved(&self) -> bool {
        self.template.is_some()
    }

    /// Get the template (if resolved).
    pub fn template(&self) -> Option<&MaterialTemplate> {
        self.template.as_deref()
    }

    /// Get the pipeline (if resolved).
    pub fn pipeline(&self) -> Option<Rc<RefCell<MaterialPipeline>>> {
        // First check for a cached pipeline (from from_cached_pipeline_*)
        if let Some(ref cached) = self.cached_pipeline {
            return Some(Rc::clone(cached));
        }
        // Then check template
        self.template.as_ref().map(|t| t.pipeline())
    }

    /// Get the vertex binding.
    pub fn vertex_binding(&self) -> Option<&VertexBinding> {
        self.vertex_binding.as_ref()
    }

    /// Get PBR textures.
    pub fn pbr_textures(&self) -> Option<&PbrTextureSet> {
        self.pbr_textures.as_ref()
    }

    /// Get PBR texture references.
    pub fn pbr_texture_refs(&self) -> Option<&[Rc<Texture>]> {
        self.pbr_texture_refs.as_deref()
    }

    /// Get a parameter value.
    pub fn get_parameter(&self, name: &str) -> Option<&MaterialValue> {
        self.parameters.get(name)
    }

    /// Get a texture by binding name.
    pub fn get_texture(&self, name: &str) -> Option<&Rc<Texture>> {
        self.textures.get(name)
    }

    /// Get all textures.
    pub fn textures(&self) -> &HashMap<String, Rc<Texture>> {
        &self.textures
    }

    // === Resolution ===

    /// Resolve the template from a registry.
    ///
    /// This is called by the renderer during registration.
    pub fn resolve(&mut self, registry: &super::registry::MaterialRegistry) -> bool {
        if self.template.is_some() {
            return true;
        }

        if let Some(name) = &self.template_name {
            if let Some(template) = registry.get_template(name) {
                self.template = Some(template.clone());
                return true;
            }
        }

        false
    }

    /// Set the resolved template directly.
    ///
    /// Used by the renderer after resolving from name.
    pub fn set_template(&mut self, template: Rc<MaterialTemplate>) {
        self.template = Some(template);
    }

    /// Set the cached pipeline directly (for non-template materials).
    pub fn set_cached_pipeline(&mut self, _pipeline: Rc<RefCell<MaterialPipeline>>) {
        // For materials without templates, we store the pipeline in a minimal wrapper
        // This is used for materials created from cached pipelines
        self.template = None;
        // Note: In a full implementation, we'd need to handle this case
    }

    // === Mutators ===

    /// Set a parameter value (mutable).
    pub fn set_parameter(&mut self, name: impl Into<String>, value: MaterialValue) {
        self.parameters.insert(name.into(), value);
    }

    /// Set a texture for a binding slot (mutable).
    pub fn set_texture(&mut self, slot: impl Into<String>, texture: Rc<Texture>) {
        self.textures.insert(slot.into(), texture);
    }

    /// Set PBR textures (mutable).
    pub fn set_pbr_textures(
        &mut self,
        pbr_textures: PbrTextureSet,
        texture_refs: Vec<Rc<Texture>>,
    ) {
        self.pbr_textures = Some(pbr_textures);
        self.pbr_texture_refs = Some(texture_refs);
    }

    /// Set bindless texture indices (mutable).
    pub fn set_bindless_indices(&mut self, indices: [u32; 4], emission: u32) {
        self.texture_indices = indices;
        self.emission_index = emission;
    }

    // === Utility ===

    /// Clone this material (shallow copy of template).
    pub fn clone_material(&self) -> Self {
        Self {
            template: self.template.clone(),
            template_name: self.template_name.clone(),
            cached_pipeline: self.cached_pipeline.clone(),
            parameters: self.parameters.clone(),
            textures: self.textures.clone(),
            vertex_binding: self.vertex_binding.clone(),
            pbr_textures: self.pbr_textures.clone(),
            pbr_texture_refs: self.pbr_texture_refs.clone(),
            texture_indices: self.texture_indices,
            emission_index: self.emission_index,
            base_color: self.base_color,
        }
    }

    /// Generate the uniform buffer data for this material.
    pub fn generate_uniform_buffer(&self) -> Result<Vec<u8>, InstanceError> {
        let template = self
            .template
            .as_ref()
            .ok_or_else(|| InstanceError::TemplateNotFound("No template set".to_string()))?;

        // Merge parameters with defaults
        let merged_params = self.get_all_parameters();

        // Get the uniform buffer layout
        let layout = template
            .reflection
            .get_uniforms_struct()
            .ok_or_else(|| InstanceError::ParameterNotFound("No uniforms struct".to_string()))?;

        let mut buffer = vec![0u8; layout.size];

        // Fill in the parameter values at their correct offsets
        for member in &layout.members {
            if let Some(value) = merged_params.get(&member.name) {
                let value_bytes = value.to_bytes();
                let offset = member.offset;

                if offset + value_bytes.len() <= buffer.len() {
                    buffer[offset..offset + value_bytes.len()].copy_from_slice(&value_bytes);
                }
            }
        }

        Ok(buffer)
    }

    /// Get all parameters merged with template defaults.
    pub fn get_all_parameters(&self) -> HashMap<String, MaterialValue> {
        let mut merged = HashMap::new();

        // Start with template defaults
        if let Some(template) = &self.template {
            for (name, value) in template.descriptor.parameters.iter() {
                merged.insert(name.clone(), value.clone());
            }
        }

        // Override with instance parameters
        for (name, value) in self.parameters.iter() {
            merged.insert(name.clone(), value.clone());
        }

        merged
    }
}

//=============================================================================
// Legacy MaterialInstance (Type Alias for Backward Compatibility)
//=============================================================================

/// Legacy type alias for backward compatibility.
///
/// New code should use `Material` directly.
pub type MaterialInstance = Material;

impl MaterialInstance {
    /// Legacy constructor: Create a new instance with a shared template.
    #[deprecated(note = "Use Material::from_template instead")]
    pub fn with_template(template: Rc<MaterialTemplate>) -> Self {
        Material::from_template(template)
    }

    /// Legacy constructor: Create a new instance with a shared template and custom parameters.
    #[deprecated(note = "Use Material::from_template and with_parameter instead")]
    pub fn with_template_and_params(
        template: Rc<MaterialTemplate>,
        params: HashMap<String, MaterialValue>,
    ) -> Self {
        let mut material = Material::from_template(template);
        material.parameters = params;
        material
    }
}

// ============================================================================
// Backward Compatibility Methods for katla_app
// ============================================================================

impl Material {
    /// Create a material from a template with a texture and color.
    ///
    /// This is a compatibility method for the old katla_app::rendering::Material API.
    ///
    /// Takes a reference to Rc<MaterialTemplate> for compatibility with how
    /// templates are returned from MaterialRegistry::get_template().
    pub fn from_template_compatible(
        template: &Rc<MaterialTemplate>,
        texture: Option<Rc<Texture>>,
        _color: Option<[f32; 4]>,
    ) -> Self {
        use crate::vulkan::vertexbinding::get_pbr_vertex_binding;

        let mut material = Material::from_template_with_binding(
            Rc::clone(template),
            get_pbr_vertex_binding(),
        );
        if let Some(tex) = texture {
            material = material.with_texture("albedo", tex);
        }
        material
    }

    /// Create a full PBR material from a template.
    pub fn from_template_pbr(
        template: &Rc<MaterialTemplate>,
        pbr_textures: PbrTextureSet,
        texture_refs: Vec<Rc<Texture>>,
        _color: Option<[f32; 4]>,
    ) -> Self {
        use crate::vulkan::vertexbinding::get_pbr_vertex_binding;

        Material::from_template_with_binding(Rc::clone(template), get_pbr_vertex_binding())
            .with_pbr_textures(pbr_textures, texture_refs)
    }

    /// Create a skinned material from a template.
    pub fn from_template_skinned(
        template: &Rc<MaterialTemplate>,
        texture: Option<Rc<Texture>>,
        _color: Option<[f32; 4]>,
    ) -> Self {
        use crate::vulkan::vertexbinding::get_skinned_vertex_binding;

        let mut material = Material::from_template_with_binding(
            Rc::clone(template),
            get_skinned_vertex_binding(),
        );
        if let Some(tex) = texture {
            material = material.with_texture("albedo", tex);
        }
        material
    }

    /// Create a skinned material with bindless texture indices.
    pub fn from_template_skinned_with_bindless(
        template: &Rc<MaterialTemplate>,
        texture: Option<Rc<Texture>>,
        _color: Option<[f32; 4]>,
        texture_indices: [u32; 4],
        emission_index: u32,
    ) -> Self {
        use crate::vulkan::vertexbinding::get_skinned_vertex_binding;

        let mut material = Material::from_template_with_binding(
            Rc::clone(template),
            get_skinned_vertex_binding(),
        )
        .with_bindless_indices(texture_indices, emission_index);

        if let Some(tex) = texture {
            material = material.with_texture("albedo", tex);
        }
        material
    }

    /// Create a full PBR material with bindless texture indices.
    pub fn from_template_pbr_bindless(
        template: &Rc<MaterialTemplate>,
        pbr_textures: PbrTextureSet,
        texture_refs: Vec<Rc<Texture>>,
        _color: Option<[f32; 4]>,
        texture_indices: [u32; 4],
        emission_index: u32,
    ) -> Self {
        use crate::vulkan::vertexbinding::get_pbr_vertex_binding;

        Material::from_template_with_binding(Rc::clone(template), get_pbr_vertex_binding())
            .with_pbr_textures(pbr_textures, texture_refs)
            .with_bindless_indices(texture_indices, emission_index)
    }

    /// Create a skinned PBR material with bindless texture indices.
    pub fn from_template_skinned_pbr_bindless(
        template: &Rc<MaterialTemplate>,
        pbr_textures: PbrTextureSet,
        texture_refs: Vec<Rc<Texture>>,
        _color: Option<[f32; 4]>,
        texture_indices: [u32; 4],
        emission_index: u32,
    ) -> Self {
        use crate::vulkan::vertexbinding::get_skinned_vertex_binding;

        Material::from_template_with_binding(Rc::clone(template), get_skinned_vertex_binding())
            .with_pbr_textures(pbr_textures, texture_refs)
            .with_bindless_indices(texture_indices, emission_index)
    }

    /// Create a material from a pipeline directly (non-template).
    pub fn from_pipeline(
        material_pipeline: MaterialPipeline,
        texture: Option<Rc<Texture>>,
        vertex_binding: VertexBinding,
        _color: Option<[f32; 4]>,
    ) -> Self {
        let mut material = Self {
            template: None,
            template_name: None,
            cached_pipeline: Some(Rc::new(RefCell::new(material_pipeline))),
            parameters: HashMap::new(),
            textures: HashMap::new(),
            vertex_binding: Some(vertex_binding),
            pbr_textures: None,
            pbr_texture_refs: None,
            texture_indices: [0; 4],
            emission_index: 0,
            base_color: None,
        };
        if let Some(tex) = texture {
            material = material.with_texture("albedo", tex);
        }
        material
    }

    /// Create a bindless material from a pipeline.
    pub fn from_pipeline_with_textures(
        material_pipeline: MaterialPipeline,
        texture: Option<Rc<Texture>>,
        vertex_binding: VertexBinding,
        _color: Option<[f32; 4]>,
        texture_indices: [u32; 4],
        emission_index: u32,
    ) -> Self {
        let mut material = Self {
            template: None,
            template_name: None,
            cached_pipeline: Some(Rc::new(RefCell::new(material_pipeline))),
            parameters: HashMap::new(),
            textures: HashMap::new(),
            vertex_binding: Some(vertex_binding),
            pbr_textures: None,
            pbr_texture_refs: None,
            texture_indices,
            emission_index,
            base_color: None,
        };
        if let Some(tex) = texture {
            material = material.with_texture("albedo", tex);
        }
        material
    }

    /// Create a bindless material from a cached pipeline.
    pub fn from_cached_pipeline_with_textures(
        material_pipeline: Rc<RefCell<MaterialPipeline>>,
        texture: Option<Rc<Texture>>,
        vertex_binding: VertexBinding,
        _color: Option<[f32; 4]>,
        texture_indices: [u32; 4],
        emission_index: u32,
    ) -> Self {
        let mut material = Self {
            template: None,
            template_name: None,
            cached_pipeline: Some(material_pipeline),
            parameters: HashMap::new(),
            textures: HashMap::new(),
            vertex_binding: Some(vertex_binding),
            pbr_textures: None,
            pbr_texture_refs: None,
            texture_indices,
            emission_index,
            base_color: None,
        };
        if let Some(tex) = texture {
            material = material.with_texture("albedo", tex);
        }
        material
    }

    /// Get the pipeline for rendering (if resolved).
    ///
    /// This returns the pipeline Rc for rendering operations.
    pub fn material_pipeline(&self) -> Option<Rc<RefCell<MaterialPipeline>>> {
        self.pipeline()
    }

    /// Get registration data for legacy compatibility.
    ///
    /// This is used by code that hasn't been updated to use the new
    /// `renderer.register_material()` API.
    #[allow(clippy::type_complexity)]
    pub fn get_registration_data(
        &self,
    ) -> (
        Option<Rc<RefCell<MaterialPipeline>>>,
        Option<Rc<Texture>>,
        Option<VertexBinding>,
        Option<PbrTextureSet>,
        Option<Vec<Rc<Texture>>>,
        [u32; 4],
        u32,
    ) {
        (
            self.pipeline(),
            self.textures.values().next().cloned(),
            self.vertex_binding.clone(),
            self.pbr_textures.clone(),
            self.pbr_texture_refs.clone(),
            self.texture_indices,
            self.emission_index,
        )
    }
}

// Note: Template tests require Vulkan context and are tested through
// the example programs and integration tests.
