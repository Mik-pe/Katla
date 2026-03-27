use crate::RendererError;
use ash::vk;
use log::warn;

impl super::VulkanRenderer {
    /// Initialize GPU animation pose evaluation pipeline.
    ///
    /// Loads the compute shader and creates `PoseComputePipeline` and
    /// `PoseComputeBuffers` for GPU-driven skeletal animation.
    pub fn init_animation_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        // Load animation compute shader
        match self
            .material_compiler
            .shader_cache
            .borrow_mut()
            .load_shader(shader_path, vk::ShaderStageFlags::COMPUTE)
        {
            Ok(shader_module) => {
                let context = self.context.clone();

                // Create pipeline and buffers
                let mut pipeline = crate::animation::PoseComputePipeline::new(context.clone());
                let buffers = crate::animation::PoseComputeBuffers::new(context);

                let shader_module_wrapper = crate::sync::VkShaderModule(shader_module);
                pipeline
                    .initialize(&mut self.asset_registry, shader_module_wrapper)
                    .map_err(|e| {
                        RendererError::InitializationFailed(format!(
                            "Failed to initialize animation pose compute pipeline: {}",
                            e
                        ))
                    })?;

                self.animation_pipeline = Some(pipeline);
                self.animation_buffers = Some(buffers);

                Ok(())
            }
            Err(e) => {
                warn!("Failed to load animation compute shader: {}", e);
                Err(RendererError::InitializationFailed(format!(
                    "Failed to load animation compute shader: {}",
                    e
                )))
            }
        }
    }

    /// Get the animation compute pipeline handle for frame graph registration.
    pub fn animation_pipeline_handle(&self) -> Option<crate::handle::PipelineHandle> {
        self.animation_pipeline
            .as_ref()
            .and_then(|p| p.pipeline_handle())
    }
}
