use crate::RendererError;
use crate::handle::PipelineHandle;
use crate::pipeline::{CompareOp, CullMode, FrontFace};
use crate::renderer::registry::AssetRegistry;
use crate::texture::ImageFormat;
use crate::vulkan::context::VulkanContext;
use crate::vulkan::material::builder::PipelineBuilder;
use crate::vulkan::material::compiler::MaterialCompiler;
use crate::vulkan::material::storage_uniform::StorageDescriptorSet;
use crate::vulkan::vertexbinding::VertexFormat;
use ash::vk;
use log::info;
use std::rc::Rc;

#[derive(Default)]
/// Owns all depth prepass GPU state.
///
/// Lifecycle:
/// - `init_depth_prepass_pipeline()` — creates the depth-only pipeline
/// - `init_depth_prepass_skinned_pipeline()` — creates the skinned depth-only pipeline
/// - `init_depth_prepass_billboard_pipeline()` — creates the billboard depth pipeline
/// - `destroy()` — no GPU resources to clean up (pipelines owned by AssetRegistry)
pub(crate) struct DepthPrepassSubsystem {
    /// Depth-only pipeline for camera-space depth rendering.
    pub pipeline: Option<PipelineHandle>,
    /// Depth-only pipeline for skinned meshes.
    pub pipeline_skinned: Option<PipelineHandle>,
    /// Depth-only pipeline for billboard entities (camera-facing quads with alpha discard).
    pub pipeline_billboard: Option<PipelineHandle>,
}

/// Dependencies needed from VulkanRenderer for depth prepass initialization.
pub(crate) struct DepthPrepassInitContext<'a> {
    pub context: &'a Rc<VulkanContext>,
    pub material_compiler: &'a mut MaterialCompiler,
    pub storage_descriptor_set: &'a StorageDescriptorSet,
    pub shared_empty_descriptor_layout: vk::DescriptorSetLayout,
    pub bindless_descriptor_layout: vk::DescriptorSetLayout,
    pub asset_registry: &'a mut AssetRegistry,
}

impl DepthPrepassSubsystem {
    /// Initialize the depth prepass pipeline.
    ///
    /// Creates a depth-only pipeline with color write disabled that renders
    /// from the camera's perspective. Used before the main geometry pass.
    pub fn init_depth_prepass_pipeline(
        &mut self,
        ctx: &mut DepthPrepassInitContext,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let mut cache = ctx.material_compiler.shader_cache.borrow_mut();
        let vert_module = cache
            .load_shader(shader_path, vk::ShaderStageFlags::VERTEX)
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to load depth prepass vertex shader: {:?}",
                    e
                ))
            })?;
        let frag_module = cache
            .load_shader(shader_path, vk::ShaderStageFlags::FRAGMENT)
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to load depth prepass fragment shader: {:?}",
                    e
                ))
            })?;
        drop(cache);

        let storage_layout = ctx.storage_descriptor_set.layout();

        let pipeline = PipelineBuilder::new(ctx.context.clone())
            .with_shaders(vert_module, frag_module)
            .with_descriptor_layouts(vec![storage_layout])
            .with_soa_attribute(0, VertexFormat::RGB32f) // position
            .with_depth_test(true, true, CompareOp::Greater)
            .with_cull_mode(CullMode::Back, FrontFace::CounterClockwise)
            .with_rendering_formats(
                Some(ImageFormat::R32Uint),
                Some(ImageFormat::D32SfloatS8Uint),
            )
            .build_dynamic()
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to build depth prepass pipeline: {:?}",
                    e
                ))
            })?;

        let pipeline_handle = ctx.asset_registry.register_pipeline(pipeline);
        self.pipeline = Some(pipeline_handle);

        info!("Depth prepass pipeline initialized (reverse-Z, back-face culled)");
        Ok(())
    }

    /// Get the depth prepass pipeline handle.
    pub fn depth_prepass_pipeline(&self) -> Option<PipelineHandle> {
        self.pipeline
    }

    /// Initialize the skinned depth prepass pipeline.
    ///
    /// Same as the regular depth prepass but uses the skinned vertex layout
    /// (includes joint indices/weights) and binds skeleton joint matrices at Set 2.
    pub fn init_depth_prepass_skinned_pipeline(
        &mut self,
        ctx: &mut DepthPrepassInitContext,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let mut cache = ctx.material_compiler.shader_cache.borrow_mut();
        let vert_module = cache
            .load_shader(shader_path, vk::ShaderStageFlags::VERTEX)
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to load skinned depth prepass vertex shader: {:?}",
                    e
                ))
            })?;
        let frag_module = cache
            .load_shader(shader_path, vk::ShaderStageFlags::FRAGMENT)
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to load skinned depth prepass fragment shader: {:?}",
                    e
                ))
            })?;
        drop(cache);

        let storage_layout = ctx.storage_descriptor_set.layout();
        let skeleton_layout = ctx.material_compiler.skeleton_descriptor_layout();

        let pipeline = PipelineBuilder::new(ctx.context.clone())
            .with_shaders(vert_module, frag_module)
            .with_descriptor_layouts(vec![
                storage_layout,
                ctx.shared_empty_descriptor_layout,
                skeleton_layout,
            ])
            .with_soa_attribute(0, VertexFormat::RGB32f) // position
            .with_soa_attribute(4, VertexFormat::RGBA16u) // joint_indices
            .with_soa_attribute(5, VertexFormat::RGBA32f) // joint_weights
            .with_depth_test(true, true, CompareOp::Greater)
            .with_cull_mode(CullMode::Back, FrontFace::CounterClockwise)
            .with_rendering_formats(
                Some(ImageFormat::R32Uint),
                Some(ImageFormat::D32SfloatS8Uint),
            )
            .build_dynamic()
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to build skinned depth prepass pipeline: {:?}",
                    e
                ))
            })?;

        let pipeline_handle = ctx.asset_registry.register_pipeline(pipeline);
        self.pipeline_skinned = Some(pipeline_handle);

        info!("Skinned depth prepass pipeline initialized (reverse-Z, back-face culled)");
        Ok(())
    }

    /// Get the skinned depth prepass pipeline handle.
    pub fn depth_prepass_skinned_pipeline(&self) -> Option<PipelineHandle> {
        self.pipeline_skinned
    }

    /// Initialize the billboard depth prepass pipeline.
    ///
    /// Uses the PBR vertex layout (position, normal, tangent, uv) with camera-facing
    /// vertex transform. Includes Set 0 (storage) and Set 1 (bindless textures) for
    /// alpha-discarded picking. Double-sided, reverse-Z, R32Uint + D32SfloatS8Uint output.
    pub fn init_depth_prepass_billboard_pipeline(
        &mut self,
        ctx: &mut DepthPrepassInitContext,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let mut cache = ctx.material_compiler.shader_cache.borrow_mut();
        let vert_module = cache
            .load_shader(shader_path, vk::ShaderStageFlags::VERTEX)
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to load billboard depth prepass vertex shader: {:?}",
                    e
                ))
            })?;
        let frag_module = cache
            .load_shader(shader_path, vk::ShaderStageFlags::FRAGMENT)
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to load billboard depth prepass fragment shader: {:?}",
                    e
                ))
            })?;
        drop(cache);

        let storage_layout = ctx.storage_descriptor_set.layout();

        let pipeline = PipelineBuilder::new(ctx.context.clone())
            .with_shaders(vert_module, frag_module)
            .with_descriptor_layouts(vec![storage_layout, ctx.bindless_descriptor_layout])
            .with_soa_attribute(0, VertexFormat::RGB32f) // position
            .with_soa_attribute(1, VertexFormat::RGB32f) // normal
            .with_soa_attribute(2, VertexFormat::RGBA32f) // tangent
            .with_soa_attribute(3, VertexFormat::RG32f) // uv
            .with_depth_test(true, true, CompareOp::Greater)
            .with_cull_mode(CullMode::None, FrontFace::CounterClockwise)
            .with_rendering_formats(
                Some(ImageFormat::R32Uint),
                Some(ImageFormat::D32SfloatS8Uint),
            )
            .build_dynamic()
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to build billboard depth prepass pipeline: {:?}",
                    e
                ))
            })?;

        let pipeline_handle = ctx.asset_registry.register_pipeline(pipeline);
        self.pipeline_billboard = Some(pipeline_handle);

        info!(
            "Billboard depth prepass pipeline initialized (reverse-Z, double-sided, Set 0 + Set 1)"
        );
        Ok(())
    }

    /// Get the billboard depth prepass pipeline handle.
    pub fn depth_prepass_billboard_pipeline(&self) -> Option<PipelineHandle> {
        self.pipeline_billboard
    }

    /// Destroy depth prepass resources.
    ///
    /// Pipelines are owned by the AssetRegistry, so no GPU cleanup is needed here.
    pub fn destroy(&mut self) {
        self.pipeline = None;
        self.pipeline_skinned = None;
        self.pipeline_billboard = None;
    }
}

impl super::VulkanRenderer {
    /// Initialize the depth prepass pipeline.
    ///
    /// Delegates to [`DepthPrepassSubsystem::init_depth_prepass_pipeline`].
    pub fn init_depth_prepass_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let mut ctx = super::depth_prepass::DepthPrepassInitContext {
            context: &self.context,
            material_compiler: &mut self.material_compiler,
            storage_descriptor_set: &self.storage_descriptor_sets[0],
            shared_empty_descriptor_layout: self.shared_empty_descriptor_layout,
            bindless_descriptor_layout: self.bindless_manager.descriptor_set_layout(),
            asset_registry: &mut self.asset_registry,
        };
        self.depth_prepass
            .init_depth_prepass_pipeline(&mut ctx, shader_path)
    }

    /// Get the depth prepass pipeline handle.
    ///
    /// Delegates to [`DepthPrepassSubsystem::depth_prepass_pipeline`].
    pub fn depth_prepass_pipeline(&self) -> Option<PipelineHandle> {
        self.depth_prepass.depth_prepass_pipeline()
    }

    /// Initialize the skinned depth prepass pipeline.
    ///
    /// Delegates to [`DepthPrepassSubsystem::init_depth_prepass_skinned_pipeline`].
    pub fn init_depth_prepass_skinned_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let mut ctx = super::depth_prepass::DepthPrepassInitContext {
            context: &self.context,
            material_compiler: &mut self.material_compiler,
            storage_descriptor_set: &self.storage_descriptor_sets[0],
            shared_empty_descriptor_layout: self.shared_empty_descriptor_layout,
            bindless_descriptor_layout: self.bindless_manager.descriptor_set_layout(),
            asset_registry: &mut self.asset_registry,
        };
        self.depth_prepass
            .init_depth_prepass_skinned_pipeline(&mut ctx, shader_path)
    }

    /// Get the skinned depth prepass pipeline handle.
    ///
    /// Delegates to [`DepthPrepassSubsystem::depth_prepass_skinned_pipeline`].
    pub fn depth_prepass_skinned_pipeline(&self) -> Option<PipelineHandle> {
        self.depth_prepass.depth_prepass_skinned_pipeline()
    }

    /// Initialize the billboard depth prepass pipeline.
    ///
    /// Delegates to [`DepthPrepassSubsystem::init_depth_prepass_billboard_pipeline`].
    pub fn init_depth_prepass_billboard_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let mut ctx = super::depth_prepass::DepthPrepassInitContext {
            context: &self.context,
            material_compiler: &mut self.material_compiler,
            storage_descriptor_set: &self.storage_descriptor_sets[0],
            shared_empty_descriptor_layout: self.shared_empty_descriptor_layout,
            bindless_descriptor_layout: self.bindless_manager.descriptor_set_layout(),
            asset_registry: &mut self.asset_registry,
        };
        self.depth_prepass
            .init_depth_prepass_billboard_pipeline(&mut ctx, shader_path)
    }

    /// Get the billboard depth prepass pipeline handle.
    ///
    /// Delegates to [`DepthPrepassSubsystem::depth_prepass_billboard_pipeline`].
    pub fn depth_prepass_billboard_pipeline(&self) -> Option<PipelineHandle> {
        self.depth_prepass.depth_prepass_billboard_pipeline()
    }
}
