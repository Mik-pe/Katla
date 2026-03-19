//! Material compiler for compiling materials from shaders.
//!
//! This module handles the compilation of WGSL shaders into SPIR-V and
//! the creation of Vulkan graphics pipelines for rendering.

use crate::StorageDescriptorSet;
use crate::texture::ImageFormat;
use crate::vulkan::bindless_texture::BindlessTextureManager;
use crate::vulkan::context::VulkanContext;
use crate::vulkan::material::shadermodule::ShaderCache;
use ash::vk;
use std::{cell::RefCell, path::Path, path::PathBuf, rc::Rc};

/// Error types for material compilation.
#[derive(Debug)]
pub enum MaterialError {
    ShaderCompilation(String),
    PipelineCreation(String),
}

impl std::fmt::Display for MaterialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ShaderCompilation(s) => write!(f, "Shader compilation failed: {}", s),
            Self::PipelineCreation(s) => write!(f, "Pipeline creation failed: {}", s),
        }
    }
}

impl std::error::Error for MaterialError {}

/// Material type presets.
#[derive(Clone, Copy, Debug)]
pub(crate) enum MaterialType {
    Auto,
    Pbr,
    Ui,
}

/// Vertex type presets.
#[derive(Clone, Copy, Debug)]
pub enum VertexType {
    Pbr,
    Ui,
    Simple,
    Skinned,
}

/// Options for material creation.
#[derive(Clone, Debug)]
pub struct MaterialOptions {
    pub alpha_blended: bool,
    pub double_sided: bool,
    pub wireframe: bool,
    pub vertex_type: VertexType,
    /// Color attachment format for this material.
    /// Default is B8G8R8A8Srgb (swapchain format).
    /// Use R16G16B16A16Sfloat for HDR rendering.
    pub color_format: ImageFormat,
    /// Whether this material uses compositing (requires set 2 descriptor set layout).
    /// Default is false.
    pub is_compositing: bool,
}

impl Default for MaterialOptions {
    fn default() -> Self {
        Self {
            alpha_blended: false,
            double_sided: false,
            wireframe: false,
            vertex_type: VertexType::Pbr,
            color_format: ImageFormat::B8G8R8A8Srgb,
            is_compositing: false,
        }
    }
}

/// Compiles material definitions into Vulkan pipelines.
pub(crate) struct MaterialCompiler {
    pub(crate) shader_cache: Rc<RefCell<ShaderCache>>,
    context: Rc<VulkanContext>,
    storage_descriptor_layout: Option<vk::DescriptorSetLayout>,
    bindless_descriptor_layout: vk::DescriptorSetLayout,
    /// Skeleton descriptor layout (Set 2 for skinned meshes)
    skeleton_descriptor_layout: vk::DescriptorSetLayout,
    /// Compositing descriptor set layout (Set 2 for compositing pass)
    /// Set dynamically when compiling compositing materials
    compositing_descriptor_set_layout: Option<vk::DescriptorSetLayout>,
    /// Shared descriptor pool for skeleton descriptor sets
    skeleton_descriptor_pool: vk::DescriptorPool,
    /// Flag to prevent double-free of skeleton resources
    skeleton_descriptor_destroyed: bool,
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

        Ok(Self {
            shader_cache: Rc::new(RefCell::new(ShaderCache::new(context.device.clone()))),
            context,
            storage_descriptor_layout: Some(storage_descriptor_layout),
            bindless_descriptor_layout,
            skeleton_descriptor_layout,
            compositing_descriptor_set_layout: None,
            skeleton_descriptor_pool,
            skeleton_descriptor_destroyed: false,
            ui_descriptor_layouts: Vec::new(),
        })
    }

    /// Compile a material from a shader file.
    ///
    /// If `options.color_format` is `ImageFormat::Auto`, creates a deferred material
    /// that will be compiled on-demand when first used with a specific format.
    pub(crate) fn compile(
        &mut self,
        registry: &mut crate::renderer::registry::AssetRegistry,
        shader_path: &Path,
        _material_type: MaterialType,
        options: MaterialOptions,
    ) -> Result<crate::handle::MaterialHandle, MaterialError> {
        // 1. Determine vertex binding
        let vertex_binding = self.get_vertex_binding(&options.vertex_type)?;

        // Check if this is a deferred material (Auto format)
        if options.color_format == crate::texture::ImageFormat::Auto {
            // Create deferred material - pipeline will be compiled on-demand
            let material_asset = crate::renderer::registry::MaterialAsset {
                pipeline: None,
                fully_compiled: false,
                shader_path: Some(shader_path.to_path_buf()),
                vertex_binding,
                textures: crate::renderer::registry::MaterialTextures::default(),
                material_descriptor_set: None,
                material_descriptor_layout: None,
            };
            return Ok(registry.register_material(material_asset));
        }

        // 2. Load shaders (WGSL file contains both vert and frag)
        let mut cache = self.shader_cache.borrow_mut();
        let vert_module = cache
            .load_shader(shader_path, vk::ShaderStageFlags::VERTEX)
            .map_err(|e| MaterialError::ShaderCompilation(format!("Vertex shader: {:?}", e)))?;
        let frag_module = cache
            .load_shader(shader_path, vk::ShaderStageFlags::FRAGMENT)
            .map_err(|e| MaterialError::ShaderCompilation(format!("Fragment shader: {:?}", e)))?;
        drop(cache);

        // 3. Build descriptor layouts
        let layouts = self.build_descriptor_layouts(&options)?;

        // 4. Build pipeline
        let pipeline = self.build_pipeline(
            &options,
            vert_module,
            frag_module,
            &layouts,
            &vertex_binding,
        )?;

        // 5. Register and return handle
        let material_asset = crate::renderer::registry::MaterialAsset {
            pipeline: Some(registry.register_pipeline(pipeline)),
            fully_compiled: true,
            shader_path: None,
            vertex_binding,
            textures: crate::renderer::registry::MaterialTextures::default(),
            material_descriptor_set: None,
            material_descriptor_layout: None,
        };

        Ok(registry.register_material(material_asset))
    }

    /// Compile a deferred material for a specific format.
    ///
    /// Takes a material that was created with `ImageFormat::Auto` and compiles
    /// it for the specified render target format.
    pub(crate) fn compile_deferred_material(
        &mut self,
        registry: &mut crate::renderer::registry::AssetRegistry,
        material_handle: crate::handle::MaterialHandle,
        format: crate::texture::ImageFormat,
    ) -> Result<(), MaterialError> {
        // Get the material asset (immutable borrow)
        let (shader_path, vertex_binding) = {
            let material = registry.get_material(material_handle).ok_or_else(|| {
                MaterialError::ShaderCompilation(format!(
                    "Material handle {:?} not found",
                    material_handle
                ))
            })?;

            // Check if already compiled
            if material.fully_compiled {
                return Ok(());
            }

            let shader_path = material
                .shader_path
                .as_ref()
                .ok_or_else(|| {
                    MaterialError::ShaderCompilation(
                        "Deferred material has no shader path".to_string(),
                    )
                })?
                .clone();

            (shader_path, material.vertex_binding.clone())
        };

        // Load shaders
        let mut cache = self.shader_cache.borrow_mut();
        let vert_module = cache
            .load_shader(&shader_path, vk::ShaderStageFlags::VERTEX)
            .map_err(|e| MaterialError::ShaderCompilation(format!("Vertex shader: {:?}", e)))?;
        let frag_module = cache
            .load_shader(&shader_path, vk::ShaderStageFlags::FRAGMENT)
            .map_err(|e| MaterialError::ShaderCompilation(format!("Fragment shader: {:?}", e)))?;
        drop(cache);

        // Create options with the specified format
        let options = MaterialOptions {
            color_format: format,
            ..Default::default()
        };

        // Build descriptor layouts
        let layouts = self.build_descriptor_layouts(&options)?;

        // Build pipeline
        let pipeline = self.build_pipeline(
            &options,
            vert_module,
            frag_module,
            &layouts,
            &vertex_binding,
        )?;

        // Register the pipeline
        let pipeline_handle = registry.register_pipeline(pipeline);

        // Update the material asset with the compiled pipeline
        if let Some(material) = registry.get_material_mut(material_handle) {
            material.pipeline = Some(pipeline_handle);
            material.fully_compiled = true;
        }

        Ok(())
    }

    fn get_vertex_binding(
        &self,
        vertex_type: &VertexType,
    ) -> Result<crate::vulkan::vertexbinding::VertexBinding, MaterialError> {
        use crate::vertex::VertexLayout;

        Ok(match vertex_type {
            VertexType::Pbr => {
                crate::vulkan::vertexbinding::VertexBinding::from(&VertexLayout::pbr())
            }
            VertexType::Ui => {
                crate::vulkan::vertexbinding::VertexBinding::from(&VertexLayout::ui())
            }
            VertexType::Simple => {
                crate::vulkan::vertexbinding::VertexBinding::from(&VertexLayout::position())
            }
            VertexType::Skinned => {
                crate::vulkan::vertexbinding::VertexBinding::from(&VertexLayout::pbr_skinned())
            }
        })
    }

    fn build_descriptor_layouts(
        &mut self,
        options: &MaterialOptions,
    ) -> Result<Vec<vk::DescriptorSetLayout>, MaterialError> {
        // UI materials use a completely different descriptor set layout
        if matches!(options.vertex_type, VertexType::Ui) {
            return self.build_ui_descriptor_layout();
        }

        let mut layouts = vec![
            self.storage_descriptor_layout
                .expect("Storage descriptor layout not initialized"),
            self.bindless_descriptor_layout,
        ];

        // Add skeleton layout for skinned materials
        if matches!(options.vertex_type, VertexType::Skinned) {
            layouts.push(self.skeleton_descriptor_layout);
        }

        // Add compositing descriptor set layout (set 2) for compositing materials
        if options.is_compositing
            && let Some(layout) = self.compositing_descriptor_set_layout
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
        // - Binding 3: uniforms (screen_size)
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
    }

    /// Get the skeleton descriptor layout.
    pub(crate) fn skeleton_descriptor_layout(&self) -> vk::DescriptorSetLayout {
        self.skeleton_descriptor_layout
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

    fn build_pipeline(
        &self,
        options: &MaterialOptions,
        vert_module: vk::ShaderModule,
        frag_module: vk::ShaderModule,
        layouts: &[vk::DescriptorSetLayout],
        vertex_binding: &crate::vulkan::vertexbinding::VertexBinding,
    ) -> Result<crate::vulkan::material::builder::Pipeline, MaterialError> {
        use crate::pipeline::{CullMode, FrontFace, PolygonMode};
        use crate::vulkan::material::builder::PipelineBuilder;

        let mut builder = PipelineBuilder::new(self.context.clone())
            .with_shaders(vert_module, frag_module)
            .with_vertex_binding(vertex_binding.clone())
            .with_descriptor_layouts(layouts.to_vec());

        // UI materials use different rendering configuration
        let is_ui = matches!(options.vertex_type, VertexType::Ui);

        if is_ui {
            // UI rendering: no depth buffer, SRGB color format
            builder = builder.with_rendering_formats(
                Some(crate::texture::ImageFormat::B8G8R8A8Srgb),
                None, // No depth buffer for UI
            );
        } else if options.is_compositing {
            // Compositing rendering: no depth buffer (fullscreen pass)
            builder = builder.with_rendering_formats(
                Some(options.color_format),
                None, // No depth buffer for compositing
            );
        } else {
            // Standard rendering with depth buffer
            builder = builder.with_rendering_formats(
                Some(options.color_format),
                Some(crate::texture::ImageFormat::D32SfloatS8Uint),
            );
        }

        // Configure render state from options
        // Disable depth test for UI passes and compositing passes (no depth attachment)
        if !is_ui && !options.is_compositing {
            builder = builder.with_depth_test(true, true, crate::pipeline::CompareOp::Greater);
        }

        if options.double_sided {
            builder = builder.with_cull_mode(CullMode::None, FrontFace::CounterClockwise);
        } else {
            builder = builder.with_cull_mode(CullMode::Back, FrontFace::CounterClockwise);
        }

        if options.alpha_blended {
            builder = builder.with_alpha_blending();
        }

        if options.wireframe {
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
        // Destroy storage descriptor layout (only if not already destroyed)
        if let Some(layout) = self.storage_descriptor_layout.take() {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(layout, None);
            }
        }

        // Destroy skeleton descriptor layout and pool
        // Note: We use a flag to prevent double-free since these are not Option types
        // Destroy is idempotent because skeleton_descriptor_destroyed prevents double-free
        if !self.skeleton_descriptor_destroyed {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(self.skeleton_descriptor_layout, None);
                self.context
                    .device
                    .destroy_descriptor_pool(self.skeleton_descriptor_pool, None);
            }
            self.skeleton_descriptor_destroyed = true;
        }

        // Destroy UI descriptor set layouts (created per UI material)
        // drain() removes all elements, so this is idempotent
        for layout in self.ui_descriptor_layouts.drain(..) {
            unsafe {
                self.context
                    .device
                    .destroy_descriptor_set_layout(layout, None);
            }
        }

        // Note: bindless_descriptor_layout is owned by BindlessTextureManager, not us
    }
}

impl Drop for MaterialCompiler {
    fn drop(&mut self) {
        self.destroy();
    }
}

/// Builder for materials with custom configuration.
pub struct MaterialBuilder<'a> {
    renderer: &'a mut crate::renderer::VulkanRenderer,
    shader_path: PathBuf,
    options: MaterialOptions,
}

impl<'a> MaterialBuilder<'a> {
    pub(crate) fn new(
        renderer: &'a mut crate::renderer::VulkanRenderer,
        shader_path: PathBuf,
    ) -> Self {
        Self {
            renderer,
            shader_path,
            options: MaterialOptions::default(),
        }
    }

    /// Enable alpha blending.
    pub fn alpha_blended(mut self) -> Self {
        self.options.alpha_blended = true;
        self
    }

    /// Disable backface culling.
    pub fn double_sided(mut self) -> Self {
        self.options.double_sided = true;
        self
    }

    /// Enable wireframe mode.
    pub fn wireframe(mut self) -> Self {
        self.options.wireframe = true;
        self
    }

    /// Set custom vertex type.
    pub fn with_vertex_type(mut self, vertex_type: VertexType) -> Self {
        self.options.vertex_type = vertex_type;
        self
    }

    /// Set color attachment format.
    pub fn with_color_format(mut self, format: ImageFormat) -> Self {
        self.options.color_format = format;
        self
    }

    /// Build the material.
    pub fn build(self) -> Result<crate::handle::MaterialHandle, crate::RendererError> {
        self.renderer
            .compile_material(&self.shader_path, self.options)
    }
}
