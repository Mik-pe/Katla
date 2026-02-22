//! Descriptor set layout builder for creating descriptor layouts without raw vk types.
//!
//! This module provides a builder pattern for creating descriptor set layouts,
//! abstracting away the raw `ash::vk` types.

use ash::vk;

use crate::render_graph::types::ShaderStages;
use crate::sync::VkDescriptorSetLayout;
use crate::vulkan::context::VulkanContext;
use crate::vulkan::pipeline_state::DescriptorType;

/// A single descriptor binding in a descriptor set layout.
#[derive(Debug, Clone)]
pub struct DescriptorBinding {
    /// Binding number.
    pub binding: u32,
    /// Descriptor type.
    pub descriptor_type: DescriptorType,
    /// Number of descriptors in this binding.
    pub descriptor_count: u32,
    /// Shader stages that can access this binding.
    pub shader_stages: ShaderStages,
}

impl DescriptorBinding {
    /// Create a new descriptor binding.
    pub fn new(
        binding: u32,
        descriptor_type: DescriptorType,
        shader_stages: ShaderStages,
    ) -> Self {
        Self {
            binding,
            descriptor_type,
            descriptor_count: 1,
            shader_stages,
        }
    }

    /// Set the descriptor count (for array bindings).
    pub fn with_count(mut self, count: u32) -> Self {
        self.descriptor_count = count;
        self
    }

    /// Convert to Vulkan vk::DescriptorSetLayoutBinding.
    pub fn into_vk(self) -> vk::DescriptorSetLayoutBinding<'static> {
        vk::DescriptorSetLayoutBinding::default()
            .binding(self.binding)
            .descriptor_type(self.descriptor_type.into())
            .descriptor_count(self.descriptor_count)
            .stage_flags(self.shader_stages.into())
    }
}

/// Builder for creating descriptor set layouts.
#[derive(Debug, Clone, Default)]
pub struct DescriptorSetLayoutBuilder {
    bindings: Vec<DescriptorBinding>,
    push_descriptor: bool,
}

impl DescriptorSetLayoutBuilder {
    /// Create a new descriptor set layout builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a binding to the descriptor set layout.
    pub fn add_binding(
        mut self,
        binding: u32,
        descriptor_type: DescriptorType,
        shader_stages: ShaderStages,
    ) -> Self {
        self.bindings.push(DescriptorBinding::new(binding, descriptor_type, shader_stages));
        self
    }

    /// Add a binding with a custom descriptor count.
    pub fn add_binding_with_count(
        mut self,
        binding: u32,
        descriptor_type: DescriptorType,
        descriptor_count: u32,
        shader_stages: ShaderStages,
    ) -> Self {
        self.bindings.push(
            DescriptorBinding::new(binding, descriptor_type, shader_stages)
                .with_count(descriptor_count),
        );
        self
    }

    /// Add a pre-built descriptor binding.
    pub fn add_descriptor_binding(mut self, binding: DescriptorBinding) -> Self {
        self.bindings.push(binding);
        self
    }

    /// Enable push descriptor mode for this layout.
    ///
    /// Push descriptors don't require descriptor set allocation - they're
    /// updated via vkCmdPushDescriptorSetKHR during command buffer recording.
    pub fn with_push_descriptor(mut self, enabled: bool) -> Self {
        self.push_descriptor = enabled;
        self
    }

    /// Build the descriptor set layout.
    pub fn build(self, context: &VulkanContext) -> Result<VkDescriptorSetLayout, vk::Result> {
        let vk_bindings: Vec<vk::DescriptorSetLayoutBinding> =
            self.bindings.into_iter().map(|b| b.into_vk()).collect();

        let mut flags = vk::DescriptorSetLayoutCreateFlags::empty();
        if self.push_descriptor {
            flags |= vk::DescriptorSetLayoutCreateFlags::PUSH_DESCRIPTOR_KHR;
        }

        let create_info = vk::DescriptorSetLayoutCreateInfo::default()
            .flags(flags)
            .bindings(&vk_bindings);

        let layout = unsafe {
            context.device.create_descriptor_set_layout(&create_info, None)?
        };

        Ok(VkDescriptorSetLayout::new(layout))
    }
}

//=============================================================================
// Tests
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_graph::types::ShaderStages;
    use crate::vulkan::pipeline_state::DescriptorType;

    #[test]
    fn test_descriptor_binding_creation() {
        let binding = DescriptorBinding::new(0, DescriptorType::UniformBuffer, ShaderStages::VERTEX);
        assert_eq!(binding.binding, 0);
        assert_eq!(binding.descriptor_count, 1);
    }

    #[test]
    fn test_descriptor_binding_with_count() {
        let binding = DescriptorBinding::new(0, DescriptorType::UniformBuffer, ShaderStages::VERTEX)
            .with_count(10);
        assert_eq!(binding.descriptor_count, 10);
    }

    #[test]
    fn test_descriptor_binding_into_vk() {
        let binding = DescriptorBinding::new(0, DescriptorType::StorageBuffer, ShaderStages::COMPUTE);
        let vk_binding = binding.into_vk();
        assert_eq!(vk_binding.binding, 0);
        assert_eq!(vk_binding.descriptor_type, vk::DescriptorType::STORAGE_BUFFER);
        assert_eq!(vk_binding.stage_flags, vk::ShaderStageFlags::COMPUTE);
    }

    #[test]
    fn test_descriptor_set_layout_builder() {
        let builder = DescriptorSetLayoutBuilder::new()
            .add_binding(0, DescriptorType::UniformBuffer, ShaderStages::VERTEX)
            .add_binding(1, DescriptorType::CombinedImageSampler, ShaderStages::FRAGMENT);

        assert_eq!(builder.bindings.len(), 2);
        assert_eq!(builder.bindings[0].binding, 0);
        assert_eq!(builder.bindings[1].binding, 1);
    }
}
