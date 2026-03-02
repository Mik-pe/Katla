//! Descriptor set layout builder for creating descriptor layouts without raw vk types.
//!
//! This module provides a builder pattern for creating descriptor set layouts,
//! abstracting away the raw `ash::vk` types.

use ash::vk;

use crate::sync::VkDescriptorSetLayout;
use crate::vulkan::context::VulkanContext;
use crate::vulkan::pipeline_state::{DescriptorType, ShaderStages};

/// A single descriptor binding in a descriptor set layout.
#[derive(Debug, Clone, Hash)]
pub(crate) struct LayoutBinding {
    /// Binding number.
    pub binding: u32,
    /// Descriptor type.
    pub descriptor_type: DescriptorType,
    /// Number of descriptors in this binding.
    pub descriptor_count: u32,
    /// Shader stages that can access this binding.
    pub shader_stages: ShaderStages,
}

impl LayoutBinding {
    /// Create a new layout binding.
    pub(crate) fn new(
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
pub(crate) struct DescriptorSetLayoutBuilder {
    bindings: Vec<LayoutBinding>,
    push_descriptor: bool,
}

impl DescriptorSetLayoutBuilder {
    /// Create a new descriptor set layout builder.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Add a binding to the descriptor set layout.
    pub(crate) fn add_binding(
        mut self,
        binding: u32,
        descriptor_type: DescriptorType,
        shader_stages: ShaderStages,
    ) -> Self {
        self.bindings
            .push(LayoutBinding::new(binding, descriptor_type, shader_stages));
        self
    }

    /// Build the descriptor set layout.
    pub(crate) fn build(
        self,
        context: &VulkanContext,
    ) -> Result<VkDescriptorSetLayout, vk::Result> {
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
            context
                .device
                .create_descriptor_set_layout(&create_info, None)?
        };

        Ok(VkDescriptorSetLayout::new(layout))
    }

    /// Get the bindings for this layout (for hashing/caching).
    pub(crate) fn bindings(&self) -> &[LayoutBinding] {
        &self.bindings
    }

    /// Check if this layout uses push descriptors.
    pub(crate) fn is_push_descriptor(&self) -> bool {
        self.push_descriptor
    }
}

//=============================================================================
// Tests
//=============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vulkan::pipeline_state::DescriptorType;

    #[test]
    fn test_layout_binding_creation() {
        let binding = LayoutBinding::new(0, DescriptorType::StorageBuffer, ShaderStages::VERTEX);
        assert_eq!(binding.binding, 0);
        assert_eq!(binding.descriptor_count, 1);
    }

    #[test]
    fn test_layout_binding_into_vk() {
        let binding = LayoutBinding::new(0, DescriptorType::StorageBuffer, ShaderStages::COMPUTE);
        let vk_binding = binding.into_vk();
        assert_eq!(vk_binding.binding, 0);
        assert_eq!(
            vk_binding.descriptor_type,
            vk::DescriptorType::STORAGE_BUFFER
        );
        assert_eq!(vk_binding.stage_flags, vk::ShaderStageFlags::COMPUTE);
    }

    #[test]
    fn test_descriptor_set_layout_builder() {
        let builder = DescriptorSetLayoutBuilder::new()
            .add_binding(0, DescriptorType::StorageBuffer, ShaderStages::VERTEX)
            .add_binding(1, DescriptorType::SampledImage, ShaderStages::FRAGMENT);

        assert_eq!(builder.bindings.len(), 2);
        assert_eq!(builder.bindings[0].binding, 0);
        assert_eq!(builder.bindings[1].binding, 1);
    }
}
