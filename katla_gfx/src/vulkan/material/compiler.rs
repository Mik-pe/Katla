//! Material compiler for compiling materials from shaders.
//!
//! This module handles the compilation of WGSL shaders into SPIR-V and
//! the creation of Vulkan graphics pipelines for rendering.

use crate::StorageDescriptorSet;
use crate::vulkan::bindless_texture::BindlessTextureManager;
use crate::vulkan::context::VulkanContext;
use crate::vulkan::material::shadermodule::ShaderCache;
use ash::vk;
use std::{cell::RefCell, path::Path, path::PathBuf, rc::Rc};

/// Error types for material compilation.
#[derive(Debug)]
pub enum MaterialError {
    ShaderNotFound(PathBuf),
    ShaderCompilation(String),
    VertexBindingRequired(String),
    PipelineCreation(String),
}

impl std::fmt::Display for MaterialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ShaderNotFound(p) => write!(f, "Shader not found: {:?}", p),
            Self::ShaderCompilation(s) => write!(f, "Shader compilation failed: {}", s),
            Self::VertexBindingRequired(s) => write!(f, "Vertex binding required: {}", s),
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
}

/// Options for material creation.
#[derive(Clone, Debug)]
pub struct MaterialOptions {
    pub alpha_blended: bool,
    pub double_sided: bool,
    pub wireframe: bool,
    pub vertex_type: VertexType,
}

impl Default for MaterialOptions {
    fn default() -> Self {
        Self {
            alpha_blended: false,
            double_sided: false,
            wireframe: false,
            vertex_type: VertexType::Pbr,
        }
    }
}

/// Compiles material definitions into Vulkan pipelines.
pub(crate) struct MaterialCompiler {
    shader_cache: Rc<RefCell<ShaderCache>>,
    context: Rc<VulkanContext>,
    storage_descriptor_layout: Option<vk::DescriptorSetLayout>,
    bindless_descriptor_layout: vk::DescriptorSetLayout,
}

impl MaterialCompiler {
    pub(crate) fn new(
        context: Rc<VulkanContext>,
        bindless_manager: &BindlessTextureManager,
        storage_descriptor_set: &StorageDescriptorSet,
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

        Ok(Self {
            shader_cache: Rc::new(RefCell::new(ShaderCache::new(context.device.clone()))),
            context,
            storage_descriptor_layout: Some(storage_descriptor_layout),
            bindless_descriptor_layout,
        })
    }

    /// Compile a material from a shader file.
    pub(crate) fn compile(
        &mut self,
        registry: &mut crate::renderer::registry::AssetRegistry,
        shader_path: &Path,
        material_type: MaterialType,
        options: MaterialOptions,
    ) -> Result<crate::handle::MaterialHandle, MaterialError> {
        // 1. Determine vertex binding
        let vertex_binding = self.get_vertex_binding(&options.vertex_type)?;

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
            pipeline: registry.register_pipeline(pipeline),
            vertex_binding,
            material_data: crate::renderer::registry::MaterialData::default(), // Default material params
            material_descriptor_set: None,
            material_descriptor_layout: None,
        };

        Ok(registry.register_material(material_asset))
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
        })
    }

    fn build_descriptor_layouts(
        &self,
        _options: &MaterialOptions,
    ) -> Result<Vec<vk::DescriptorSetLayout>, MaterialError> {
        Ok(vec![
            self.storage_descriptor_layout
                .expect("Storage descriptor layout not initialized"),
            self.bindless_descriptor_layout,
        ])
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
            .with_descriptor_layouts(layouts.to_vec())
            // Note: No push constants - WGSL shaders use instance_index instead
            // TODO: Use offscreen HDR format once render graph supports intermediate textures
            // For now, render directly to swapchain format
            .with_rendering_formats(
                Some(crate::texture::ImageFormat::B8G8R8A8Srgb),
                Some(crate::texture::ImageFormat::D32SfloatS8Uint),
            );

        // Configure render state from options
        builder = builder.with_depth_test(true, true, crate::pipeline::CompareOp::Greater);

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

    /// Clean up descriptor layouts.
    pub(crate) fn destroy(&mut self) {
        if let Some(layout) = self.storage_descriptor_layout.take() {
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

    /// Build the material.
    pub fn build(self) -> Result<crate::handle::MaterialHandle, crate::RendererError> {
        self.renderer
            .material_compiler
            .compile(
                &mut self.renderer.asset_registry,
                &self.shader_path,
                MaterialType::Auto,
                self.options,
            )
            .map_err(|e| {
                crate::RendererError::InitializationFailed(format!(
                    "Material compilation failed: {}",
                    e
                ))
            })
    }
}
