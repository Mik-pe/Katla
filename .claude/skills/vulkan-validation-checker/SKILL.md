---
name: vulkan-validation-checker
description: Analyze Vulkan code for validation errors, synchronization issues, and best practices violations. Use when implementing new Vulkan features, debugging rendering issues, or reviewing code changes.
allowed-tools: Read, Grep, Glob, Bash
---

# Vulkan Validation Checker

## Overview

This skill helps identify common Vulkan validation errors, synchronization issues, and best practices violations in the Katla Vulkan engine.

## Quick Checks

### 1. Basic Validation Scan
```bash
# Check for null handle usage (common error)
grep -rn "vk::.*::null()" katla_vulkan/src/

# Check for uninitialized descriptors
grep -rn "UNDEFINED" katla_vulkan/src/

# Check for proper error handling
grep -rn "unwrap()" katla_vulkan/src/vulkan/ | grep -v "// "
```

### 2. Synchronization Analysis
```bash
# Check for pipeline barriers
grep -rn "pipeline_barrier\|PipelineBarrier" katla_vulkan/src/

# Check for semaphore usage
grep -rn "Semaphore\|Fence" katla_vulkan/src/

# Check for command buffer synchronization
grep -rn "cmd_wait_events\|wait_semaphores" katla_vulkan/src/
```

### 3. Descriptor Set Validation
```bash
# Check descriptor set layout creation
grep -rn "DescriptorSetLayout" katla_vulkan/src/

# Check for descriptor writes
grep -rn "write_descriptor_sets\|WriteDescriptorSet" katla_vulkan/src/

# Check for descriptor set allocation
grep -rn "allocate_descriptor_sets" katla_vulkan/src/
```

## Common Validation Error Patterns

### Memory Management Issues

**1. Missing Memory Property Flags**
```rust
// WRONG: No memory properties specified
let memory = device.allocate_memory(&info, None)?;

// CORRECT: Specify appropriate memory properties
let memory_requirements = device.get_buffer_memory_requirements(buffer);
let memory_type_index = find_memory_type(
    memory_requirements.memory_type_bits,
    vk::MemoryPropertyFlags::DEVICE_LOCAL
)?;
```

**2. Incorrect Buffer/Image Usage Flags**
```rust
// Check for missing usage flags
// - TRANSFER_SRC for transfer sources
// - TRANSFER_DST for transfer destinations
// - UNIFORM_BUFFER for uniform buffers
// - VERTEX_BUFFER for vertex buffers
// - INDEX_BUFFER for index buffers
```

### Synchronization Issues

**1. Missing Pipeline Barriers**
```bash
# Find images that may need barriers
grep -rn "ImageLayout::" katla_vulkan/src/ | grep -v "Undefined"
```

**2. Incorrect Image Layout Transitions**
```rust
// COMMON ERROR: Not transitioning image layout before use
// Correct pattern:
cmd_buffer.pipeline_barrier(
    src_stage_mask, dst_stage_mask,
    vk::DependencyFlags::empty(),
    &[],
    &[image_memory_barrier]
);
```

**3. Queue Family Ownership Transfer**
```rust
// When transferring between queue families (graphics → compute → present)
// Use vk::ImageMemoryBarrier with:
// - src_queue_family_index: graphics queue index
// - dst_queue_family_index: present queue index
```

### Descriptor Set Issues

**1. Binding Mismatch**
```rust
// Ensure shader binding numbers match set layout bindings
// Shader: layout(set = 0, binding = 0) uniform Camera { ... };
// Rust: binding: 0 in DescriptorSetLayoutCreateInfo
```

**2. Descriptor Type Mismatch**
```rust
// Match descriptor types to shader resource types:
// - uniform buffer → UNIFORM_BUFFER
// - storage buffer → STORAGE_BUFFER
// - combined image sampler → COMBINED_IMAGE_SAMPLER
// - sampler → SAMPLER
// - storage image → STORAGE_IMAGE
```

**3. Dynamic Offset Issues**
```rust
// When using dynamic offsets, ensure:
// - alignment requirements are met (minUniformBufferOffsetAlignment)
// - offsets are within buffer bounds
// - offsets are provided in bind_descriptor_sets call
```

### Render Pass Issues

**1. Attachment Load/Store Operations**
```rust
// Check for incorrect load ops:
// - LOAD_OP_LOAD for attachments that need previous contents
// - LOAD_OP_CLEAR for first use or when clearing
// - LOAD_OP_DONT_CARE for contents that will be overwritten

// Check store ops:
// - STORE_OP_STORE for attachments needed later
// - STORE_OP_DONT_CARE for final outputs
```

**2. Subpass Dependencies**
```rust
// Missing subpass dependencies can cause:
// - Hazards (read-after-write, write-after-read)
// - Undefined behavior
// - GPU hangs

// Add explicit dependencies for external subpasses:
vk::SubpassDependency {
    src_subpass: vk::SUBPASS_EXTERNAL,
    dst_subpass: 0,
    src_stage_mask: vk::PipelineStageFlags::BOTTOM_OF_PIPE,
    dst_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
    // ...
}
```

### Shader Interface Issues

**1. Location Mismatch**
```bash
# Check vertex attribute bindings match shader locations
# Shader: layout(location = 0) in vec3 position;
# Rust: location: 0 in VertexInputAttributeDescription
```

**2. Push Constant Range Mismatch**
```rust
// Ensure push constant ranges match shader declarations
// - offset must be multiple of 4
// - size must be multiple of 4
// - ranges must not overlap
// - total size <= minPushConstantsSize (usually 128 bytes)
```

## Katla-Specific Validation

### Ash Type Exclusion Rule
```bash
# Check for ash::vk types in public APIs (should use wrappers)
grep -rn "pub use ash::vk" katla_vulkan/src/
grep -rn "pub fn.*vk::" katla_vulkan/src/

# All public APIs should use wrapper types from:
# - katla_vulkan/src/render_graph/types.rs (ImageFormat, ImageLayout, etc.)
# - katla_vulkan/src/vulkan/vertexbuffer.rs (IndexType)
```

### RAII Wrapper Validation
```rust
// Check that all Vulkan resources implement Drop:
// - Buffers: vkDestroyBuffer
// - Images: vkDestroyImage
// - ImageViews: vkDestroyImageView
// - Samplers: vkDestroySampler
// - Framebuffers: vkDestroyFramebuffer
// - RenderPasses: vkDestroyRenderPass
// - DescriptorSetLayouts: vkDestroyDescriptorSetLayout
// - PipelineLayouts: vkDestroyPipelineLayout
// - Pipelines: vkDestroyPipeline

// Run this check:
grep -rn "impl Drop for" katla_vulkan/src/vulkan/
```

### Command Buffer Validation
```bash
# Check for command buffer begin/end pairs
grep -rn "begin_command_buffer\|begin_rendering" katla_vulkan/src/
grep -rn "end_command_buffer\|end_rendering" katla_vulkan/src/

# Verify they're balanced (should have equal counts in same functions)
```

## Validation Layer Integration

### Enable Validation Layers
```bash
# Run with validation layers enabled
export VK_LAYER_KHRONOS_VALIDATION=1
cargo run

# Enable specific validation features:
export VK_LAYER_KHRONOS_VALIDATION=1
export VK_LAYER_FLAGS_KHRONOS_VALIDATION=best-practices
cargo run
```

### Common Validation Messages

**1. UNASSIGNED-CoreValidation-Shader-OutputNotConsumed**
- Shader outputs vertex attributes not consumed by fragment shader
- Fix: Remove unused outputs or add matching inputs in fragment shader

**2. UNASSIGNED-CoreValidation-DrawState-InvalidImageLayout**
- Image used in wrong layout
- Fix: Add pipeline barrier to transition layout before use

**3. UNASSIGNED-CoreValidation-DrawState-DescriptorSetNotUpdated**
- Descriptor set hasn't been updated since last use
- Fix: Call update_descriptor_sets or rebind descriptor sets

**4. VUID-VkDescriptorImageInfo-imageLayout-00344**
- ImageLayout in descriptor write doesn't match actual image layout
- Fix: Ensure layout matches or use GENERAL layout

**5. UNASSIGNED-CoreValidation-Shader-InterfaceMismatch**
- Vertex attribute format doesn't match shader input type
- Fix: Match format (vec3 = R32G32B32_SFLOAT, etc.)

## Before Implementing New Vulkan Code

### Checklist

1. **Review Validation Layer Output**
   - Run with validation layers enabled
   - Address all ERROR and WARNING messages
   - Review BEST_PRACTICES warnings

2. **Check Synchronization**
   - Identify all pipeline stage dependencies
   - Add appropriate pipeline barriers
   - Verify queue family ownership transfers

3. **Verify Memory Management**
   - All allocations have corresponding frees
   - RAII wrappers implement Drop correctly
   - Error paths clean up resources

4. **Validate Descriptor Usage**
   - Descriptor set layouts match shader bindings
   - Descriptor types match shader resource types
   - Dynamic offsets are properly aligned

5. **Check Render Pass Compatibility**
   - Attachment load/store ops are correct
   - Subpass dependencies are specified
   - Image layouts are properly transitioned

## Code Review Checklist

When reviewing Vulkan code changes:

- [ ] No `unwrap()` calls on Vulkan API results (use proper error handling)
- [ ] All Vulkan resources are wrapped in RAII types with Drop
- [ ] Public APIs don't expose ash::vk types (use wrapper types)
- [ ] Pipeline barriers are present for all image layout transitions
- [ ] Descriptor sets are updated before use
- [ ] Command buffers are properly reset/fenced before reuse
- [ ] Error paths clean up all allocated resources
- [ ] Shader interfaces match vertex input/bindings
- [ ] Memory allocations respect alignment requirements
- [ ] Validation layers run clean (no ERROR/WARNING messages)

## Resources

- [Vulkan Validation Layers Documentation](https://github.com/KhronosGroup/Vulkan-ValidationLayers)
- [Vulkan Spec - Appendix B: Validation Rules](https://registry.khronos.org/vulkan/specs/1.3/html/chapB.html)
- [Vulkan Best Practices](https://github.com/KhronosGroup/Vulkan-Guide/blob/main/chapters/adoption_recommendations.adoc)
