# Vulkan Descriptor Set Patterns

This document captures common Vulkan descriptor set patterns discovered during Katla development.

## Descriptor Set Creation Pattern

The standard pattern for creating descriptor sets in Katla:

1. **Create Layout**: Use `vk::DescriptorSetLayoutCreateInfo` with bindings array
2. **Create Pool**: Allocate descriptor pool before allocating descriptor sets
3. **Allocate Sets**: Use `vk::DescriptorSetAllocateInfo` with layout and pool
4. **Update Descriptors**: Use `vk::WriteDescriptorSet` to bind resources

### Example Code

```rust
// 1. Create layout
let layout_info = vk::DescriptorSetLayoutCreateInfo::builder()
    .bindings(&bindings);
let descriptor_layout = device.create_descriptor_set_layout(&layout_info, None)?;

// 2. Create pool
let pool_size = vk::DescriptorPoolSize::builder()
    .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
    .descriptor_count(1024);
let pool_info = vk::DescriptorPoolCreateInfo::builder()
    .pool_sizes(&[pool_size])
    .max_sets(100);
let descriptor_pool = device.create_descriptor_pool(&pool_info, None)?;

// 3. Allocate sets
let alloc_info = vk::DescriptorSetAllocateInfo::builder()
    .descriptor_pool(descriptor_pool)
    .set_layouts(&[descriptor_layout]);
let descriptor_set = device.allocate_descriptor_sets(&alloc_info)?[0];

// 4. Update descriptors
let write = vk::WriteDescriptorSet::builder()
    .dst_set(descriptor_set)
    .dst_binding(0)
    .dst_array_element(slot_idx as u32)  // For array bindings
    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
    .image_info(&image_info);
device.update_descriptor_sets(&[write], &[]);
```

## Array Bindings

When updating descriptor array bindings (e.g., texture arrays), use `dst_array_element` to target specific indices:

```rust
for (slot_idx, texture_view) in textures.iter().enumerate() {
    let write = vk::WriteDescriptorSet::builder()
        .dst_set(descriptor_set)
        .dst_binding(0)
        .dst_array_element(slot_idx as u32)  // Target specific array element
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .image_info(&[image_info]);
    device.update_descriptor_sets(&[write], &[]);
}
```

## RAII Cleanup Pattern

Use wrapper types for automatic resource cleanup:

```rust
pub struct VkDescriptorSetLayout(pub vk::DescriptorSetLayout);

impl Drop for VkDescriptorSetLayout {
    fn drop(&mut self) {
        // Vulkan cleanup logic
    }
}
```

**Key Points:**
- Always create layout before pool and sets
- Use `dst_array_element` for array bindings
- Wrap Vulkan types in RAII structs for automatic cleanup
- Pool must support the descriptor type and count needed

**Discovered During:** compositing-descriptor-set feature (milestone: compositing-infrastructure)
