use super::*;

impl VulkanRenderer {
    /// Get the compositing descriptor set layout for compiling compositing materials.
    ///
    /// This layout is used when creating the pipeline layout for compositing materials.
    /// It must be set in the material compiler before compiling the compositing shader.
    pub fn compositing_descriptor_set_layout(&self) -> vk::DescriptorSetLayout {
        self.compositing_descriptor_set_layout
    }

    /// Set the compositing descriptor set layout in the material compiler.
    ///
    /// This must be called before compiling a compositing material to ensure
    /// the pipeline layout includes descriptor set 2.
    pub fn set_compositing_descriptor_set_layout(&mut self) {
        self.material_compiler
            .set_compositing_descriptor_set_layout(self.compositing_descriptor_set_layout);
    }

    /// Clear the compositing descriptor set layout from the material compiler.
    ///
    /// This should be called after compiling the compositing material.
    pub fn clear_compositing_descriptor_set_layout(&mut self) {
        self.material_compiler
            .clear_compositing_descriptor_set_layout();
    }
}
