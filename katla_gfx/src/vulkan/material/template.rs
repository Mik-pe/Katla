//! Material template and instance system.
//!
//! This module provides memory-efficient material instancing where multiple
//! material instances can share a single pipeline while having different
//! parameters and textures.

use super::{
    MaterialDescriptor, MaterialParameters, MaterialPipeline, MaterialValue, PbrTextureSet,
    ShaderReflection,
};
use crate::handle::PipelineHandle;
use crate::vulkan::context::VulkanContext;
use crate::{Texture, VertexBinding};
use ash::vk;
use std::{collections::HashMap, rc::Rc};

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

/// A material template that references a pipeline via handle.
///
/// Multiple material instances can reference this template, making it
/// memory-efficient to have many materials with the same shader but
/// different parameters.
///
/// The pipeline is referenced by handle from the MaterialPipelineCache,
/// which owns all pipelines centrally.
pub struct MaterialTemplate {
    name: String,
    descriptor: MaterialDescriptor,
    pipeline: PipelineHandle,
    desc_layout: vk::DescriptorSetLayout,
    texture_set_layout: Option<vk::DescriptorSetLayout>,
    skeleton_set_layout: Option<vk::DescriptorSetLayout>,
    is_bindless: bool,
    reflection: ShaderReflection,
    default_parameters: MaterialParameters,
}

impl MaterialTemplate {
    /// Create a new material template from a descriptor and pipeline handle.
    pub fn new(
        name: String,
        descriptor: MaterialDescriptor,
        reflection: ShaderReflection,
        pipeline: MaterialPipeline,
    ) -> Self {
        let desc_layout = pipeline
            .desc_layout
            .expect("Pipeline created without descriptor set layout");
        let texture_set_layout = pipeline.texture_set_layout;
        let skeleton_set_layout = pipeline.skeleton_set_layout;
        let is_bindless = pipeline.is_bindless;

        let default_parameters = MaterialParameters::new(descriptor.clone(), reflection.clone());

        Self {
            name,
            descriptor,
            pipeline: PipelineHandle::NONE,
            desc_layout,
            texture_set_layout,
            skeleton_set_layout,
            is_bindless,
            reflection,
            default_parameters,
        }
    }

    /// Create a material template from a cached pipeline handle.
    ///
    /// This is used with MaterialPipelineCache where the pipeline
    /// is stored centrally and referenced by handle.
    pub fn from_cached_pipeline(
        name: String,
        descriptor: MaterialDescriptor,
        reflection: ShaderReflection,
        pipeline_handle: PipelineHandle,
    ) -> Self {
        let default_parameters = MaterialParameters::new(descriptor.clone(), reflection.clone());

        Self {
            name,
            descriptor,
            pipeline: pipeline_handle,
            desc_layout: vk::DescriptorSetLayout::null(),
            texture_set_layout: None,
            skeleton_set_layout: None,
            is_bindless: false,
            reflection,
            default_parameters,
        }
    }

    /// Create a material template from a cached pipeline with layout info.
    pub fn from_cached_pipeline_with_layouts(
        name: String,
        descriptor: MaterialDescriptor,
        reflection: ShaderReflection,
        pipeline_handle: PipelineHandle,
        desc_layout: vk::DescriptorSetLayout,
        texture_set_layout: Option<vk::DescriptorSetLayout>,
        skeleton_set_layout: Option<vk::DescriptorSetLayout>,
        is_bindless: bool,
    ) -> Self {
        let default_parameters = MaterialParameters::new(descriptor.clone(), reflection.clone());

        Self {
            name,
            descriptor,
            pipeline: pipeline_handle,
            desc_layout,
            texture_set_layout,
            skeleton_set_layout,
            is_bindless,
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

    /// Get the pipeline handle
    pub fn pipeline(&self) -> PipelineHandle {
        self.pipeline
    }

    /// Get the reflection data
    pub fn reflection(&self) -> &ShaderReflection {
        &self.reflection
    }

    /// Get the default parameters
    pub fn default_parameters(&self) -> &MaterialParameters {
        &self.default_parameters
    }

    /// Check if this template uses bindless textures.
    pub fn is_bindless(&self) -> bool {
        self.is_bindless
    }

    /// Check if this template uses storage buffer mode (modern rendering)
    pub fn is_storage(&self) -> bool {
        self.texture_set_layout.is_some()
    }

    /// Destroy the template's resources.
    pub fn destroy(&self, context: &Rc<VulkanContext>) {
        unsafe {
            context
                .device
                .destroy_descriptor_set_layout(self.desc_layout, None);
        }

        if let Some(texture_layout) = self.texture_set_layout {
            unsafe {
                context
                    .device
                    .destroy_descriptor_set_layout(texture_layout, None);
            }
        }

        if let Some(skeleton_layout) = self.skeleton_set_layout {
            unsafe {
                context
                    .device
                    .destroy_descriptor_set_layout(skeleton_layout, None);
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
/// - Pipeline handle: `Material::from_pipeline_handle(pipeline, vertex_binding)`
///
/// # Builder Pattern
///
/// ```ignore
/// let material = Material::new("gltf_pbr_bindless")
///     .with_pbr_textures(pbr_textures)
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
    /// Pipeline handle for materials created with a specific pipeline
    pipeline: PipelineHandle,
    /// Whether this material uses bindless textures
    is_bindless: bool,
    /// Instance-specific parameter values
    parameters: HashMap<String, MaterialValue>,
    /// Instance-specific textures by binding name
    textures: HashMap<String, Rc<Texture>>,
    /// Vertex binding for this material's geometry
    vertex_binding: Option<VertexBinding>,
    /// PBR texture set for full PBR materials (uses TextureHandle)
    pbr_textures: Option<PbrTextureSet>,
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
            pipeline: PipelineHandle::NONE,
            is_bindless: false,
            parameters: HashMap::new(),
            textures: HashMap::new(),
            vertex_binding: None,
            pbr_textures: None,
            texture_indices: [0; 4],
            emission_index: 0,
            base_color: None,
        }
    }

    /// Create a material from an existing template.
    pub fn from_template(template: Rc<MaterialTemplate>) -> Self {
        let is_bindless = template.is_bindless();
        Self {
            template: Some(template),
            template_name: None,
            pipeline: PipelineHandle::NONE,
            is_bindless,
            parameters: HashMap::new(),
            textures: HashMap::new(),
            vertex_binding: None,
            pbr_textures: None,
            texture_indices: [0; 4],
            emission_index: 0,
            base_color: None,
        }
    }

    /// Create a material from a template reference.
    pub fn from_template_ref(template: &Rc<MaterialTemplate>) -> Self {
        Self::from_template(Rc::clone(template))
    }

    /// Create a material from a template with optional texture.
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
        let is_bindless = template.is_bindless();
        Self {
            template: Some(template),
            template_name: None,
            pipeline: PipelineHandle::NONE,
            is_bindless,
            parameters: HashMap::new(),
            textures: HashMap::new(),
            vertex_binding: Some(vertex_binding),
            pbr_textures: None,
            texture_indices: [0; 4],
            emission_index: 0,
            base_color: None,
        }
    }

    /// Create a material from a pipeline handle.
    pub fn from_pipeline_handle(
        pipeline: PipelineHandle,
        vertex_binding: VertexBinding,
        is_bindless: bool,
    ) -> Self {
        Self {
            template: None,
            template_name: None,
            pipeline,
            is_bindless,
            parameters: HashMap::new(),
            textures: HashMap::new(),
            vertex_binding: Some(vertex_binding),
            pbr_textures: None,
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

    /// Set PBR textures (uses TextureHandle, no refs needed).
    pub fn with_pbr_textures(mut self, pbr_textures: PbrTextureSet) -> Self {
        self.pbr_textures = Some(pbr_textures);
        self
    }

    /// Set bindless texture indices.
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

    /// Get the pipeline handle.
    pub fn pipeline(&self) -> PipelineHandle {
        if let Some(template) = &self.template {
            template.pipeline()
        } else {
            self.pipeline
        }
    }

    /// Check if this material uses bindless textures.
    pub fn is_bindless(&self) -> bool {
        self.is_bindless
    }

    /// Get the vertex binding.
    pub fn vertex_binding(&self) -> Option<&VertexBinding> {
        self.vertex_binding.as_ref()
    }

    /// Get PBR textures.
    pub fn pbr_textures(&self) -> Option<&PbrTextureSet> {
        self.pbr_textures.as_ref()
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
    pub fn resolve(&mut self, registry: &super::registry::MaterialRegistry) -> bool {
        if self.template.is_some() {
            return true;
        }

        if let Some(name) = &self.template_name
            && let Some(template) = registry.get_template(name)
        {
            self.is_bindless = template.is_bindless();
            self.template = Some(template.clone());
            return true;
        }

        false
    }

    /// Set the resolved template directly.
    pub fn set_template(&mut self, template: Rc<MaterialTemplate>) {
        self.is_bindless = template.is_bindless();
        self.template = Some(template);
    }

    /// Set the pipeline handle directly.
    pub fn set_pipeline(&mut self, pipeline: PipelineHandle, is_bindless: bool) {
        self.pipeline = pipeline;
        self.is_bindless = is_bindless;
        self.template = None;
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

    /// Set PBR textures (mutable, uses TextureHandle).
    pub fn set_pbr_textures(&mut self, pbr_textures: PbrTextureSet) {
        self.pbr_textures = Some(pbr_textures);
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
            pipeline: self.pipeline,
            is_bindless: self.is_bindless,
            parameters: self.parameters.clone(),
            textures: self.textures.clone(),
            vertex_binding: self.vertex_binding.clone(),
            pbr_textures: self.pbr_textures.clone(),
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

        let merged_params = self.get_all_parameters();

        let layout = template
            .reflection
            .get_uniforms_struct()
            .ok_or_else(|| InstanceError::ParameterNotFound("No uniforms struct".to_string()))?;

        let mut buffer = vec![0u8; layout.size];

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

        if let Some(template) = &self.template {
            for (name, value) in template.descriptor.parameters.iter() {
                merged.insert(name.clone(), value.clone());
            }
        }

        for (name, value) in self.parameters.iter() {
            merged.insert(name.clone(), value.clone());
        }

        merged
    }
}
