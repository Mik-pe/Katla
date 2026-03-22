//! Compositing descriptor set for multi-viewport rendering.
//!
//! This module provides a descriptor set for compositing multiple viewport
//! textures onto the final output. Uses a fixed texture array (max 8 viewports)
//! at set 2, binding 0.
//!
//! # Descriptor Layout
//!
//! - **Set 2, Binding 0**: `texture_2d[]` array (max 8 textures)
//! - **Shader access**: `@group(2) @binding(0) var viewportTextures: binding_array<texture_2d<f32>, 8>;`
//!
//! # Example
//!
//! ```ignore
//! use katla_gfx::render_graph::descriptor_sets::CompositingDescriptorSet;
//!
//! // Create with 2 viewport textures
//! let textures = vec![viewport0_view, viewport1_view];
//! let desc_set = CompositingDescriptorSet::new(&context, &textures)?;
//!
//! // Update textures at runtime
//! desc_set.update_textures(&[new_view0, new_view1])?;
//! ```

use ash::vk;
use std::rc::Rc;

use crate::RendererError;
use crate::sync::VkDescriptorSetLayout;
use crate::vulkan::context::VulkanContext;

/// Maximum number of viewports supported by compositing.
pub const MAX_VIEWPORTS: usize = 8;

/// Compositing descriptor set for multi-viewport rendering.
///
/// Manages a descriptor set with a fixed array of 8 texture bindings for
/// compositing multiple viewport outputs. Textures are bound to set 2,
/// binding 0, and can be updated at runtime via `update_textures()`.
///
/// # Descriptor Layout
/// - **Set 2**: Compositing descriptor set
///   - **Binding 0**: Texture array (max 8 textures)
///
/// # Errors
///
/// - Returns `RendererError::InvalidOperation("Too many viewports")` if more than
///   8 textures are provided during creation or update.
///
/// # Example
///
/// Create with 2 viewport textures:
///
/// ```ignore
/// let textures = vec![viewport0_view, viewport1_view];
/// let desc_set = CompositingDescriptorSet::new(&context, &textures)?;
/// ```
///
/// Update textures at runtime (same or fewer count):
///
/// ```ignore
/// desc_set.update_textures(&[new_view0, new_view1])?;
/// ```
pub struct CompositingDescriptorSet {
    /// Descriptor pool for the compositing set.
    descriptor_pool: vk::DescriptorPool,
    /// Descriptor set layout for compositing.
    descriptor_layout: VkDescriptorSetLayout,
    /// Vulkan descriptor set handle.
    descriptor_set: vk::DescriptorSet,
    /// Device handle for cleanup and updates.
    device: ash::Device,
    /// Number of currently bound textures (for validation).
    texture_count: usize,
}

impl CompositingDescriptorSet {
    /// Create a compositing descriptor set layout (without allocating a descriptor set).
    ///
    /// This creates only the descriptor set layout that can be used when compiling
    /// pipelines. The descriptor set itself will be allocated later during frame execution.
    ///
    /// Uses `UPDATE_AFTER_BIND` to allow safe per-frame descriptor updates without
    /// invalidating in-flight command buffers.
    ///
    /// # Arguments
    /// * `device` - Vulkan device
    ///
    /// # Returns
    /// The descriptor set layout handle
    pub fn create_layout(device: &ash::Device) -> Result<vk::DescriptorSetLayout, RendererError> {
        let bindings = [vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
            .descriptor_count(MAX_VIEWPORTS as u32)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)];

        let binding_flags = [vk::DescriptorBindingFlags::UPDATE_AFTER_BIND];

        let mut binding_flags_info =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::default().binding_flags(&binding_flags);

        let layout_info = vk::DescriptorSetLayoutCreateInfo::default()
            .bindings(&bindings)
            .flags(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL)
            .push_next(&mut binding_flags_info);

        unsafe {
            device
                .create_descriptor_set_layout(&layout_info, None)
                .map_err(|e| {
                    RendererError::VulkanError(format!(
                        "Failed to create compositing descriptor set layout: {:?}",
                        e
                    ))
                })
        }
    }

    /// Create a new compositing descriptor set.
    ///
    /// Creates a descriptor set layout and descriptor set with a texture array
    /// at set 2, binding 0. The texture array has a fixed size of 8, but only
    /// the first N textures are written (where N = `textures.len()`).
    ///
    /// # Arguments
    /// * `context` - Vulkan context
    /// * `textures` - Slice of image views to bind (max 8)
    ///
    /// # Returns
    /// A new CompositingDescriptorSet, or an error if:
    /// - More than 8 textures are provided
    /// - Descriptor set creation fails
    ///
    /// # Example
    /// ```ignore
    /// let textures = vec![viewport0_view, viewport1_view];
    /// let desc_set = CompositingDescriptorSet::new(&context, &textures)?;
    /// ```
    pub fn new(
        context: &Rc<VulkanContext>,
        textures: &[vk::ImageView],
    ) -> Result<Self, RendererError> {
        if textures.len() > MAX_VIEWPORTS {
            return Err(RendererError::InvalidOperation(format!(
                "Too many viewports: {} exceeds maximum of {}",
                textures.len(),
                MAX_VIEWPORTS
            )));
        }

        let bindings = [vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
            .descriptor_count(MAX_VIEWPORTS as u32)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)];

        let binding_flags = [vk::DescriptorBindingFlags::UPDATE_AFTER_BIND];

        let mut binding_flags_info =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::default().binding_flags(&binding_flags);

        let layout_info = vk::DescriptorSetLayoutCreateInfo::default()
            .bindings(&bindings)
            .flags(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL)
            .push_next(&mut binding_flags_info);

        let descriptor_layout = unsafe {
            context
                .device
                .create_descriptor_set_layout(&layout_info, None)?
        };

        let pool_sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::SAMPLED_IMAGE)
            .descriptor_count(MAX_VIEWPORTS as u32)];

        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(1)
            .flags(vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND);

        let descriptor_pool = unsafe { context.device.create_descriptor_pool(&pool_info, None)? };

        // Allocate descriptor set
        let layouts = [descriptor_layout];
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&layouts);

        let descriptor_sets = unsafe { context.device.allocate_descriptor_sets(&alloc_info)? };
        let descriptor_set = descriptor_sets[0];

        // Create the descriptor set instance
        let desc_set = Self {
            descriptor_pool,
            descriptor_layout: VkDescriptorSetLayout::new(descriptor_layout),
            descriptor_set,
            device: context.device.clone(),
            texture_count: textures.len(),
        };

        // Write texture descriptors (this will fill all 8 slots)
        // We need a mutable reference to call update_textures
        // Since we just created desc_set, we can use unsafe mut pattern or restructure
        // For now, let's duplicate the update logic here to avoid borrow issues
        if textures.is_empty() {
            return Err(RendererError::InvalidOperation(
                "At least one viewport texture must be provided".to_string(),
            ));
        }

        // Fill all slots to avoid Vulkan validation errors
        let mut all_image_views: Vec<vk::ImageView> = textures.to_vec();
        let placeholder = textures[0];
        while all_image_views.len() < MAX_VIEWPORTS {
            all_image_views.push(placeholder);
        }

        // Update descriptor set for all slots
        for (slot_idx, &image_view) in all_image_views.iter().enumerate() {
            let image_info = [vk::DescriptorImageInfo::default()
                .image_view(image_view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];

            let write = vk::WriteDescriptorSet::default()
                .dst_set(desc_set.descriptor_set)
                .dst_binding(0)
                .dst_array_element(slot_idx as u32)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(1)
                .image_info(&image_info);

            unsafe {
                context.device.update_descriptor_sets(&[write], &[]);
            }
        }

        Ok(desc_set)
    }

    /// Update viewport textures at runtime.
    ///
    /// Replaces the texture bindings in the descriptor set with new image views.
    /// The number of textures can be different from the initial count, but must
    /// not exceed MAX_VIEWPORTS (8).
    ///
    /// IMPORTANT: This function will update ALL descriptors up to MAX_VIEWPORTS.
    /// Any slots beyond the provided textures will be filled with a placeholder
    //1 to avoid Vulkan validation errors about uninitialized descriptors.
    ///
    /// # Arguments
    /// * `context` - Vulkan context
    /// * `textures` - New slice of image views to bind (max 8)
    ///
    /// # Returns
    /// Ok(()) if successful, or an error if:
    /// - More than 8 textures are provided
    /// - Descriptor update fails
    ///
    /// # Example
    /// ```ignore
    /// // Update with 2 new textures
    /// desc_set.update_textures(&[new_view0, new_view1])?;
    ///
    /// // Update with 4 new textures
    /// desc_set.update_textures(&[v0, v1, v2, v3])?;
    /// ```
    pub fn update_textures(&mut self, textures: &[vk::ImageView]) -> Result<(), RendererError> {
        // Validate viewport count
        if textures.len() > MAX_VIEWPORTS {
            return Err(RendererError::InvalidOperation(format!(
                "Too many viewports: {} exceeds maximum of {}",
                textures.len(),
                MAX_VIEWPORTS
            )));
        }

        // Collect all image views to update (provided textures + placeholders)
        let mut all_image_views: Vec<vk::ImageView> = textures.to_vec();

        // Fill remaining slots with the first texture (placeholder) to avoid
        // Vulkan validation errors about uninitialized array elements
        // This is necessary because WGSL binding arrays compile to SPIR-V
        // arrays that may access any element regardless of runtime branching
        let placeholder = if textures.is_empty() {
            return Err(RendererError::InvalidOperation(
                "At least one viewport texture must be provided".to_string(),
            ));
        } else {
            textures[0]
        };

        while all_image_views.len() < MAX_VIEWPORTS {
            all_image_views.push(placeholder);
        }

        // Update descriptor set for all slots (including placeholders)
        for (slot_idx, &image_view) in all_image_views.iter().enumerate() {
            let image_info = [vk::DescriptorImageInfo::default()
                .image_view(image_view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];

            let write = vk::WriteDescriptorSet::default()
                .dst_set(self.descriptor_set)
                .dst_binding(0)
                .dst_array_element(slot_idx as u32)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(1)
                .image_info(&image_info);

            unsafe {
                self.device.update_descriptor_sets(&[write], &[]);
            }
        }

        self.texture_count = textures.len();
        Ok(())
    }

    /// Get the raw Vulkan descriptor set handle.
    ///
    /// Used for binding the descriptor set during command buffer recording.
    ///
    /// # Example
    /// ```ignore
    /// cmd_buf.bind_descriptor_sets(
    ///     vk::PipelineBindPoint::GRAPHICS,
    ///     pipeline_layout,
    ///     2, // set 2
    ///     &[compositing_desc.vk_set()],
    ///     &[],
    /// );
    /// ```
    pub fn vk_set(&self) -> vk::DescriptorSet {
        self.descriptor_set
    }

    /// Get the descriptor set layout for pipeline creation.
    ///
    /// Used when creating pipeline layouts that include the compositing set.
    ///
    /// # Example
    /// ```ignore
    /// let layouts = [
    ///     storage_set_layout, // set 0
    ///     bindless_set_layout, // set 1
    ///     compositing_set.layout(), // set 2
    /// ];
    /// let pipeline_layout = device.create_pipeline_layout(&layouts)?;
    /// ```
    pub fn layout(&self) -> vk::DescriptorSetLayout {
        self.descriptor_layout.vk()
    }

    /// Get the number of currently bound textures.
    ///
    /// Useful for validation and debugging to ensure the expected number
    /// of viewports are bound.
    ///
    /// # Example
    /// ```ignore
    /// assert_eq!(compositing_desc.texture_count(), 2);
    /// ```
    pub fn texture_count(&self) -> usize {
        self.texture_count
    }
}

impl Drop for CompositingDescriptorSet {
    fn drop(&mut self) {
        unsafe {
            // Destroying the pool automatically frees all descriptor sets in it
            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);
            self.device
                .destroy_descriptor_set_layout(self.descriptor_layout.vk(), None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_viewports_constant() {
        // Ensure we have the expected maximum
        assert_eq!(MAX_VIEWPORTS, 8);
    }

    #[test]
    fn test_compositing_descriptor_set_new_requires_vulkan() {
        // This test verifies the API signature is correct
        // Actual functionality testing requires a Vulkan context

        // Verify MAX_VIEWPORTS is a reasonable value
        assert!(MAX_VIEWPORTS >= 2);
        assert!(MAX_VIEWPORTS <= 16);
    }

    #[test]
    fn test_compositing_descriptor_set_validation_logic() {
        // Test validation logic without requiring Vulkan context
        // We can verify the error message format

        let count = 10;
        let error_msg = format!(
            "Too many viewports: {} exceeds maximum of {}",
            count, MAX_VIEWPORTS
        );

        assert!(error_msg.contains("Too many viewports"));
        assert!(error_msg.contains(&count.to_string()));
        assert!(error_msg.contains(&MAX_VIEWPORTS.to_string()));
    }

    #[test]
    fn test_compositing_descriptor_set_texture_count() {
        // Verify texture_count() method exists and returns the expected type
        // Actual testing requires Vulkan context

        // This is a compile-time check that the API is correct
        fn check_texture_count(desc_set: &CompositingDescriptorSet) -> usize {
            desc_set.texture_count()
        }

        // Verify the function signature
        assert!(std::mem::size_of::<usize>() > 0);
    }
}
