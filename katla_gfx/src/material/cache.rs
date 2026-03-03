//! Material pipeline cache for deduplicating pipeline creation.

use std::collections::HashMap;
use std::rc::Rc;

use crate::handle::{PipelineHandle, ResourceStorage};
use crate::pipeline::{CompareOp, CullMode, FrontFace};
use crate::sync::VkRenderPass;
use crate::vulkan::context::VulkanContext;
use crate::vulkan::material::{MaterialPipeline, Pipeline, PipelineBuilder, ShaderModule};

use super::{MaterialDefinition, MaterialDomain, MaterialKey};

/// Error type for material pipeline cache operations.
#[derive(Debug)]
pub(crate) enum MaterialCacheError {
    /// Failed to create pipeline
    PipelineCreationFailed(String),
    /// Shader compilation failed
    ShaderCompilationFailed(String),
    /// Invalid material configuration
    InvalidConfiguration(String),
}

impl std::fmt::Display for MaterialCacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MaterialCacheError::PipelineCreationFailed(msg) => {
                write!(f, "Pipeline creation failed: {}", msg)
            }
            MaterialCacheError::ShaderCompilationFailed(msg) => {
                write!(f, "Shader compilation failed: {}", msg)
            }
            MaterialCacheError::InvalidConfiguration(msg) => {
                write!(f, "Invalid material configuration: {}", msg)
            }
        }
    }
}

impl std::error::Error for MaterialCacheError {}

/// Cache for material pipelines keyed by MaterialKey.
///
/// This cache deduplicates pipeline creation by reusing existing pipelines
/// when materials have compatible configurations. Pipelines are stored in
/// a central `ResourceStorage` and referenced by opaque `PipelineHandle`.
pub(crate) struct MaterialPipelineCache {
    context: Rc<VulkanContext>,
    cache: HashMap<MaterialKey, PipelineHandle>,
    storage: ResourceStorage<MaterialPipeline>,
}

impl MaterialPipelineCache {
    /// Create a new empty pipeline cache (internal).
    pub(crate) fn new(context: Rc<VulkanContext>) -> Self {
        Self {
            context,
            cache: HashMap::new(),
            storage: ResourceStorage::new(),
        }
    }

    /// Get or create a pipeline for the given material.
    ///
    /// If a compatible pipeline already exists in the cache, returns its handle.
    /// Otherwise, creates a new pipeline, caches it, and returns the new handle.
    ///
    /// # Arguments
    /// * `material` - MaterialDefinition implementation to create pipeline for
    ///
    /// # Returns
    /// * `Ok(PipelineHandle)` - Handle to the cached or newly created pipeline
    /// * `Err(MaterialCacheError)` - If pipeline creation fails
    pub(crate) fn get_or_create<M: MaterialDefinition + ?Sized>(
        &mut self,
        material: &M,
    ) -> Result<PipelineHandle, MaterialCacheError> {
        let key = MaterialKey::from_material(material);

        if let Some(&handle) = self.cache.get(&key) {
            return Ok(handle);
        }

        let pipeline = self.create_pipeline_for_material(material)?;
        let handle = PipelineHandle::new(self.storage.insert(pipeline));

        self.cache.insert(key, handle);
        Ok(handle)
    }

    /// Get or create a pipeline for bindless materials.
    ///
    /// Bindless materials require the bindless texture layout from
    /// BindlessTextureManager. This method creates pipelines that use
    /// the bindless texture array instead of individual texture descriptors.
    ///
    /// # Arguments
    /// * `material` - MaterialDefinition implementation to create pipeline for
    /// * `bindless_layout` - Descriptor set layout from BindlessTextureManager
    pub(crate) fn get_or_create_bindless<M: MaterialDefinition + ?Sized>(
        &mut self,
        material: &M,
        bindless_layout: crate::sync::VkDescriptorSetLayout,
    ) -> Result<PipelineHandle, MaterialCacheError> {
        let key = MaterialKey::from_material(material);

        if let Some(&handle) = self.cache.get(&key) {
            return Ok(handle);
        }

        let pipeline = self.create_bindless_pipeline(material, bindless_layout)?;
        let handle = PipelineHandle::new(self.storage.insert(pipeline));

        self.cache.insert(key, handle);
        Ok(handle)
    }

    /// Get a pipeline by handle.
    pub(crate) fn get_pipeline(&self, handle: PipelineHandle) -> Option<&MaterialPipeline> {
        if handle.is_none() {
            return None;
        }
        self.storage.get(handle.index())
    }

    /// Get a mutable pipeline by handle.
    pub(crate) fn get_pipeline_mut(
        &mut self,
        handle: PipelineHandle,
    ) -> Option<&mut MaterialPipeline> {
        if handle.is_none() {
            return None;
        }
        self.storage.get_mut(handle.index())
    }

    /// Create a bindless pipeline for a material.
    fn create_bindless_pipeline<M: MaterialDefinition + ?Sized>(
        &self,
        material: &M,
        bindless_layout: crate::sync::VkDescriptorSetLayout,
    ) -> Result<MaterialPipeline, MaterialCacheError> {
        let render_state = material.render_state();
        let vertex_binding = material.vertex_binding();

        if !material.uses_bindless() {
            return Err(MaterialCacheError::InvalidConfiguration(
                "get_or_create_bindless() requires uses_bindless() to return true".to_string(),
            ));
        }

        let vert_shader =
            self.load_shader(&material.vertex_shader(), ash::vk::ShaderStageFlags::VERTEX)?;
        let frag_shader = self.load_shader(
            &material.fragment_shader(),
            ash::vk::ShaderStageFlags::FRAGMENT,
        )?;

        let layout_builders = material.descriptor_layouts();
        let mut vk_layouts: Vec<ash::vk::DescriptorSetLayout> = Vec::new();
        let mut wrapped_layouts: Vec<crate::sync::VkDescriptorSetLayout> = Vec::new();

        if let Some(builder) = layout_builders.first() {
            let wrapped = builder.clone().build(&self.context).map_err(|e| {
                MaterialCacheError::PipelineCreationFailed(format!(
                    "Descriptor layout failed: {:?}",
                    e
                ))
            })?;
            vk_layouts.push(wrapped.vk());
            wrapped_layouts.push(wrapped);
        }

        vk_layouts.push(bindless_layout.vk());

        if material.uses_skeleton()
            && let Some(builder) = layout_builders.get(1)
        {
            let wrapped = builder.clone().build(&self.context).map_err(|e| {
                MaterialCacheError::PipelineCreationFailed(format!(
                    "Skeleton layout failed: {:?}",
                    e
                ))
            })?;
            vk_layouts.push(wrapped.vk());
            wrapped_layouts.push(wrapped);
        }

        let mut pipeline_builder = PipelineBuilder::new(self.context.clone())
            .with_shaders(vert_shader.module, frag_shader.module)
            .with_entry_points(
                vert_shader.entry_point.clone(),
                frag_shader.entry_point.clone(),
            )
            .with_vertex_input(
                vec![vertex_binding.get_binding_desc(0)],
                vertex_binding.get_attribute_desc(0),
            )
            .with_depth_test(
                render_state.depth_test,
                render_state.depth_write,
                CompareOp::Greater,
            )
            .with_descriptor_layouts(vk_layouts.clone())
            .with_rendering_formats(Some(material.color_format()), Some(material.depth_format()));

        if render_state.cull_backfaces {
            pipeline_builder =
                pipeline_builder.with_cull_mode(CullMode::Back, FrontFace::CounterClockwise);
        } else {
            pipeline_builder =
                pipeline_builder.with_cull_mode(CullMode::None, FrontFace::CounterClockwise);
        }

        if render_state.alpha_blending {
            pipeline_builder = pipeline_builder.with_alpha_blending();
        }

        let pipeline = pipeline_builder
            .build(VkRenderPass::from(ash::vk::RenderPass::null()))
            .map_err(|e| MaterialCacheError::PipelineCreationFailed(format!("{:?}", e)))?;

        let uniform_set_layout = vk_layouts
            .first()
            .copied()
            .unwrap_or(ash::vk::DescriptorSetLayout::null());
        let skeleton_set_layout = if material.uses_skeleton() {
            vk_layouts.get(2).copied()
        } else {
            None
        };

        let material_pipeline = if material.uses_skeleton() {
            MaterialPipeline::new_bindless_skinned(
                pipeline,
                uniform_set_layout,
                skeleton_set_layout.unwrap_or(ash::vk::DescriptorSetLayout::null()),
                self.context.clone(),
            )
        } else {
            MaterialPipeline::new_bindless(pipeline, uniform_set_layout, self.context.clone())
        };

        Ok(material_pipeline)
    }

    /// Create a pipeline for a material using the MaterialDefinition trait directly.
    fn create_pipeline_for_material<M: MaterialDefinition + ?Sized>(
        &self,
        material: &M,
    ) -> Result<MaterialPipeline, MaterialCacheError> {
        let render_state = material.render_state();
        let vertex_binding = material.vertex_binding();

        if material.uses_bindless() {
            return Err(MaterialCacheError::InvalidConfiguration(
                "Bindless materials require bindless_layout. Use get_or_create_bindless() instead."
                    .to_string(),
            ));
        }

        let vert_shader =
            self.load_shader(&material.vertex_shader(), ash::vk::ShaderStageFlags::VERTEX)?;
        let frag_shader = self.load_shader(
            &material.fragment_shader(),
            ash::vk::ShaderStageFlags::FRAGMENT,
        )?;

        let layout_builders = material.descriptor_layouts();
        let mut vk_layouts: Vec<ash::vk::DescriptorSetLayout> =
            Vec::with_capacity(layout_builders.len());
        let mut wrapped_layouts: Vec<crate::sync::VkDescriptorSetLayout> =
            Vec::with_capacity(layout_builders.len());

        for builder in &layout_builders {
            let wrapped = builder.clone().build(&self.context).map_err(|e| {
                MaterialCacheError::PipelineCreationFailed(format!(
                    "Descriptor layout failed: {:?}",
                    e
                ))
            })?;
            vk_layouts.push(wrapped.vk());
            wrapped_layouts.push(wrapped);
        }

        let mut pipeline_builder = PipelineBuilder::new(self.context.clone())
            .with_shaders(vert_shader.module, frag_shader.module)
            .with_entry_points(
                vert_shader.entry_point.clone(),
                frag_shader.entry_point.clone(),
            )
            .with_vertex_input(
                vec![vertex_binding.get_binding_desc(0)],
                vertex_binding.get_attribute_desc(0),
            )
            .with_depth_test(
                render_state.depth_test,
                render_state.depth_write,
                CompareOp::Greater,
            )
            .with_descriptor_layouts(vk_layouts.clone())
            .with_rendering_formats(Some(material.color_format()), Some(material.depth_format()));

        if render_state.cull_backfaces {
            pipeline_builder =
                pipeline_builder.with_cull_mode(CullMode::Back, FrontFace::CounterClockwise);
        } else {
            pipeline_builder =
                pipeline_builder.with_cull_mode(CullMode::None, FrontFace::CounterClockwise);
        }

        if render_state.alpha_blending {
            pipeline_builder = pipeline_builder.with_alpha_blending();
        }

        let pipeline = pipeline_builder
            .build(VkRenderPass::from(ash::vk::RenderPass::null()))
            .map_err(|e| MaterialCacheError::PipelineCreationFailed(format!("{:?}", e)))?;

        let material_pipeline = self.create_material_pipeline(
            pipeline,
            wrapped_layouts,
            material.domain(),
            material.uses_skeleton(),
        );

        Ok(material_pipeline)
    }

    /// Load a shader from ShaderSource.
    fn load_shader(
        &self,
        source: &crate::vulkan::material::descriptor::ShaderSource,
        stage: ash::vk::ShaderStageFlags,
    ) -> Result<ShaderModule, MaterialCacheError> {
        let entry_point = std::ffi::CString::new(if stage == ash::vk::ShaderStageFlags::VERTEX {
            "vs_main"
        } else {
            "fs_main"
        })
        .unwrap();

        use crate::vulkan::material::descriptor::ShaderSource;

        match source {
            ShaderSource::WgslFile(path) => ShaderModule::from_wgsl(
                self.context.device.clone(),
                path,
                stage,
                entry_point.to_str().unwrap(),
            )
            .map_err(|e| MaterialCacheError::ShaderCompilationFailed(format!("{:?}", e))),
            ShaderSource::WgslString(code) => ShaderModule::from_wgsl_string(
                self.context.device.clone(),
                code,
                stage,
                entry_point.to_str().unwrap(),
            )
            .map_err(|e| MaterialCacheError::ShaderCompilationFailed(format!("{:?}", e))),
            ShaderSource::PreCompiled(bytes) => {
                ShaderModule::from_bytes(self.context.device.clone(), bytes, stage, "main")
                    .map_err(|e| MaterialCacheError::ShaderCompilationFailed(format!("{:?}", e)))
            }
        }
    }

    /// Create a MaterialPipeline from the raw pipeline and layouts.
    fn create_material_pipeline(
        &self,
        pipeline: Pipeline,
        layouts: Vec<crate::sync::VkDescriptorSetLayout>,
        domain: MaterialDomain,
        uses_skeleton: bool,
    ) -> MaterialPipeline {
        // Convert layouts to vk types
        let vk_layouts: Vec<ash::vk::DescriptorSetLayout> =
            layouts.iter().map(|l| l.vk()).collect();

        // Determine layout assignment based on domain and skeleton
        let uniform_set_layout = vk_layouts
            .first()
            .copied()
            .unwrap_or(ash::vk::DescriptorSetLayout::null());
        let texture_set_layout = vk_layouts.get(1).copied();
        let skeleton_set_layout = if uses_skeleton {
            vk_layouts.get(2).copied()
        } else {
            None
        };

        // Create appropriate MaterialPipeline based on configuration
        match domain {
            MaterialDomain::Ui => {
                // UI materials use two sets: uniform (set 0) and push descriptor (set 1)
                let push_descriptor_layout = vk_layouts
                    .get(1)
                    .copied()
                    .unwrap_or(ash::vk::DescriptorSetLayout::null());
                MaterialPipeline::new_ui(
                    pipeline,
                    uniform_set_layout,
                    push_descriptor_layout,
                    self.context.clone(),
                )
            }
            _ => {
                // Standard surface/post-process/particle materials
                if uses_skeleton {
                    MaterialPipeline::new_storage_skinned(
                        pipeline,
                        uniform_set_layout,
                        texture_set_layout.unwrap_or(ash::vk::DescriptorSetLayout::null()),
                        skeleton_set_layout.unwrap_or(ash::vk::DescriptorSetLayout::null()),
                        self.context.clone(),
                    )
                } else if let Some(tex_layout) = texture_set_layout {
                    MaterialPipeline::new_storage(
                        pipeline,
                        uniform_set_layout,
                        tex_layout,
                        self.context.clone(),
                    )
                } else {
                    MaterialPipeline::new_custom(pipeline, uniform_set_layout, self.context.clone())
                }
            }
        }
    }

    /// Get the number of cached pipelines.
    pub(crate) fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if the cache is empty.
    pub(crate) fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Check if a pipeline exists for the given material.
    pub(crate) fn contains<M: MaterialDefinition + ?Sized>(&self, material: &M) -> bool {
        let key = MaterialKey::from_material(material);
        self.cache.contains_key(&key)
    }

    /// Clear all cached pipelines.
    pub(crate) fn clear(&mut self) {
        self.cache.clear();
        self.storage.clear();
    }

    /// Remove a specific pipeline from the cache.
    ///
    /// Returns true if the pipeline was in the cache and was removed.
    pub(crate) fn remove<M: MaterialDefinition + ?Sized>(&mut self, material: &M) -> bool {
        let key = MaterialKey::from_material(material);
        if let Some(handle) = self.cache.remove(&key) {
            self.storage.remove(handle.index());
            true
        } else {
            false
        }
    }

    /// Get statistics about the cache.
    pub(crate) fn stats(&self) -> MaterialCacheStats {
        let mut by_domain = HashMap::new();
        for key in self.cache.keys() {
            *by_domain.entry(key.domain).or_insert(0) += 1;
        }
        MaterialCacheStats {
            total_pipelines: self.cache.len(),
            by_domain,
        }
    }
}

impl Drop for MaterialPipelineCache {
    fn drop(&mut self) {
        if !self.cache.is_empty() {
            log::debug!(
                "MaterialPipelineCache dropping with {} pipelines",
                self.cache.len()
            );
        }
    }
}

/// Statistics about the material pipeline cache.
#[derive(Debug, Clone)]
pub(crate) struct MaterialCacheStats {
    /// Total number of cached pipelines
    pub total_pipelines: usize,
    /// Pipelines grouped by domain
    pub by_domain: HashMap<MaterialDomain, usize>,
}
