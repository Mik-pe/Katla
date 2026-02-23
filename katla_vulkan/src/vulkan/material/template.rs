//! Material template and instance system.
//!
//! This module provides memory-efficient material instancing where multiple
//! material instances can share a single pipeline while having different
//! parameters and textures.

use super::{
    MaterialDescriptor, MaterialParameters, MaterialPipeline, MaterialValue,
    ShaderReflection,
};
use crate::Texture;
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

/// An instance of a material template with instance-specific parameters
///
/// Each instance can have different parameter values and textures while
/// sharing the same pipeline with other instances.
pub struct MaterialInstance {
    template: Rc<MaterialTemplate>,
    parameters: HashMap<String, MaterialValue>,
    textures: HashMap<String, Rc<Texture>>,
}

impl MaterialInstance {
    /// Create a new instance with a shared template
    pub fn with_template(template: Rc<MaterialTemplate>) -> Self {
        Self {
            template,
            parameters: HashMap::new(),
            textures: HashMap::new(),
        }
    }

    /// Create a new instance with a shared template and custom parameters
    pub fn with_template_and_params(
        template: Rc<MaterialTemplate>,
        params: HashMap<String, MaterialValue>,
    ) -> Self {
        Self {
            template,
            parameters: params,
            textures: HashMap::new(),
        }
    }

    /// Get the template this instance references
    pub fn template(&self) -> &MaterialTemplate {
        &self.template
    }

    /// Get the pipeline (shared with template)
    pub fn pipeline(&self) -> Rc<RefCell<MaterialPipeline>> {
        self.template.pipeline()
    }

    /// Get the reflection data (shared with template)
    pub fn reflection(&self) -> &ShaderReflection {
        &self.template.reflection
    }

    /// Set a parameter value
    pub fn set_parameter(&mut self, name: impl Into<String>, value: MaterialValue) {
        self.parameters.insert(name.into(), value);
    }

    /// Get a parameter value
    pub fn get_parameter(&self, name: &str) -> Option<&MaterialValue> {
        // Check instance parameters first
        if let Some(value) = self.parameters.get(name) {
            return Some(value);
        }
        // Fall back to template defaults
        self.template.default_parameters.get(name)
    }

    /// Set a texture for a binding slot
    pub fn set_texture(&mut self, slot: impl Into<String>, texture: Rc<Texture>) {
        self.textures.insert(slot.into(), texture);
    }

    /// Get a texture for a binding slot
    pub fn get_texture(&self, slot: &str) -> Option<&Rc<Texture>> {
        self.textures.get(slot)
    }

    /// Get all instance parameters (merged with defaults)
    pub fn get_all_parameters(&self) -> HashMap<String, MaterialValue> {
        let mut merged = HashMap::new();

        // Start with template defaults
        for (name, value) in self.template.descriptor.parameters.iter() {
            merged.insert(name.clone(), value.clone());
        }

        // Override with instance parameters
        for (name, value) in self.parameters.iter() {
            merged.insert(name.clone(), value.clone());
        }

        merged
    }

    /// Generate the uniform buffer data for this instance
    pub fn generate_uniform_buffer(&self) -> Result<Vec<u8>, InstanceError> {
        // Create a temporary parameters container with merged values
        let merged_params = self.get_all_parameters();

        // For now, we'll need to reconstruct the MaterialParameters
        // In a full implementation, this would use the reflection system
        // to generate the buffer with proper offsets

        // Get the uniform buffer layout
        let layout = self
            .template
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

    /// Clone this instance
    ///
    /// This creates a shallow copy that shares the template but has
    /// independent parameters and textures.
    pub fn clone_instance(&self) -> Self {
        Self {
            template: Rc::clone(&self.template),
            parameters: self.parameters.clone(),
            textures: self.textures.clone(),
        }
    }
}

// Note: Template tests require Vulkan context and are tested through
// the example programs and integration tests.
