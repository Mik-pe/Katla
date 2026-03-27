use crate::RendererError;
use crate::handle::PipelineHandle;
use ash::vk;
use log::info;

#[derive(Default)]
/// Bundles all depth prepass state from VulkanRenderer.
pub(crate) struct DepthPrepassState {
    /// Depth-only pipeline for camera-space depth rendering
    pub pipeline: Option<PipelineHandle>,
    /// Depth-only pipeline for skinned meshes
    pub pipeline_skinned: Option<PipelineHandle>,
}

impl super::VulkanRenderer {
    /// Initialize the depth prepass pipeline.
    ///
    /// Creates a depth-only pipeline with color write disabled that renders
    /// from the camera's perspective. Used before the main geometry pass.
    pub fn init_depth_prepass_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        use crate::pipeline::{CullMode, FrontFace};
        use crate::vulkan::material::builder::PipelineBuilder;
        use crate::vulkan::vertexbinding::VertexFormat;

        let mut cache = self.material_compiler.shader_cache.borrow_mut();
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

        let storage_layout = self.storage_descriptor_sets[0].layout();

        let pipeline = PipelineBuilder::new(self.context.clone())
            .with_shaders(vert_module, frag_module)
            .with_descriptor_layouts(vec![storage_layout])
            .with_soa_attribute(0, VertexFormat::RGB32f) // position
            .with_depth_test(true, true, crate::pipeline::CompareOp::Greater)
            .with_cull_mode(CullMode::Back, FrontFace::CounterClockwise)
            .with_rendering_formats(
                Some(crate::texture::ImageFormat::R32Uint),
                Some(crate::texture::ImageFormat::D32SfloatS8Uint),
            )
            .build_dynamic()
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to build depth prepass pipeline: {:?}",
                    e
                ))
            })?;

        let pipeline_handle = self.asset_registry.register_pipeline(pipeline);
        self.depth_prepass.pipeline = Some(pipeline_handle);

        info!("Depth prepass pipeline initialized (reverse-Z, back-face culled)");
        Ok(())
    }

    /// Get the depth prepass pipeline handle.
    pub fn depth_prepass_pipeline(&self) -> Option<PipelineHandle> {
        self.depth_prepass.pipeline
    }

    /// Initialize the skinned depth prepass pipeline.
    ///
    /// Same as the regular depth prepass but uses the skinned vertex layout
    /// (includes joint indices/weights) and binds skeleton joint matrices at Set 2.
    pub fn init_depth_prepass_skinned_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        use crate::pipeline::{CullMode, FrontFace};
        use crate::vulkan::material::builder::PipelineBuilder;
        use crate::vulkan::vertexbinding::VertexFormat;

        let mut cache = self.material_compiler.shader_cache.borrow_mut();
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

        let storage_layout = self.storage_descriptor_sets[0].layout();
        let skeleton_layout = self.material_compiler.skeleton_descriptor_layout();

        let pipeline = PipelineBuilder::new(self.context.clone())
            .with_shaders(vert_module, frag_module)
            .with_descriptor_layouts(vec![
                storage_layout,
                self.shared_empty_descriptor_layout,
                skeleton_layout,
            ])
            .with_soa_attribute(0, VertexFormat::RGB32f) // position
            .with_soa_attribute(4, VertexFormat::RGBA16u) // joint_indices
            .with_soa_attribute(5, VertexFormat::RGBA32f) // joint_weights
            .with_depth_test(true, true, crate::pipeline::CompareOp::Greater)
            .with_cull_mode(CullMode::Back, FrontFace::CounterClockwise)
            .with_rendering_formats(
                Some(crate::texture::ImageFormat::R32Uint),
                Some(crate::texture::ImageFormat::D32SfloatS8Uint),
            )
            .build_dynamic()
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to build skinned depth prepass pipeline: {:?}",
                    e
                ))
            })?;

        let pipeline_handle = self.asset_registry.register_pipeline(pipeline);
        self.depth_prepass.pipeline_skinned = Some(pipeline_handle);

        info!("Skinned depth prepass pipeline initialized (reverse-Z, back-face culled)");
        Ok(())
    }

    /// Get the skinned depth prepass pipeline handle.
    pub fn depth_prepass_skinned_pipeline(&self) -> Option<PipelineHandle> {
        self.depth_prepass.pipeline_skinned
    }
}
