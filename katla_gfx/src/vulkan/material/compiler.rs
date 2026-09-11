//! Material compiler for compiling materials from shaders.
//!
//! This module handles the compilation of WGSL shaders into SPIR-V and
//! the creation of Vulkan graphics pipelines for rendering.

use crate::vulkan::bindless_texture::BindlessTextureManager;
use crate::vulkan::context::VulkanContext;
use crate::vulkan::material::shadermodule::ShaderCache;
use crate::vulkan::material::storage_uniform::StorageDescriptorSet;
use ash::vk;
use std::{cell::RefCell, path::Path, rc::Rc};

/// Error types for material compilation.
#[derive(Debug)]
pub enum MaterialError {
    ShaderCompilation(String),
    PipelineCreation(String),
    /// A pipeline variant failed to compile; carries the material and the
    /// resolved variant identity for diagnosis.
    VariantCompilation {
        material: crate::handle::MaterialHandle,
        color_format: crate::texture::ImageFormat,
        depth_format: Option<crate::texture::ImageFormat>,
        reason: String,
    },
}

impl std::fmt::Display for MaterialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ShaderCompilation(s) => write!(f, "Shader compilation failed: {}", s),
            Self::PipelineCreation(s) => write!(f, "Pipeline creation failed: {}", s),
            Self::VariantCompilation {
                material,
                color_format,
                depth_format,
                reason,
            } => write!(
                f,
                "Pipeline variant for material {material:?} (color {color_format:?}, \
                 depth {depth_format:?}) failed: {reason}"
            ),
        }
    }
}

impl std::error::Error for MaterialError {}

/// Compiles material definitions into Vulkan pipelines.
pub(crate) struct MaterialCompiler {
    pub(crate) shader_cache: Rc<RefCell<ShaderCache>>,
    context: Rc<VulkanContext>,
    storage_descriptor_layout: Option<vk::DescriptorSetLayout>,
    bindless_descriptor_layout: vk::DescriptorSetLayout,
    /// Skeleton descriptor layout (Set 2 for skinned meshes)
    skeleton_descriptor_layout: Option<vk::DescriptorSetLayout>,
    /// Compositing descriptor set layout (Set 2 for compositing pass)
    /// Set dynamically when compiling compositing materials
    compositing_descriptor_set_layout: Option<vk::DescriptorSetLayout>,
    /// Light culling descriptor set layout (Set 3 for PBR materials with dynamic lights)
    light_culling_descriptor_layout: Option<vk::DescriptorSetLayout>,
    /// Shadow descriptor set layout (Set 4 for PBR materials with shadow mapping)
    shadow_descriptor_layout: Option<vk::DescriptorSetLayout>,
    /// Empty placeholder descriptor set layout (Set 2 for PBR materials without skeleton)
    empty_descriptor_layout: Option<vk::DescriptorSetLayout>,
    /// Shared descriptor pool for skeleton descriptor sets
    skeleton_descriptor_pool: Option<vk::DescriptorPool>,
    /// UI descriptor set layouts (created per UI material, tracked for cleanup)
    ui_descriptor_layouts: Vec<vk::DescriptorSetLayout>,
}

impl MaterialCompiler {
    pub(crate) fn new(
        context: Rc<VulkanContext>,
        bindless_manager: &BindlessTextureManager,
        _storage_descriptor_set: &StorageDescriptorSet,
    ) -> Result<Self, MaterialError> {
        let bindless_descriptor_layout = bindless_manager.descriptor_set_layout();

        // Storage descriptor layout (set 0: frame_data + objects)
        let storage_bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
        ];

        let storage_layout_info =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(&storage_bindings);

        let storage_descriptor_layout = unsafe {
            context
                .device
                .create_descriptor_set_layout(&storage_layout_info, None)
        }
        .map_err(|e| {
            MaterialError::ShaderCompilation(format!(
                "Failed to create storage descriptor layout: {:?}",
                e
            ))
        })?;

        // Skeleton descriptor layout (set 2 for skinned meshes)
        let skeleton_binding = [vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX)];

        let skeleton_layout_info =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(&skeleton_binding);

        let skeleton_descriptor_layout = unsafe {
            context
                .device
                .create_descriptor_set_layout(&skeleton_layout_info, None)
        }
        .map_err(|e| {
            MaterialError::ShaderCompilation(format!(
                "Failed to create skeleton descriptor layout: {:?}",
                e
            ))
        })?;

        // Skeleton descriptor pool (shared across all skeletons)
        // Max 1024 skeletons - should be more than enough for most scenes
        let skeleton_pool_sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1024)];

        let skeleton_pool_info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(1024)
            .pool_sizes(&skeleton_pool_sizes);

        let skeleton_descriptor_pool = unsafe {
            context
                .device
                .create_descriptor_pool(&skeleton_pool_info, None)
        }
        .map_err(|e| {
            MaterialError::ShaderCompilation(format!(
                "Failed to create skeleton descriptor pool: {:?}",
                e
            ))
        })?;

        // Empty placeholder descriptor set layout (Set 2 for PBR materials without skeleton)
        // Ensures light culling always occupies Set 3 in the pipeline layout
        let empty_layout_info = vk::DescriptorSetLayoutCreateInfo::default();
        let empty_descriptor_layout = unsafe {
            context
                .device
                .create_descriptor_set_layout(&empty_layout_info, None)
        }
        .map_err(|e| {
            MaterialError::ShaderCompilation(format!(
                "Failed to create empty descriptor layout: {:?}",
                e
            ))
        })?;

        Ok(Self {
            shader_cache: Rc::new(RefCell::new(ShaderCache::new(context.device.clone()))),
            context,
            storage_descriptor_layout: Some(storage_descriptor_layout),
            bindless_descriptor_layout,
            skeleton_descriptor_layout: Some(skeleton_descriptor_layout),
            compositing_descriptor_set_layout: None,
            light_culling_descriptor_layout: None,
            shadow_descriptor_layout: None,
            empty_descriptor_layout: Some(empty_descriptor_layout),
            skeleton_descriptor_pool: Some(skeleton_descriptor_pool),
            ui_descriptor_layouts: Vec::new(),
        })
    }

    /// Register a material from its compilation descriptor.
    ///
    /// The material starts with no compiled variants. A descriptor with a
    /// concrete color format is compiled for that format immediately;
    /// `ImageFormat::Auto` defers all compilation to the first use, where
    /// the pass's target format selects the variant
    /// (see [`compile_variant`](Self::compile_variant)).
    pub(crate) fn compile(
        &mut self,
        registry: &mut crate::renderer::registry::AssetRegistry,
        descriptor: &crate::renderer::pipeline_descriptor::PipelineDescriptor,
    ) -> Result<crate::handle::MaterialHandle, MaterialError> {
        self.validate_layout(&descriptor.vertex)?;

        let handle = registry.register_material(crate::renderer::registry::MaterialAsset {
            descriptor: descriptor.clone(),
            variants: std::collections::HashMap::new(),
            textures: crate::renderer::registry::MaterialTextures::default(),
        });

        if descriptor.color_format != crate::texture::ImageFormat::Auto {
            let key = crate::renderer::pipeline_variant::PipelineVariantKey::resolve(
                descriptor,
                descriptor.color_format,
            );
            self.compile_variant(registry, handle, &key)?;
        }

        Ok(handle)
    }

    /// Compile one pipeline variant of a registered material.
    ///
    /// The key's resolved formats select the attachments the pipeline is
    /// built for; the material's descriptor provides every other input.
    /// The compiled pipelines are cached on the material under the key.
    pub(crate) fn compile_variant(
        &mut self,
        registry: &mut crate::renderer::registry::AssetRegistry,
        material_handle: crate::handle::MaterialHandle,
        key: &crate::renderer::pipeline_variant::PipelineVariantKey,
    ) -> Result<crate::renderer::registry::MaterialVariant, MaterialError> {
        let fail = |reason: String| MaterialError::VariantCompilation {
            material: material_handle,
            color_format: key.color_format(),
            depth_format: key.depth_format(),
            reason,
        };

        if registry.get_material(material_handle).is_none() {
            return Err(fail("material handle not found".to_string()));
        }
        // Build from the key's resolved descriptor: its color format is
        // concrete, so the pipeline targets exactly the key's configuration
        // even when the material was registered with `Auto`.
        let descriptor = key.descriptor().clone();
        let vertex_binding = self.validate_layout(&descriptor.vertex)?;

        let (vertex_entry, fragment_entry) = match &descriptor.stages {
            crate::renderer::pipeline_descriptor::PipelineStages::Graphics {
                vertex_entry,
                fragment_entry,
            } => (vertex_entry.clone(), fragment_entry.clone()),
            crate::renderer::pipeline_descriptor::PipelineStages::Compute { .. } => {
                return Err(fail(
                    "compute pipelines have no graphics variants".to_string(),
                ));
            }
        };

        let shader_path = std::path::PathBuf::from(&descriptor.shader_path);
        let mut cache = self.shader_cache.borrow_mut();
        let vert_module = cache
            .load_shader_with_entry(&shader_path, vk::ShaderStageFlags::VERTEX, &vertex_entry)
            .map_err(|e| fail(format!("Vertex shader: {e:?}")))?;
        let frag_module = cache
            .load_shader_with_entry(
                &shader_path,
                vk::ShaderStageFlags::FRAGMENT,
                &fragment_entry,
            )
            .map_err(|e| fail(format!("Fragment shader: {e:?}")))?;
        drop(cache);

        let layouts = self
            .build_descriptor_layouts(&descriptor)
            .map_err(|e| fail(e.to_string()))?;

        let pipeline = self
            .build_pipeline(
                &descriptor,
                vert_module,
                frag_module,
                &layouts,
                &vertex_binding,
                key.depth_format(),
            )
            .map_err(|e| fail(e.to_string()))?;
        let pipeline_handle = registry.register_pipeline(pipeline);

        // UI materials additionally compile an instanced pipeline using
        // vs_instanced/fs_instanced entry points with UnitQuadVertex format.
        let instanced_pipeline = if descriptor.is_ui_layout() {
            Some(
                registry.register_pipeline(
                    self.build_instanced_ui_pipeline(&shader_path, &layouts, key.color_format())
                        .map_err(|e| fail(e.to_string()))?,
                ),
            )
        } else {
            None
        };

        let variant = crate::renderer::registry::MaterialVariant {
            pipeline: pipeline_handle,
            instanced_pipeline,
        };

        if !registry.insert_material_variant(material_handle, key.clone(), variant) {
            return Err(fail("material handle not found".to_string()));
        }

        log::debug!(
            "compile_variant: material {material_handle:?} color {:?} depth {:?}",
            key.color_format(),
            key.depth_format()
        );
        Ok(variant)
    }

    /// Reject vertex layouts no backend can bind, returning the vertex
    /// binding for the canonical layouts.
    fn validate_layout(
        &self,
        layout: &crate::vertex::VertexLayout,
    ) -> Result<crate::vulkan::vertexbinding::VertexBinding, MaterialError> {
        use crate::vertex::VertexLayout;

        if layout == &VertexLayout::pbr()
            || layout == &VertexLayout::ui()
            || layout == &VertexLayout::position()
            || layout == &VertexLayout::pbr_skinned()
        {
            Ok(crate::vulkan::vertexbinding::VertexBinding::from(layout))
        } else {
            Err(MaterialError::ShaderCompilation(format!(
                "unknown vertex layout ({} attributes, stride {}): Vulkan material \
                 compilation supports the canonical PBR / UI / position / skinned layouts",
                layout.len(),
                layout.stride(),
            )))
        }
    }

    fn build_descriptor_layouts(
        &mut self,
        descriptor: &crate::renderer::pipeline_descriptor::PipelineDescriptor,
    ) -> Result<Vec<vk::DescriptorSetLayout>, MaterialError> {
        // UI materials use a completely different descriptor set layout
        if descriptor.is_ui_layout() {
            return self.build_ui_descriptor_layout();
        }

        let mut layouts = vec![
            self.storage_descriptor_layout
                .expect("storage descriptor layout not initialized"),
            self.bindless_descriptor_layout,
        ];

        // Set 2: skeleton, compositing, or empty placeholder
        // All three are mutually exclusive at set 2 so the pipeline layout
        // indices are consistent for downstream descriptor set binding.
        let is_skinned = descriptor.vertex == crate::vertex::VertexLayout::pbr_skinned();
        let is_pbr = descriptor.vertex == crate::vertex::VertexLayout::pbr();
        if descriptor.native.vulkan.compositing {
            if let Some(layout) = self.compositing_descriptor_set_layout {
                layouts.push(layout);
            } else {
                // Fallback: compositing not set up yet, use empty placeholder
                layouts.push(
                    self.empty_descriptor_layout
                        .expect("empty descriptor layout not initialized"),
                );
            }
        } else if is_skinned {
            layouts.push(
                self.skeleton_descriptor_layout
                    .expect("skeleton descriptor layout not initialized"),
            );
        } else if is_pbr {
            layouts.push(self.empty_descriptor_layout.unwrap());
        }

        // Add light culling descriptor set layout (Set 3) for PBR materials
        // Both PBR and Skinned materials support Forward+ dynamic lighting
        if (is_pbr || is_skinned)
            && let Some(layout) = self.light_culling_descriptor_layout
        {
            layouts.push(layout);
        }

        // Add shadow descriptor set layout (Set 4) for PBR materials
        if (is_pbr || is_skinned)
            && let Some(layout) = self.shadow_descriptor_layout
        {
            layouts.push(layout);
        }

        Ok(layouts)
    }

    /// Build UI descriptor set layouts.
    ///
    /// UI shader uses bindless textures:
    /// - Set 0: UI resources (sampler, uniforms)
    /// - Set 1: Bindless texture array (shared with 3D materials)
    fn build_ui_descriptor_layout(
        &mut self,
    ) -> Result<Vec<vk::DescriptorSetLayout>, MaterialError> {
        // UI descriptor set layout Set 0 (must match shader bindings in ui.wgsl):
        // - Binding 1: sampler (shared)
        // - Binding 3: uniforms (screen_size, ndc_y_flip, texture_index)
        // - Binding 4: instance data storage buffer (for instanced draws)
        let ui_bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(3)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(4)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
        ];

        let layout_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&ui_bindings);

        let ui_layout = unsafe {
            self.context
                .device
                .create_descriptor_set_layout(&layout_info, None)
                .map_err(|e| {
                    MaterialError::ShaderCompilation(format!(
                        "Failed to create UI descriptor layout: {:?}",
                        e
                    ))
                })?
        };

        // Track this layout for cleanup (owned by MaterialCompiler)
        self.ui_descriptor_layouts.push(ui_layout);

        // Return both layouts: Set 0 (UI resources) and Set 1 (bindless textures)
        Ok(vec![ui_layout, self.bindless_descriptor_layout])
    }

    /// Get the skeleton descriptor pool for allocating skeleton descriptor sets.
    pub(crate) fn skeleton_descriptor_pool(&self) -> vk::DescriptorPool {
        self.skeleton_descriptor_pool
            .expect("skeleton descriptor pool not initialized")
    }

    /// Get the skeleton descriptor layout.
    pub(crate) fn skeleton_descriptor_layout(&self) -> vk::DescriptorSetLayout {
        self.skeleton_descriptor_layout
            .expect("skeleton descriptor layout not initialized")
    }

    /// Invalidate cached shader modules for the given path.
    pub(crate) fn invalidate_shader_cache(&self, path: &Path) {
        self.shader_cache.borrow_mut().invalidate(path);
    }

    /// Set the compositing descriptor set layout for compiling compositing materials.
    ///
    /// This must be set before compiling a material with `is_compositing: true`.
    /// The layout is created by the frame graph's compositing descriptor set.
    pub(crate) fn set_compositing_descriptor_set_layout(
        &mut self,
        layout: vk::DescriptorSetLayout,
    ) {
        self.compositing_descriptor_set_layout = Some(layout);
    }

    /// Clear the compositing descriptor set layout after compilation.
    pub(crate) fn clear_compositing_descriptor_set_layout(&mut self) {
        self.compositing_descriptor_set_layout = None;
    }

    /// Set the light culling descriptor set layout for compiling PBR materials.
    ///
    /// This must be set before compiling PBR materials. The layout comes from
    /// the LightCullingBuffers and is used as Set 3 in the PBR pipeline.
    pub(crate) fn set_light_culling_descriptor_layout(&mut self, layout: vk::DescriptorSetLayout) {
        self.light_culling_descriptor_layout = Some(layout);
    }

    pub(crate) fn set_shadow_descriptor_layout(&mut self, layout: vk::DescriptorSetLayout) {
        self.shadow_descriptor_layout = Some(layout);
    }

    /// Build the instanced UI pipeline using `vs_instanced`/`fs_instanced` entry points.
    ///
    /// This pipeline uses the `UnitQuadVertex` format (Float2 only at binding 0) and
    /// reads per-instance data from the `instance_data` storage buffer (binding 4, set 0).
    /// The vertex format is minimal because all positioning/sizing/coloring comes from
    /// the instance data, not from per-vertex attributes.
    fn build_instanced_ui_pipeline(
        &self,
        shader_path: &Path,
        layouts: &[vk::DescriptorSetLayout],
        color_format: crate::texture::ImageFormat,
    ) -> Result<crate::vulkan::material::builder::Pipeline, MaterialError> {
        use crate::pipeline::{CullMode, FrontFace};
        use crate::vulkan::material::builder::PipelineBuilder;

        // Load shaders with instanced entry points
        let mut cache = self.shader_cache.borrow_mut();
        let vert_module = cache
            .load_shader_with_entry(shader_path, vk::ShaderStageFlags::VERTEX, "vs_instanced")
            .map_err(|e| {
                MaterialError::ShaderCompilation(format!("Instanced vertex shader: {:?}", e))
            })?;
        let frag_module = cache
            .load_shader_with_entry(shader_path, vk::ShaderStageFlags::FRAGMENT, "fs_instanced")
            .map_err(|e| {
                MaterialError::ShaderCompilation(format!("Instanced fragment shader: {:?}", e))
            })?;
        drop(cache);

        // Unit quad vertex binding: only Float2 local_pos
        let quad_binding = crate::vulkan::vertexbinding::VertexBinding::from(
            &crate::vertex::VertexLayout::new(vec![crate::vertex::VertexAttributeFormat::Float2]),
        );

        let pipeline = PipelineBuilder::new(self.context.clone())
            .with_shaders(vert_module, frag_module)
            .with_entry_points(c"vs_instanced", c"fs_instanced")
            .with_vertex_binding(quad_binding)
            .with_descriptor_layouts(layouts.to_vec())
            .with_rendering_formats(Some(color_format), None)
            .with_depth_test(false, false, crate::pipeline::CompareOp::Always)
            .with_cull_mode(CullMode::None, FrontFace::CounterClockwise)
            .with_alpha_blending()
            .build(crate::sync::VkRenderPass::default())
            .map_err(|e| {
                MaterialError::PipelineCreation(format!("Instanced UI pipeline: {}", e))
            })?;

        log::info!("UI instanced pipeline compiled with vs_instanced/fs_instanced entry points");
        Ok(pipeline)
    }

    fn build_pipeline(
        &self,
        descriptor: &crate::renderer::pipeline_descriptor::PipelineDescriptor,
        vert_module: vk::ShaderModule,
        frag_module: vk::ShaderModule,
        layouts: &[vk::DescriptorSetLayout],
        vertex_binding: &crate::vulkan::vertexbinding::VertexBinding,
        depth_format: Option<crate::texture::ImageFormat>,
    ) -> Result<crate::vulkan::material::builder::Pipeline, MaterialError> {
        use crate::pipeline::PolygonMode;
        use crate::renderer::pipeline_descriptor::PipelineStages;
        use crate::vulkan::material::builder::PipelineBuilder;

        // UI materials use different rendering configuration
        let is_ui = descriptor.is_ui_layout();

        let (vertex_entry, fragment_entry) = match &descriptor.stages {
            PipelineStages::Graphics {
                vertex_entry,
                fragment_entry,
            } => (vertex_entry, fragment_entry),
            PipelineStages::Compute { .. } => {
                return Err(MaterialError::PipelineCreation(
                    "compute pipelines have no graphics variants".to_string(),
                ));
            }
        };
        let vertex_entry = std::ffi::CString::new(vertex_entry.as_str()).map_err(|e| {
            MaterialError::PipelineCreation(format!("invalid vertex entry point: {e}"))
        })?;
        let fragment_entry = std::ffi::CString::new(fragment_entry.as_str()).map_err(|e| {
            MaterialError::PipelineCreation(format!("invalid fragment entry point: {e}"))
        })?;

        // SOA vertex bindings for non-UI materials (separate per-attribute buffers)
        // UI materials keep interleaved binding for dynamic mesh updates
        let mut builder = if is_ui {
            PipelineBuilder::new(self.context.clone())
                .with_shaders(vert_module, frag_module)
                .with_entry_points(vertex_entry.as_c_str(), fragment_entry.as_c_str())
                .with_vertex_binding(vertex_binding.clone())
                .with_descriptor_layouts(layouts.to_vec())
        } else {
            PipelineBuilder::new(self.context.clone())
                .with_shaders(vert_module, frag_module)
                .with_entry_points(vertex_entry.as_c_str(), fragment_entry.as_c_str())
                .with_vertex_binding_soa(vertex_binding.clone())
                .with_descriptor_layouts(layouts.to_vec())
        };

        // Attachments come from the variant key: the resolved color format
        // plus the shared depth derivation (None for UI, compositing, and
        // depth-test-disabled pipelines).
        builder = builder.with_rendering_formats(Some(descriptor.color_format), depth_format);

        // A depth-free pipeline normalises to no test/write and Always so
        // every entry path shares the same effective state.
        if depth_format.is_some() {
            builder =
                builder.with_depth_test(true, descriptor.depth.write, descriptor.depth.compare);
        } else {
            builder = builder.with_depth_test(false, false, crate::pipeline::CompareOp::Always);
        }

        builder = builder.with_cull_mode(
            descriptor.cull,
            crate::pipeline::FrontFace::CounterClockwise,
        );

        if descriptor.blend == crate::renderer::pipeline_descriptor::BlendMode::AlphaBlend {
            builder = builder.with_alpha_blending();
        }

        if descriptor.wireframe {
            builder = builder.with_polygon_mode(PolygonMode::Line);
        }

        builder.build(crate::sync::VkRenderPass::default()).map_err(
            |e: crate::vulkan::material::builder::PipelineError| {
                MaterialError::PipelineCreation(e.to_string())
            },
        )
    }

    /// Clean up descriptor layouts and pool.
    /// This is idempotent - can be called multiple times safely.
    pub(crate) fn destroy(&mut self) {
        if let Some(layout) = self.storage_descriptor_layout.take() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(layout, None);
            }
        }

        if let Some(layout) = self.skeleton_descriptor_layout.take() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(layout, None);
            }
        }

        if let Some(pool) = self.skeleton_descriptor_pool.take() {
            unsafe {
                self.context.device.destroy_descriptor_pool(pool, None);
            }
        }

        if let Some(layout) = self.empty_descriptor_layout.take() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(layout, None);
            }
        }

        for layout in self.ui_descriptor_layouts.drain(..) {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(layout, None);
            }
        }
    }
}

impl Drop for MaterialCompiler {
    fn drop(&mut self) {
        self.destroy();
    }
}
