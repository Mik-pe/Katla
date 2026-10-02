//! Material compiler for compiling materials from shaders.
//!
//! This module handles the compilation of WGSL shaders into SPIR-V and
//! the creation of Vulkan graphics pipelines for rendering.

use crate::vulkan::bindless_texture::BindlessTextureManager;
use crate::vulkan::context::VulkanContext;
use crate::vulkan::material::shadermodule::ShaderCache;
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
    bindless_descriptor_layout: vk::DescriptorSetLayout,
    /// Descriptor layouts owned by compiled material variants.
    owned_descriptor_layouts: Vec<vk::DescriptorSetLayout>,
    interfaces: std::collections::HashMap<
        (String, crate::renderer::pipeline_descriptor::PipelineStages),
        crate::renderer::graphics_interface::GraphicsInterface,
    >,
}

impl MaterialCompiler {
    pub(crate) fn new(
        context: Rc<VulkanContext>,
        bindless_manager: &BindlessTextureManager,
    ) -> Self {
        let bindless_descriptor_layout = bindless_manager.descriptor_set_layout();

        Self {
            shader_cache: Rc::new(RefCell::new(ShaderCache::new(context.device.clone()))),
            context,
            bindless_descriptor_layout,
            owned_descriptor_layouts: Vec::new(),
            interfaces: Default::default(),
        }
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
            if let Err(error) = self.compile_variant(registry, handle, &key) {
                registry.remove_material(handle);
                return Err(error);
            }
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
        let previous_layouts = self.owned_descriptor_layouts.len();
        let result = self.prepare_variant(registry, material_handle, key);
        if result.is_err() {
            for layout in self.owned_descriptor_layouts.drain(previous_layouts..) {
                unsafe {
                    self.context
                        .device
                        .destroy_descriptor_set_layout(layout, None);
                }
            }
        }
        result
    }

    fn prepare_variant(
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
        let frag_module = fragment_entry
            .as_deref()
            .map(|entry| {
                cache
                    .load_shader_with_entry(&shader_path, vk::ShaderStageFlags::FRAGMENT, entry)
                    .map_err(|e| fail(format!("Fragment shader: {e:?}")))
            })
            .transpose()?;
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
        // UI materials additionally compile an instanced pipeline using
        // vs_instanced/fs_instanced entry points with UnitQuadVertex format.
        let instanced_pipeline = if descriptor.is_ui_layout() {
            Some(
                self.build_instanced_ui_pipeline(&shader_path, &layouts, key.color_format())
                    .map_err(|error| fail(error.to_string()))?,
            )
        } else {
            None
        };
        let pipeline_handle = registry.register_pipeline(pipeline);
        let instanced_pipeline =
            instanced_pipeline.map(|pipeline| registry.register_pipeline(pipeline));

        let variant = crate::renderer::registry::MaterialVariant {
            pipeline: pipeline_handle,
            instanced_pipeline,
        };

        if !registry.insert_material_variant(material_handle, key.clone(), variant) {
            registry.remove_pipeline(pipeline_handle);
            if let Some(handle) = instanced_pipeline {
                registry.remove_pipeline(handle);
            }
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
            || layout.is_empty()
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

        use crate::renderer::graphics_interface::{GraphicsBindingKind, GraphicsInterface};
        let source =
            super::shadermodule::resolved_source(std::path::Path::new(&descriptor.shader_path))
                .map_err(|error| MaterialError::ShaderCompilation(format!("{error:?}")))?;
        let interface = GraphicsInterface::reflect(&source, &descriptor.stages)
            .map_err(MaterialError::ShaderCompilation)?;
        let max_group = interface.bindings.iter().map(|binding| binding.group).max();
        let mut layouts = Vec::new();
        for group in 0..max_group.map_or(0, |group| group + 1) {
            if group == 1
                && interface
                    .bindings
                    .iter()
                    .any(|binding| binding.group == 1 && binding.array)
            {
                layouts.push(self.bindless_descriptor_layout);
                continue;
            }
            let bindings: Vec<_> = interface
                .bindings
                .iter()
                .filter(|binding| binding.group == group)
                .map(|binding| {
                    let kind = match binding.kind {
                        GraphicsBindingKind::Buffer {
                            usage: crate::render_graph::BufferUsage::Uniform,
                            ..
                        } => vk::DescriptorType::UNIFORM_BUFFER,
                        GraphicsBindingKind::Buffer { .. } => vk::DescriptorType::STORAGE_BUFFER,
                        GraphicsBindingKind::Image { storage: false } => {
                            vk::DescriptorType::SAMPLED_IMAGE
                        }
                        GraphicsBindingKind::Image { storage: true } => {
                            vk::DescriptorType::STORAGE_IMAGE
                        }
                        GraphicsBindingKind::Sampler { .. } => vk::DescriptorType::SAMPLER,
                    };
                    let mut stages = vk::ShaderStageFlags::empty();
                    if binding.stages.vertex {
                        stages |= vk::ShaderStageFlags::VERTEX;
                    }
                    if binding.stages.fragment {
                        stages |= vk::ShaderStageFlags::FRAGMENT;
                    }
                    vk::DescriptorSetLayoutBinding::default()
                        .binding(binding.binding)
                        .descriptor_type(kind)
                        .descriptor_count(1)
                        .stage_flags(stages)
                })
                .collect();
            let flags = vk::DescriptorSetLayoutCreateFlags::empty();
            let layout = unsafe {
                self.context.device.create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default()
                        .flags(flags)
                        .bindings(&bindings),
                    None,
                )
            }
            .map_err(|error| {
                MaterialError::PipelineCreation(format!("Reflected descriptor layout: {error}"))
            })?;
            self.owned_descriptor_layouts.push(layout);
            layouts.push(layout);
        }
        self.interfaces.insert(
            (descriptor.shader_path.clone(), descriptor.stages.clone()),
            interface,
        );
        Ok(layouts)
    }

    pub(crate) fn interface(
        &self,
        descriptor: &crate::renderer::pipeline_descriptor::PipelineDescriptor,
    ) -> Option<&crate::renderer::graphics_interface::GraphicsInterface> {
        self.interfaces
            .get(&(descriptor.shader_path.clone(), descriptor.stages.clone()))
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
        self.owned_descriptor_layouts.push(ui_layout);

        // Return both layouts: Set 0 (UI resources) and Set 1 (bindless textures)
        Ok(vec![ui_layout, self.bindless_descriptor_layout])
    }

    /// Invalidate cached shader modules for the given path.
    pub(crate) fn invalidate_shader_cache(&self, path: &Path) {
        self.shader_cache.borrow_mut().invalidate(path);
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
        frag_module: Option<vk::ShaderModule>,
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
        let fragment_entry = std::ffi::CString::new(fragment_entry.as_deref().unwrap_or("fs_main"))
            .map_err(|e| {
                MaterialError::PipelineCreation(format!("invalid fragment entry point: {e}"))
            })?;

        // SOA vertex bindings for non-UI materials (separate per-attribute buffers)
        // UI materials keep interleaved binding for dynamic mesh updates
        let mut builder = if is_ui {
            PipelineBuilder::new(self.context.clone())
                .with_optional_fragment(vert_module, frag_module)
                .with_entry_points(vertex_entry.as_c_str(), fragment_entry.as_c_str())
                .with_vertex_binding(vertex_binding.clone())
                .with_descriptor_layouts(layouts.to_vec())
        } else {
            PipelineBuilder::new(self.context.clone())
                .with_optional_fragment(vert_module, frag_module)
                .with_entry_points(vertex_entry.as_c_str(), fragment_entry.as_c_str())
                .with_vertex_binding_soa(vertex_binding.clone())
                .with_descriptor_layouts(layouts.to_vec())
        };

        // Attachments come from the variant key: the resolved color format
        // plus the shared depth derivation (None for UI, compositing, and
        // depth-test-disabled pipelines).
        builder = builder
            .with_rendering_formats(
                descriptor
                    .color_attachment
                    .then_some(descriptor.color_format),
                depth_format,
            )
            .with_depth_bias(
                descriptor.depth_bias.constant,
                descriptor.depth_bias.slope_factor,
                descriptor.depth_bias.clamp,
            )
            .with_color_write_mask(vk::ColorComponentFlags::from_raw(u32::from(
                descriptor.color_write_mask.0,
            )));

        // A depth-free pipeline normalises to no test/write and Always so
        // every entry path shares the same effective state.
        if depth_format.is_some() {
            builder = builder.with_depth_test(
                descriptor.depth.test,
                descriptor.depth.write,
                descriptor.depth.compare,
            );
        } else {
            builder = builder.with_depth_test(false, false, crate::pipeline::CompareOp::Always);
        }

        builder = builder.with_cull_mode(
            descriptor.cull,
            crate::pipeline::FrontFace::CounterClockwise,
        );

        if let Some(stencil) = descriptor.stencil {
            let face = |state: crate::renderer::pipeline_descriptor::StencilFaceState| {
                vk::StencilOpState {
                    fail_op: stencil_op(state.fail),
                    pass_op: stencil_op(state.pass),
                    depth_fail_op: stencil_op(state.depth_fail),
                    compare_op: state.compare.into(),
                    compare_mask: stencil.read_mask,
                    write_mask: stencil.write_mask,
                    reference: stencil.reference,
                }
            };
            builder = builder.with_stencil_test(face(stencil.front), face(stencil.back));
        }

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
        for layout in self.owned_descriptor_layouts.drain(..) {
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

fn stencil_op(operation: crate::renderer::pipeline_descriptor::StencilOperation) -> vk::StencilOp {
    use crate::renderer::pipeline_descriptor::StencilOperation::*;
    match operation {
        Keep => vk::StencilOp::KEEP,
        Zero => vk::StencilOp::ZERO,
        Replace => vk::StencilOp::REPLACE,
        IncrementClamp => vk::StencilOp::INCREMENT_AND_CLAMP,
        DecrementClamp => vk::StencilOp::DECREMENT_AND_CLAMP,
        Invert => vk::StencilOp::INVERT,
        IncrementWrap => vk::StencilOp::INCREMENT_AND_WRAP,
        DecrementWrap => vk::StencilOp::DECREMENT_AND_WRAP,
    }
}

#[cfg(test)]
mod failure_tests {
    use crate::texture::ImageFormat;
    use crate::{GpuRenderer, PipelineDescriptor, ValidationMode, VulkanRenderer};

    #[test]
    #[ignore = "requires a Vulkan device"]
    fn test_failed_initial_and_instanced_material_compilation_releases_resources() {
        let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .try_init();
        let mut renderer = VulkanRenderer::init_headless(
            8,
            8,
            ValidationMode::Enabled,
            c"material-failure".into(),
            c"Katla".into(),
        )
        .unwrap();
        let materials = renderer.asset_registry.material_count();
        let layouts = renderer.material_compiler.owned_descriptor_layouts.len();
        let missing = PipelineDescriptor::pbr("/does-not-exist/material.wgsl")
            .with_color_format(ImageFormat::R16G16B16A16Sfloat);
        assert!(renderer.compile_material(&missing).is_err());
        assert_eq!(renderer.asset_registry.material_count(), materials);
        assert_eq!(
            renderer.material_compiler.owned_descriptor_layouts.len(),
            layouts
        );

        let path =
            std::env::temp_dir().join(format!("katla-ui-failure-{}.wgsl", std::process::id()));
        let source = include_str!("../../../../resources/shaders/ui/ui.wgsl")
            .replace("fn vs_instanced(", "fn missing_instanced(");
        std::fs::write(&path, source).unwrap();
        let descriptor = PipelineDescriptor::ui(path.to_string_lossy().into_owned())
            .with_color_format(ImageFormat::B8G8R8A8Srgb);
        assert!(renderer.compile_material(&descriptor).is_err());
        assert_eq!(renderer.asset_registry.material_count(), materials);
        assert_eq!(
            renderer.material_compiler.owned_descriptor_layouts.len(),
            layouts
        );
        std::fs::remove_file(path).unwrap();
        renderer.destroy();
    }
}
