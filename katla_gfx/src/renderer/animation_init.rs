use crate::RendererError;

impl super::VulkanRenderer {
    /// Initialize animation buffer ownership; pipelines belong to graph commands.
    pub fn init_animation_pipeline(
        &mut self,
        _shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        self.animation_buffers = Some(crate::animation::PoseComputeBuffers::new(
            self.context.clone(),
        ));
        Ok(())
    }
}
