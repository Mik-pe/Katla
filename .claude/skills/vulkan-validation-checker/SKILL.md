---
name: vulkan-validation-checker
description: Analyze Vulkan code for validation errors, synchronization issues, and best practices violations. Updated for Vulkan 1.3 (2026) with modern features like Dynamic Rendering, Buffer Device Address, Descriptor Indexing, and Synchronization2. Use when implementing new Vulkan features, debugging rendering issues, or reviewing code changes.
allowed-tools: Read, Grep, Glob, Bash
---

# Vulkan Validation Checker (2026 Edition)

## Overview

This skill helps identify common Vulkan validation errors, synchronization issues, and best practices violations in the Katla Vulkan engine, with a focus on **Vulkan 1.3 modern practices**.

## Modern Vulkan 1.3 Checklist

Before deep-diving into validation, check if the code is using modern Vulkan patterns:

### Vulkan 1.3 Core Features
- [ ] **Dynamic Rendering** - Replace traditional render pass objects
- [ ] **Buffer Device Address** - Access buffers via pointers instead of descriptors
- [ ] **Descriptor Indexing (Bindless)** - Single large texture arrays
- [ ] **Synchronization2** - Improved barrier API with `vkCmdPipelineBarrier2`

### 2026 Best Practices
- [ ] **VMA with `VMA_MEMORY_USAGE_AUTO`** - Automatic memory type selection
- [ ] **Persistent buffer mapping** - Safe and efficient in modern Vulkan
- [ ] **Frames in-flight** - CPU/GPU parallelism (2-3 frames)
- [ ] **Slang shading language** - More modern than GLSL, runtime compilation
- [ ] **VK_IMAGE_LAYOUT_ATTACHMENT_OPTIMAL** - Unified layout for all attachments

## Quick Checks

### 1. Modern Vulkan Feature Scan
```bash
# Check for Dynamic Rendering (Vulkan 1.3)
grep -rn "begin_rendering\|cmd_begin_rendering\|BeginRendering" katla_vulkan/src/

# Check for Buffer Device Address
grep -rn "SHADER_DEVICE_ADDRESS\|BufferDeviceAddress" katla_vulkan/src/

# Check for Descriptor Indexing (bindless)
grep -rn "descriptorIndexing\|bindless\|NonUniformResourceIndex" katla_vulkan/src/

# Check for Synchronization2
grep -rn "pipeline_barrier2\|PipelineBarrier2\|ImageMemoryBarrier2\|DependencyInfo" katla_vulkan/src/

# Check for VMA usage
grep -rn "VMA_MEMORY_USAGE_AUTO" katla_vulkan/src/
```

### 2. Legacy Pattern Detection
```bash
# Check for legacy render pass usage (should use Dynamic Rendering)
grep -rn "create_render_pass\|RenderPass::create" katla_vulkan/src/

# Check for traditional descriptor management (should use bindless)
grep -rn "allocate_descriptor_sets\|update_descriptor_sets" katla_vulkan/src/ | \
  grep -v "bindless\|texture array"

# Check for old-style pipeline barriers (should use Synchronization2)
grep -rn "pipeline_barrier\|PipelineBarrier" katla_vulkan/src/ | \
  grep -v "pipeline_barrier2\|PipelineBarrier2"
```

### 3. Validation Error Patterns
```bash
# Check for null handle usage (common error)
grep -rn "vk::.*::null()" katla_vulkan/src/

# Check for UNDEFINED layouts
grep -rn "UNDEFINED\|Undefined" katla_vulkan/src/

# Check for proper error handling
grep -rn "unwrap()" katla_vulkan/src/vulkan/ | grep -v "// "
```

### 4. Synchronization Analysis
```bash
# Check for pipeline barriers
grep -rn "pipeline_barrier\|PipelineBarrier2" katla_vulkan/src/

# Check for semaphore usage
grep -rn "Semaphore\|Fence" katla_vulkan/src/

# Check for command buffer synchronization
grep -rn "cmd_wait_events\|wait_semaphores" katla_vulkan/src/
```

## Common Validation Error Patterns

### Memory Management Issues

**1. Missing Memory Property Flags**
```rust
// WRONG: No memory properties specified
let memory = device.allocate_memory(&info, None)?;

// CORRECT: Use VMA with automatic selection
let allocation = vma_create_buffer(
    allocator,
    &buffer_ci,
    &VmaAllocationCreateInfo {
        usage: VMA_MEMORY_USAGE_AUTO,  // Let VMA choose
        flags: VMA_ALLOCATION_CREATE_HOST_ACCESS_SEQUENTIAL_WRITE_BIT |
               VMA_ALLOCATION_CREATE_MAPPED_BIT,
        ..Default::default()
    },
    &mut buffer,
    &mut allocation,
    None
)?;
```

**2. Incorrect Buffer/Image Usage Flags**
```rust
// Check for missing usage flags
// - TRANSFER_SRC for transfer sources
// - TRANSFER_DST for transfer destinations
// - SHADER_DEVICE_ADDRESS_BIT for buffer device address
// - UNIFORM_BUFFER for uniform buffers (if not using BDA)
// - VERTEX_BUFFER for vertex buffers
// - INDEX_BUFFER for index buffers

// MODERN: Use Buffer Device Address instead
VkBufferCreateInfo buffer_ci {
    .sType = VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO,
    .size = sizeof(ShaderData),
    .usage = VK_BUFFER_USAGE_SHADER_DEVICE_ADDRESS_BIT
};
```

**3. Not Using VMA Memory Auto-Selection**
```rust
// WRONG: Manually selecting memory types
let memory_type = find_memory_type(/* ... */)?;

// CORRECT: Use VMA_MEMORY_USAGE_AUTO
VmaAllocationCreateInfo alloc_ci {
    .flags = VMA_ALLOCATION_CREATE_HOST_ACCESS_SEQUENTIAL_WRITE_BIT |
             VMA_ALLOCATION_CREATE_HOST_ACCESS_ALLOW_TRANSFER_INSTEAD_BIT |
             VMA_ALLOCATION_CREATE_MAPPED_BIT,
    .usage = VMA_MEMORY_USAGE_AUTO  // Automatic selection
};
```

### Synchronization Issues (Synchronization2)

**1. Not Using Synchronization2**
```rust
// LEGACY (Vulkan 1.0): Old-style barrier
cmd_buffer.pipeline_barrier(
    src_stage_mask, dst_stage_mask,
    vk::DependencyFlags::empty(),
    &[],
    &[image_memory_barrier]
);

// MODERN (Vulkan 1.3): Synchronization2
VkImageMemoryBarrier2 barrier {
    .sType = VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER_2,
    .srcStageMask = VK_PIPELINE_STAGE_2_NONE,
    .srcAccessMask = VK_ACCESS_2_NONE,
    .dstStageMask = VK_PIPELINE_STAGE_2_TRANSFER_BIT,
    .dstAccessMask = VK_ACCESS_2_TRANSFER_WRITE_BIT,
    .oldLayout = VK_IMAGE_LAYOUT_UNDEFINED,
    .newLayout = VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
    .image = image,
    .subresourceRange = { .aspectMask = VK_IMAGE_ASPECT_COLOR_BIT, .levelCount = 1, .layerCount = 1 }
};
VkDependencyInfo barrier_info {
    .sType = VK_STRUCTURE_TYPE_DEPENDENCY_INFO,
    .imageMemoryBarrierCount = 1,
    .pImageMemoryBarriers = &barrier
};
vkCmdPipelineBarrier2(cmd_buffer, &barrier_info);
```

**2. Missing Pipeline Barriers**
```bash
# Find images that may need barriers
grep -rn "ImageLayout::" katla_vulkan/src/ | grep -v "Undefined\|AttachmentOptimal"
```

**3. Incorrect Image Layout Transitions**
```rust
// COMMON ERROR: Not transitioning image layout before use
// Correct pattern with Synchronization2:
VkImageMemoryBarrier2 barrier {
    .sType = VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER_2,
    .srcStageMask = VK_PIPELINE_STAGE_2_NONE,
    .srcAccessMask = VK_ACCESS_2_NONE,
    .dstStageMask = VK_PIPELINE_STAGE_2_COLOR_ATTACHMENT_OUTPUT_BIT,
    .dstAccessMask = VK_ACCESS_2_COLOR_ATTACHMENT_READ_BIT | VK_ACCESS_2_COLOR_ATTACHMENT_WRITE_BIT,
    .oldLayout = VK_IMAGE_LAYOUT_UNDEFINED,
    .newLayout = VK_IMAGE_LAYOUT_ATTACHMENT_OPTIMAL,  // Vulkan 1.3 unified layout
    .image = swapchain_images[image_index],
    .subresourceRange = {.aspectMask = VK_IMAGE_ASPECT_COLOR_BIT, .levelCount = 1, .layerCount = 1 }
};
```

**4. Queue Family Ownership Transfer**
```rust
// When transferring between queue families (graphics → compute → present)
// Use VkImageMemoryBarrier2 with:
// - srcQueueFamilyIndex: graphics queue index
// - dstQueueFamilyIndex: present queue index
// - Use VK_IMAGE_LAYOUT_ATTACHMENT_OPTIMAL for Vulkan 1.3
```

### Dynamic Rendering vs Legacy Render Pass

**1. Using Legacy Render Pass Objects**
```rust
// LEGACY (Vulkan 1.0): Cumbersome render pass objects
VkRenderPassCreateInfo render_pass_ci {
    .sType = VK_STRUCTURE_TYPE_RENDER_PASS_CREATE_INFO,
    .attachmentCount = 2,
    .pAttachments = attachments,
    .subpassCount = 1,
    .pSubpasses = &subpass,
    .dependencyCount = 1,
    .pDependencies = &dependency,
};
vkCreateRenderPass(device, &render_pass_ci, nullptr, &render_pass);

// MODERN (Vulkan 1.3): Dynamic Rendering
VkRenderingAttachmentInfo color_attachment {
    .sType = VK_STRUCTURE_TYPE_RENDERING_ATTACHMENT_INFO,
    .imageView = swapchain_image_views[image_index],
    .imageLayout = VK_IMAGE_LAYOUT_ATTACHMENT_OPTIMAL,
    .loadOp = VK_ATTACHMENT_LOAD_OP_CLEAR,
    .storeOp = VK_ATTACHMENT_STORE_OP_STORE,
    .clearValue = {.color = {0.0f, 0.0f, 0.2f, 1.0f}}
};

VkRenderingInfo rendering_info {
    .sType = VK_STRUCTURE_TYPE_RENDERING_INFO,
    .renderArea = {.extent = {.width = 1920, .height = 1080}},
    .layerCount = 1,
    .colorAttachmentCount = 1,
    .pColorAttachments = &color_attachment,
    .pDepthAttachment = &depth_attachment,
};
vkCmdBeginRendering(cmd_buffer, &rendering_info);
// ... draw commands ...
vkCmdEndRendering(cmd_buffer);
```

**2. Pipeline Creation with Dynamic Rendering**
```rust
// MODERN: Pipeline setup for Dynamic Rendering
VkPipelineRenderingCreateInfo rendering_ci {
    .sType = VK_STRUCTURE_TYPE_PIPELINE_RENDERING_CREATE_INFO,
    .colorAttachmentCount = 1,
    .pColorAttachmentFormats = &image_format,
    .depthAttachmentFormat = depth_format
    // Passed via pNext in pipeline create info
};

VkGraphicsPipelineCreateInfo pipeline_ci {
    .sType = VK_STRUCTURE_TYPE_GRAPHICS_PIPELINE_CREATE_INFO,
    .pNext = &rendering_ci,  // Link dynamic rendering info
    // ... no renderPass needed ...
    .layout = pipeline_layout
};
```

### Buffer Device Address (BDA) vs Descriptors

**1. Using Descriptors for Buffers**
```rust
// LEGACY: Descriptors for uniform buffers
// Shader: layout(set = 0, binding = 0) uniform Camera { mat4 view; mat4 proj; };
// Need: DescriptorSetLayout, DescriptorPool, DescriptorSet, updates, binds

// MODERN: Buffer Device Address
// Shader: uniform ShaderData *shaderData;  // Pointer access
// Create buffer with device address bit
VkBufferCreateInfo buffer_ci {
    .sType = VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO,
    .size = sizeof(ShaderData),
    .usage = VK_BUFFER_USAGE_SHADER_DEVICE_ADDRESS_BIT
};

// Get device address
VkBufferDeviceAddressInfo address_info {
    .sType = VK_STRUCTURE_TYPE_BUFFER_DEVICE_ADDRESS_INFO,
    .buffer = shader_data_buffer
};
VkDeviceAddress device_address = vkGetBufferDeviceAddress(device, &address_info);

// Pass via push constant
vkCmdPushConstants(cmd_buffer, pipeline_layout,
                   VK_SHADER_STAGE_VERTEX_BIT,
                   0, sizeof(VkDeviceAddress), &device_address);
```

**2. Shader Access with BDA**
```rust
// Shader code (Slang, GLSL, HLSL all support this)
[shader("vertex")]
VSOutput main(VSInput input, uniform ShaderData *shaderData, uint instanceIndex : SV_VulkanInstanceID) {
    VSOutput output;
    float4x4 modelMat = shaderData->model[instanceIndex];
    output.Pos = mul(shaderData->projection, mul(shaderData->view, mul(modelMat, float4(input.Pos.xyz, 1.0))));
    return output;
}
```

### Descriptor Indexing (Bindless Textures)

**1. Per-Texture Descriptor Management**
```rust
// LEGACY: One descriptor set per texture
// Doesn't scale - limited by descriptor pool size

// MODERN: Bindless texture array
// Create one large array of textures
VkDescriptorSetLayoutBinding desc_layout_binding_tex {
    .descriptorType = VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER,
    .descriptorCount = static_cast<uint32_t>(textures.size()),  // All textures in one array
    .stageFlags = VK_SHADER_STAGE_FRAGMENT_BIT
};

// Enable variable descriptor count
VkDescriptorBindingFlags desc_variable_flag {
    VK_DESCRIPTOR_BINDING_VARIABLE_DESCRIPTOR_COUNT_BIT
};
VkDescriptorSetLayoutBindingFlagsCreateInfo desc_binding_flags {
    .sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_BINDING_FLAGS_CREATE_INFO,
    .bindingCount = 1,
    .pBindingFlags = &desc_variable_flag
};

// Shader access
[shader("fragment")]
float4 main(VSOutput input) {
    // Index directly into texture array
    float3 color = textures[NonUniformResourceIndex(input.InstanceIndex)].Sample(input.UV).rgb;
    return float4(color, 1.0);
}
```

**2. Required Features for Bindless**
```rust
// Enable in device creation
VkPhysicalDeviceVulkan12Features enabled_vk12_features {
    .sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_VULKAN_1_2_FEATURES,
    .descriptorIndexing = true,
    .shaderSampledImageArrayNonUniformIndexing = true,
    .descriptorBindingVariableDescriptorCount = true,
    .runtimeDescriptorArray = true
};
```

### Frames in Flight

**1. Resource Duplication Issues**
```rust
// WRONG: Single shared resource for CPU/GPU
struct FrameResources {
    shader_data_buffer: vk::Buffer,  // CPU writes while GPU reads!
};

// MODERN: Per-frame duplication (2-3 frames in flight)
struct FrameResources {
    shader_data_buffers: Vec<Buffer>,  // One per frame
    command_buffers: Vec<vk::CommandBuffer>,
    fences: Vec<vk::Fence>,
    semaphores: Vec<vk::Semaphore>,
};

// In render loop:
let frame_index = current_frame % max_frames_in_flight;
// CPU writes to shader_data_buffers[frame_index]
// GPU reads from shader_data_buffers[(frame_index + 1) % max_frames_in_flight]
```

**2. Synchronization for Frames in Flight**
```rust
// Wait for GPU to finish using this frame's resources
vkWaitForFences(device, 1, &fences[frame_index], VK_TRUE, UINT64_MAX);
vkResetFences(device, 1, &fences[frame_index]);

// Now safe to update
memcpy(shader_data_buffers[frame_index].mapped, &shader_data, sizeof(ShaderData));
```

### Persistent Buffer Mapping

**1. Frequent Map/Unmap**
```rust
// LEGACY: Map/unmap every frame
vmaMapMemory(allocator, allocation, &ptr);
memcpy(ptr, data, size);
vmaUnmapMemory(allocator, allocation);

// MODERN: Persistent mapping (safe in Vulkan)
VmaAllocationCreateInfo alloc_ci {
    .flags = VMA_ALLOCATION_CREATE_HOST_ACCESS_SEQUENTIAL_WRITE_BIT |
             VMA_ALLOCATION_CREATE_HOST_ACCESS_ALLOW_TRANSFER_INSTEAD_BIT |
             VMA_ALLOCATION_CREATE_MAPPED_BIT,  // Keep mapped
    .usage = VMA_MEMORY_USAGE_AUTO
};
vmaCreateBuffer(allocator, &buffer_ci, &alloc_ci, &buffer, &allocation, nullptr);
vmaMapMemory(allocator, allocation, &persistent_ptr);  // Map once

// Now just memcpy whenever
memcpy(persistent_ptr, data, size);  // No map/unmap overhead
```

### Shader Interface Issues

**1. Location Mismatch**
```bash
# Check vertex attribute bindings match shader locations
# Shader: layout(location = 0) in vec3 position;
# Rust: location: 0 in VertexInputAttributeDescription
```

**2. Push Constant Range for BDA**
```rust
// MODERN: Pass buffer addresses via push constants
VkPushConstantRange push_constant_range {
    .stageFlags = VK_SHADER_STAGE_VERTEX_BIT,
    .offset = 0,
    .size = sizeof(VkDeviceAddress)  // Just a pointer!
};

// No need for large uniform buffer bindings anymore
```

**3. Struct Layout Matching**
```rust
// IMPORTANT: Match CPU/GPU struct layouts
// Use VK_EXT_scalar_block_layout or Vulkan 1.2 feature
// Or manually align/pad structures

// CPU side
struct ShaderData {
    glm::mat4 projection;
    glm::mat4 view;
    glm::mat4 model[3];
    glm::vec4 lightPos;
    uint32_t selected;
};

// Shader side (Slang/GLSL)
struct ShaderData {
    float4x4 projection;
    float4x4 view;
    float4x4 model[3];
    float4 lightPos;
    uint selected;
};
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
// - RenderPasses: vkDestroyRenderPass (if not using Dynamic Rendering)
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
export VK_LAYER_FLAGS_KHRONOS_VALIDATION=best-practices,synchronize
cargo run
```

### Common Validation Messages

**1. UNASSIGNED-CoreValidation-Shader-OutputNotConsumed**
- Shader outputs vertex attributes not consumed by fragment shader
- Fix: Remove unused outputs or add matching inputs in fragment shader

**2. UNASSIGNED-CoreValidation-DrawState-InvalidImageLayout**
- Image used in wrong layout
- Fix: Add pipeline barrier to transition layout before use
- MODERN: Use `VK_IMAGE_LAYOUT_ATTACHMENT_OPTIMAL` for all attachments (Vulkan 1.3)

**3. UNASSIGNED-CoreValidation-DrawState-DescriptorSetNotUpdated**
- Descriptor set hasn't been updated since last use
- Fix: Call update_descriptor_sets or rebind descriptor sets
- MODERN: Use bindless textures to avoid frequent updates

**4. VUID-VkDescriptorImageInfo-imageLayout-00344**
- ImageLayout in descriptor write doesn't match actual image layout
- Fix: Ensure layout matches or use GENERAL layout
- MODERN: Use `VK_IMAGE_LAYOUT_READ_ONLY_OPTIMAL` for shader resources

**5. UNASSIGNED-CoreValidation-Shader-InterfaceMismatch**
- Vertex attribute format doesn't match shader input type
- Fix: Match format (vec3 = R32G32B32_SFLOAT, etc.)

**6. VUID-vkCmdDrawIndexed-None-02721 (Buffer Device Address)**
- Buffer device address not enabled or used incorrectly
- Fix: Enable bufferDeviceAddress feature and use SHADER_DEVICE_ADDRESS_BIT

**7. VUID-vkCmdBindDescriptorSets-pDescriptorSets-00358 (Bindless)**
- Descriptor indexing features not enabled
- Fix: Enable descriptorIndexing, shaderSampledImageArrayNonUniformIndexing, etc.

## Before Implementing New Vulkan Code

### Checklist (2026 Edition)

1. **Use Vulkan 1.3 as Baseline**
   - Target Vulkan 1.3 in `VkApplicationInfo`
   - Enable Vulkan 1.2 and 1.3 features
   - Avoid extension-only features if available in core

2. **Prefer Modern Over Legacy Patterns**
   - Use Dynamic Rendering instead of render pass objects
   - Use Buffer Device Address for uniform buffers
   - Use Descriptor Indexing (bindless) for textures
   - Use Synchronization2 (`vkCmdPipelineBarrier2`)

3. **Integrate VMA**
   - Use `VMA_MEMORY_USAGE_AUTO` for automatic selection
   - Use `VMA_ALLOCATION_CREATE_MAPPED_BIT` for persistent mapping
   - Enable `VMA_ALLOCATOR_CREATE_BUFFER_DEVICE_ADDRESS_BIT`

4. **Implement Frames in Flight**
   - Duplicate CPU/GPU shared resources (2-3 copies)
   - Use fences for CPU-GPU synchronization
   - Use semaphores for GPU-GPU synchronization

5. **Review Validation Layer Output**
   - Run with validation layers enabled
   - Address all ERROR and WARNING messages
   - Review BEST_PRACTICES warnings

6. **Check Synchronization**
   - Use `VkImageMemoryBarrier2` and `VkDependencyInfo`
   - Verify queue family ownership transfers
   - Use `VK_IMAGE_LAYOUT_ATTACHMENT_OPTIMAL` for attachments

7. **Verify Memory Management**
   - All allocations use VMA
   - Persistent mapping for frequently updated buffers
   - RAII wrappers implement Drop correctly

8. **Validate Shader Interfaces**
   - Match struct layouts between CPU and GPU
   - Use push constants for buffer addresses
   - Use `NonUniformResourceIndex` for bindless access

## Code Review Checklist (2026 Edition)

When reviewing Vulkan code changes:

### Modern Feature Usage
- [ ] Targets Vulkan 1.3 as baseline
- [ ] Uses Dynamic Rendering instead of render pass objects
- [ ] Uses Buffer Device Address for uniform buffers
- [ ] Uses Descriptor Indexing (bindless) for textures
- [ ] Uses Synchronization2 (`vkCmdPipelineBarrier2`)

### Memory Management
- [ ] Uses VMA with `VMA_MEMORY_USAGE_AUTO`
- [ ] Uses persistent mapping where appropriate
- [ ] RAII wrappers implement Drop correctly

### Synchronization
- [ ] Uses `VkImageMemoryBarrier2` and `VkDependencyInfo`
- [ ] Uses `VK_IMAGE_LAYOUT_ATTACHMENT_OPTIMAL` for attachments
- [ ] Proper frames in-flight implementation
- [ ] Fences for CPU-GPU sync, semaphores for GPU-GPU sync

### Error Handling
- [ ] No `unwrap()` calls on Vulkan API results
- [ ] Error paths clean up all allocated resources
- [ ] Public APIs don't expose ash::vk types (use wrapper types)

### Validation
- [ ] Shader interfaces match vertex input/bindings
- [ ] Struct layouts match between CPU and GPU
- [ ] Validation layers run clean (no ERROR/WARNING messages)
- [ ] All Vulkan resources wrapped in RAII types

### Legacy Code
- [ ] No legacy render pass objects (use Dynamic Rendering)
- [ ] No per-buffer descriptor management (use BDA)
- [ ] No per-texture descriptor sets (use bindless)
- [ ] No old-style pipeline barriers (use Synchronization2)

## Resources

### Modern Vulkan (2026)
- [How to Vulkan in 2026](https://howtovulkan.com) - Comprehensive guide to modern Vulkan practices
- [Vulkan 1.3 Specification](https://registry.khronos.org/vulkan/specs/1.3/html/)
- [Vulkan Guide - Best Practices](https://github.com/KhronosGroup/Vulkan-Guide/blob/main/chapters/adoption_recommendations.adoc)

### Validation
- [Vulkan Validation Layers Documentation](https://github.com/KhronosGroup/Vulkan-ValidationLayers)
- [Vulkan Spec - Appendix B: Validation Rules](https://registry.khronos.org/vulkan/specs/1.3/html/chapB.html)
- [Synchronization2 Reference](https://registry.khronos.org/vulkan/specs/1.3-extensions/man/html/VK_KHR_synchronization2.html)

### Libraries
- [Vulkan Memory Allocator (VMA)](https://github.com/GPUOpen-LibrariesAndSDKs/VulkanMemoryAllocator)
- [Slang Shading Language](https://shader-slang.com/)
- [Volk - Meta-loader](https://github.com/zeux/volk)
