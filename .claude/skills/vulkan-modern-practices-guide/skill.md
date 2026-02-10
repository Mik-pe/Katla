---
name: vulkan-modern-practices-guide
description: Comprehensive guide to modern Vulkan 1.3 (2026) development patterns. Covers Dynamic Rendering, Buffer Device Address, Descriptor Indexing (bindless), Synchronization2, VMA integration, frames in-flight, and Slang shading language. Use when implementing new Vulkan features, refactoring legacy code, or learning modern Vulkan practices.
allowed-tools: Read, Grep, Glob, Bash
---

# Modern Vulkan 1.3 Practices Guide (2026)

## Overview

This guide covers **modern Vulkan 1.3 development patterns** as of 2026, based on best practices from [howtovulkan.com](https://howtovulkan.com). Vulkan has evolved significantly since 1.0, and modern features make development much simpler and more efficient.

## Why Modern Vulkan?

Vulkan 1.0 (2016) required verbose workarounds for many common tasks. Vulkan 1.3 (2022) brings these features into core:

| Feature | Vulkan 1.0 | Vulkan 1.3 |
|---------|-----------|-----------|
| Render Passes | Complex, rigid objects | Dynamic Rendering |
| Buffer Access | Descriptors required | Buffer Device Address |
| Texture Management | Per-texture descriptors | Bindless (Descriptor Indexing) |
| Synchronization | `vkCmdPipelineBarrier` | `vkCmdPipelineBarrier2` |
| Memory Management | Manual memory types | VMA `VMA_MEMORY_USAGE_AUTO` |

## Vulkan 1.3 Core Features

### 1. Dynamic Rendering

**Replaces:** Traditional render pass objects

Dynamic rendering is simpler and more flexible than legacy render pass objects:

```rust
// LEGACY (Vulkan 1.0): Create rigid render pass
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

// MODERN (Vulkan 1.3): Begin rendering directly
VkRenderingAttachmentInfo color_attachment {
    .sType = VK_STRUCTURE_TYPE_RENDERING_ATTACHMENT_INFO,
    .imageView = swapchain_image_views[image_index],
    .imageLayout = VK_IMAGE_LAYOUT_ATTACHMENT_OPTIMAL,  // Vulkan 1.3 unified layout
    .loadOp = VK_ATTACHMENT_LOAD_OP_CLEAR,
    .storeOp = VK_ATTACHMENT_STORE_OP_STORE,
    .clearValue = {.color = {0.0f, 0.0f, 0.2f, 1.0f}}
};

VkRenderingAttachmentInfo depth_attachment {
    .sType = VK_STRUCTURE_TYPE_RENDERING_ATTACHMENT_INFO,
    .imageView = depth_image_view,
    .imageLayout = VK_IMAGE_LAYOUT_ATTACHMENT_OPTIMAL,
    .loadOp = VK_ATTACHMENT_LOAD_OP_CLEAR,
    .storeOp = VK_ATTACHMENT_STORE_OP_DONT_CARE,
    .clearValue = {.depthStencil = {1.0f, 0}}
};

VkRenderingInfo rendering_info {
    .sType = VK_STRUCTURE_TYPE_RENDERING_INFO,
    .renderArea = {.extent = {.width = window_width, .height = window_height}},
    .layerCount = 1,
    .colorAttachmentCount = 1,
    .pColorAttachments = &color_attachment,
    .pDepthAttachment = &depth_attachment,
};

vkCmdBeginRendering(cmd_buffer, &rendering_info);
// ... draw commands ...
vkCmdEndRendering(cmd_buffer);
```

**Benefits:**
- No render pass creation/management
- No tight coupling between pipeline and render pass
- Simpler attachment configuration
- Easier to modify at runtime

**Pipeline Creation:**
```rust
// Link dynamic rendering to pipeline creation
VkPipelineRenderingCreateInfo rendering_ci {
    .sType = VK_STRUCTURE_TYPE_PIPELINE_RENDERING_CREATE_INFO,
    .colorAttachmentCount = 1,
    .pColorAttachmentFormats = &swapchain_format,
    .depthAttachmentFormat = depth_format
};

VkGraphicsPipelineCreateInfo pipeline_ci {
    .sType = VK_STRUCTURE_TYPE_GRAPHICS_PIPELINE_CREATE_INFO,
    .pNext = &rendering_ci,  // Pass via pNext
    // ... no renderPass field needed ...
};
```

### 2. Buffer Device Address (BDA)

**Replaces:** Descriptors for uniform buffers

Access buffers directly via pointers instead of descriptor management:

```rust
// Enable in device creation
VkPhysicalDeviceVulkan12Features vk12_features {
    .sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_VULKAN_1_2_FEATURES,
    .bufferDeviceAddress = true
};

// Create buffer with device address support
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

// Pass address via push constants (tiny compared to UBO descriptors!)
vkCmdPushConstants(cmd_buffer, pipeline_layout,
                   VK_SHADER_STAGE_VERTEX_BIT,
                   0, sizeof(VkDeviceAddress), &device_address);
```

**Shader Access:**
```glsl
// Shader can access buffer via pointer
struct ShaderData {
    mat4 projection;
    mat4 view;
    mat4 model[3];
    vec4 lightPos;
    uint selected;
};

[shader("vertex")]
VSOutput main(VSInput input, uniform ShaderData *shaderData, uint instanceIndex : SV_VulkanInstanceID) {
    VSOutput output;
    // Access like a normal pointer!
    float4x4 modelMat = shaderData->model[instanceIndex];
    output.Pos = mul(shaderData->projection, mul(shaderData->view, mul(modelMat, float4(input.Pos.xyz, 1.0))));
    return output;
}
```

**Benefits:**
- No descriptor set layout/pool/management
- No descriptor updates
- Just a pointer passed via push constants
- Scales to any number of buffers

### 3. Descriptor Indexing (Bindless)

**Replaces:** Per-texture descriptor sets

Put all textures in one large array, index in shader:

```rust
// Enable required features
VkPhysicalDeviceVulkan12Features vk12_features {
    .sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_VULKAN_1_2_FEATURES,
    .descriptorIndexing = true,
    .shaderSampledImageArrayNonUniformIndexing = true,
    .descriptorBindingVariableDescriptorCount = true,
    .runtimeDescriptorArray = true
};

// Create descriptor set layout with array of textures
VkDescriptorBindingFlags desc_flags {
    VK_DESCRIPTOR_BINDING_VARIABLE_DESCRIPTOR_COUNT_BIT  // Allow variable size
};

VkDescriptorSetLayoutBindingFlagsCreateInfo flags_ci {
    .sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_BINDING_FLAGS_CREATE_INFO,
    .bindingCount = 1,
    .pBindingFlags = &desc_flags
};

VkDescriptorSetLayoutBinding binding {
    .binding = 0,
    .descriptorType = VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER,
    .descriptorCount = static_cast<uint32_t>(textures.size()),  // All textures!
    .stageFlags = VK_SHADER_STAGE_FRAGMENT_BIT,
    .pImmutableSamplers = nullptr
};

VkDescriptorSetLayoutCreateInfo layout_ci {
    .sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO,
    .pNext = &flags_ci,
    .bindingCount = 1,
    .pBindings = &binding
};

uint32_t max_textures = textures.size();
VkDescriptorSetVariableDescriptorCountAllocateInfo count_info {
    .sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_VARIABLE_DESCRIPTOR_COUNT_ALLOCATE_INFO,
    .descriptorSetCount = 1,
    .pDescriptorCounts = &max_textures
};

VkDescriptorSetAllocateInfo alloc_info {
    .sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO,
    .pNext = &count_info,
    .descriptorPool = descriptor_pool,
    .descriptorSetCount = 1,
    .pSetLayouts = &descriptor_set_layout
};

vkAllocateDescriptorSets(device, &alloc_info, &descriptor_set);
```

**Shader Access:**
```glsl
// Declare texture array
Sampler2D textures[];

[shader("fragment")]
float4 main(VSOutput input) : SV_Target {
    // Index directly into array!
    float3 color = textures[NonUniformResourceIndex(input.InstanceIndex)]
                        .Sample(input.UV).rgb;
    return float4(color, 1.0);
}
```

**Benefits:**
- Allocate once at startup
- No per-texture descriptor management
- Scales to thousands of textures
- Index by any value (instance ID, material ID, etc.)

### 4. Synchronization2

**Replaces:** Legacy `vkCmdPipelineBarrier`

More explicit and easier to understand:

```rust
// LEGACY (Vulkan 1.0)
vkCmdPipelineBarrier(
    cmd_buffer,
    src_stage_mask,
    dst_stage_mask,
    dependency_flags,
    memory_barrier_count,
    memory_barriers,
    buffer_memory_barrier_count,
    buffer_barriers,
    image_memory_barrier_count,
    image_barriers
);

// MODERN (Vulkan 1.3)
VkImageMemoryBarrier2 barrier {
    .sType = VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER_2,
    .srcStageMask = VK_PIPELINE_STAGE_2_NONE,
    .srcAccessMask = VK_ACCESS_2_NONE,
    .dstStageMask = VK_PIPELINE_STAGE_2_COLOR_ATTACHMENT_OUTPUT_BIT,
    .dstAccessMask = VK_ACCESS_2_COLOR_ATTACHMENT_READ_BIT |
                     VK_ACCESS_2_COLOR_ATTACHMENT_WRITE_BIT,
    .oldLayout = VK_IMAGE_LAYOUT_UNDEFINED,
    .newLayout = VK_IMAGE_LAYOUT_ATTACHMENT_OPTIMAL,  // Vulkan 1.3!
    .image = swapchain_images[image_index],
    .subresourceRange = {
        .aspectMask = VK_IMAGE_ASPECT_COLOR_BIT,
        .levelCount = 1,
        .layerCount = 1
    }
};

VkDependencyInfo dependency_info {
    .sType = VK_STRUCTURE_TYPE_DEPENDENCY_INFO,
    .imageMemoryBarrierCount = 1,
    .pImageMemoryBarriers = &barrier
};

vkCmdPipelineBarrier2(cmd_buffer, &dependency_info);
```

**Benefits:**
- More explicit (stage and access separate)
- Better structure for chaining
- Easier to understand
- `VK_IMAGE_LAYOUT_ATTACHMENT_OPTIMAL` unifies all attachment layouts

## VMA Integration

**Vulkan Memory Allocator** simplifies memory management:

### Basic Setup

```rust
// Enable BDA support
VmaVulkanFunctions vk_functions {
    .vkGetInstanceProcAddr = vkGetInstanceProcAddr,
    .vkGetDeviceProcAddr = vkGetDeviceProcAddr,
    .vkCreateBuffer = vkCreateBuffer  // Required functions
};

VmaAllocatorCreateInfo allocator_ci {
    .flags = VMA_ALLOCATOR_CREATE_BUFFER_DEVICE_ADDRESS_BIT,
    .physicalDevice = physical_device,
    .device = device,
    .pVulkanFunctions = &vk_functions,
    .instance = instance,
    .vulkanApiVersion = VK_API_VERSION_1_3
};

VmaAllocator allocator;
vmaCreateAllocator(&allocator_ci, &allocator);
```

### Automatic Memory Selection

```rust
// VMA automatically selects the best memory type!
VmaAllocationCreateInfo alloc_ci {
    .flags = VMA_ALLOCATION_CREATE_HOST_ACCESS_SEQUENTIAL_WRITE_BIT |
             VMA_ALLOCATION_CREATE_HOST_ACCESS_ALLOW_TRANSFER_INSTEAD_BIT |
             VMA_ALLOCATION_CREATE_MAPPED_BIT,  // Persistent mapping
    .usage = VMA_MEMORY_USAGE_AUTO,  // Let VMA choose!
    .priority = 1.0f
};

// Create and allocate in one call
VkBuffer buffer;
VmaAllocation allocation;
vmaCreateBuffer(allocator, &buffer_ci, &alloc_ci, &buffer, &allocation, nullptr);
```

### Dedicated Allocations

```rust
// For large images (swapchain attachments, etc.)
VmaAllocationCreateInfo alloc_ci {
    .flags = VMA_ALLOCATION_CREATE_DEDICATED_MEMORY_BIT,  // Separate allocation
    .usage = VMA_MEMORY_USAGE_AUTO
};

vmaCreateImage(allocator, &image_ci, &alloc_ci, &image, &allocation, nullptr);
```

## Frames in Flight

CPU and GPU work in parallel by duplicating shared resources:

```rust
const uint32_t MAX_FRAMES_IN_FLIGHT = 2;  // or 3

struct FrameResources {
    // CPU writes while GPU reads from previous frame
    shader_data_buffer: Buffer,

    // Command buffers
    command_buffer: vk::CommandBuffer,

    // Synchronization
    fence: vk::Fence,           // GPU→CPU signal
    present_semaphore: vk::Semaphore,  // Image acquired
};

struct Renderer {
    frame_resources: Vec<FrameResources>,  // 2-3 copies
    current_frame: usize,
}

impl Renderer {
    fn render(&mut self) {
        let frame_idx = self.current_frame;

        // Wait for GPU to finish using this frame's resources
        self.wait_for_fence(frame_idx);

        // Safe to update now (GPU is using other frame's resources)
        self.update_shader_data(frame_idx);

        // Record and submit
        self.record_commands(frame_idx);
        self.submit(frame_idx);

        // Advance frame
        self.current_frame = (self.current_frame + 1) % MAX_FRAMES_IN_FLIGHT;
    }
}
```

**What to duplicate:**
- CPU/GPU shared buffers (uniform buffers)
- Command buffers
- Fences
- Semaphores (presentation)

**What NOT to duplicate:**
- GPU-only resources (depth images, textures)
- Swapchain images (managed by driver)

## Slang Shading Language

**Slang** is more modern than GLSL with better features:

```cpp
// Slang allows all stages in one file
struct VSInput {
    float3 Pos;
    float3 Normal;
    float2 UV;
};

struct VSOutput {
    float4 Pos : SV_POSITION;
    float3 Normal;
    float2 UV;
    float3 LightVec;
    float3 ViewVec;
    uint InstanceIndex;
};

[shader("vertex")]
VSOutput main(VSInput input, uniform ShaderData *shaderData, uint instanceIndex : SV_VulkanInstanceID) {
    VSOutput output;
    float4x4 modelMat = shaderData->model[instanceIndex];
    output.Normal = mul((float3x3)mul(shaderData->view, modelMat), input.Normal);
    output.UV = input.UV;
    output.Pos = mul(shaderData->projection, mul(shaderData->view, mul(modelMat, float4(input.Pos.xyz, 1.0))));

    float4 fragPos = mul(mul(shaderData->view, modelMat), float4(input.Pos.xyz, 1.0));
    output.LightVec = shaderData->lightPos.xyz - fragPos.xyz;
    output.ViewVec = -fragPos.xyz;
    output.InstanceIndex = instanceIndex;
    return output;
}

[shader("fragment")]
float4 main(VSOutput input) : SV_Target {
    float3 N = normalize(input.Normal);
    float3 L = normalize(input.LightVec);
    float3 V = normalize(input.ViewVec);
    float3 R = reflect(-L, N);

    float3 diffuse = max(dot(N, L), 0.0025);
    float3 specular = pow(max(dot(R, V), 0.0), 16.0) * 0.75;

    // Bindless texture access
    float3 color = textures[NonUniformResourceIndex(input.InstanceIndex)]
                        .Sample(input.UV).rgb;

    return float4(diffuse * color + specular, 1.0);
}
```

**Benefits:**
- All stages in one file
- More modern syntax
- Runtime compilation
- Better tooling
- Compatible with GLSL/HLSL patterns

## Quick Start Checklist

### Device Creation

```rust
VkApplicationInfo app_info {
    .sType = VK_STRUCTURE_TYPE_APPLICATION_INFO,
    .apiVersion = VK_API_VERSION_1_3  // Target 1.3!
};

VkPhysicalDeviceVulkan12Features vk12_features {
    .sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_VULKAN_1_2_FEATURES,
    .pNext = &vk13_features,
    .descriptorIndexing = true,
    .shaderSampledImageArrayNonUniformIndexing = true,
    .descriptorBindingVariableDescriptorCount = true,
    .runtimeDescriptorArray = true,
    .bufferDeviceAddress = true
};

VkPhysicalDeviceVulkan13Features vk13_features {
    .sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_VULKAN_1_3_FEATURES,
    .synchronization2 = true,
    .dynamicRendering = true
};

VkDeviceCreateInfo device_ci {
    .sType = VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO,
    .pNext = &vk12_features,
    // ... queue create infos, extensions ...
};
```

### Render Loop Pattern

```rust
fn render_frame(&mut self) {
    // 1. Wait for fence (CPU→GPU sync)
    vkWaitForFences(device, 1, &fences[frame_idx], VK_TRUE, UINT64_MAX);
    vkResetFences(device, 1, &fences[frame_idx]);

    // 2. Acquire swapchain image
    vkAcquireNextImageKHR(device, swapchain, UINT64_MAX,
                          present_semaphores[frame_idx], VK_NULL_HANDLE, &image_index);

    // 3. Update shader data (safe now - GPU isn't using it)
    memcpy(shader_data_buffers[frame_idx].mapped, &shader_data, sizeof(ShaderData));

    // 4. Reset and record command buffer
    vkResetCommandBuffer(command_buffers[frame_idx], 0);
    vkBeginCommandBuffer(command_buffers[frame_idx], &begin_info);

    // 5. Transition images (Synchronization2)
    VkImageMemoryBarrier2 barriers[2] = {
        // Swapchain image
        {.sType = VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER_2,
         .newLayout = VK_IMAGE_LAYOUT_ATTACHMENT_OPTIMAL,
         .image = swapchain_images[image_index], ...},
        // Depth image
        {.sType = VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER_2,
         .newLayout = VK_IMAGE_LAYOUT_ATTACHMENT_OPTIMAL,
         .image = depth_image, ...}
    };
    vkCmdPipelineBarrier2(cmd, &dependency_info);

    // 6. Begin rendering (Dynamic Rendering)
    vkCmdBeginRendering(cmd, &rendering_info);

    // 7. Set viewport, scissor, bind pipeline
    vkCmdSetViewport(cmd, 0, 1, &viewport);
    vkCmdSetScissor(cmd, 0, 1, &scissor);
    vkCmdBindPipeline(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, pipeline);

    // 8. Bind descriptor set (bindless textures - only once!)
    vkCmdBindDescriptorSets(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS,
                           pipeline_layout, 0, 1, &texture_descriptor_set, 0, nullptr);

    // 9. Pass buffer address via push constants (BDA)
    vkCmdPushConstants(cmd, pipeline_layout, VK_SHADER_STAGE_VERTEX_BIT,
                      0, sizeof(VkDeviceAddress), &shader_data_device_address);

    // 10. Draw
    vkCmdDrawIndexed(cmd, index_count, instance_count, 0, 0, 0);

    // 11. End rendering
    vkCmdEndRendering(cmd);

    // 12. Transition for presentation
    vkCmdPipelineBarrier2(cmd, &present_barrier_info);

    vkEndCommandBuffer(cmd);

    // 13. Submit (with semaphores for GPU→GPU sync)
    VkSubmitInfo submit_info {
        .sType = VK_STRUCTURE_TYPE_SUBMIT_INFO,
        .waitSemaphoreCount = 1,
        .pWaitSemaphores = &present_semaphores[frame_idx],
        .pWaitDstStageMask = &wait_stage,
        .commandBufferCount = 1,
        .pCommandBuffers = &cmd,
        .signalSemaphoreCount = 1,
        .pSignalSemaphores = &render_semaphores[image_index]
    };
    vkQueueSubmit(queue, 1, &submit_info, fences[frame_idx]);

    // 14. Present
    VkPresentInfoKHR present_info {
        .sType = VK_STRUCTURE_TYPE_PRESENT_INFO_KHR,
        .waitSemaphoreCount = 1,
        .pWaitSemaphores = &render_semaphores[image_index],
        .swapchainCount = 1,
        .pSwapchains = &swapchain,
        .pImageIndices = &image_index
    };
    vkQueuePresentKHR(queue, &present_info);

    // 15. Advance frame
    frame_idx = (frame_idx + 1) % MAX_FRAMES_IN_FLIGHT;
}
```

## Migration Guide

### From Vulkan 1.0 to 1.3

| Vulkan 1.0 | Vulkan 1.3 | Benefit |
|------------|------------|---------|
| `VkRenderPass` | Dynamic Rendering | Simpler setup, no coupling |
| Descriptors for buffers | Buffer Device Address | No descriptor management |
| Per-texture descriptors | Bindless arrays | Scales better |
| `vkCmdPipelineBarrier` | `vkCmdPipelineBarrier2` | More explicit |
| Manual memory types | `VMA_MEMORY_USAGE_AUTO` | Automatic selection |
| Map/unmap every frame | Persistent mapping | Better performance |
| Single shared resources | Frames in flight | CPU/GPU parallelism |
| GLSL shaders | Slang | More modern features |

## Code Examples

### Complete Modern Setup

```rust
// 1. Enable features
VkPhysicalDeviceVulkan12Features vk12 {
    .sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_VULKAN_1_2_FEATURES,
    .pNext = &vk13,
    .descriptorIndexing = true,
    .shaderSampledImageArrayNonUniformIndexing = true,
    .descriptorBindingVariableDescriptorCount = true,
    .runtimeDescriptorArray = true,
    .bufferDeviceAddress = true
};

VkPhysicalDeviceVulkan13Features vk13 {
    .sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_VULKAN_1_3_FEATURES,
    .synchronization2 = true,
    .dynamicRendering = true
};

// 2. Create allocator with BDA
VmaAllocatorCreateInfo vma_ci {
    .flags = VMA_ALLOCATOR_CREATE_BUFFER_DEVICE_ADDRESS_BIT,
    .physicalDevice = physical_device,
    .device = device,
    .instance = instance,
    .vulkanApiVersion = VK_API_VERSION_1_3
};
vmaCreateAllocator(&vma_ci, &allocator);

// 3. Create bindless texture descriptor set
uint32_t texture_count = 100;  // Max textures
VkDescriptorSetLayoutBinding tex_binding {
    .descriptorType = VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER,
    .descriptorCount = texture_count,
    .stageFlags = VK_SHADER_STAGE_FRAGMENT_BIT
};
// ... create layout, allocate set, populate with textures ...

// 4. Create per-frame resources with BDA
for i in 0..MAX_FRAMES_IN_FLIGHT {
    VkBufferCreateInfo buf_ci {
        .size = sizeof(ShaderData),
        .usage = VK_BUFFER_USAGE_SHADER_DEVICE_ADDRESS_BIT
    };
    VmaAllocationCreateInfo alloc_ci {
        .flags = VMA_ALLOCATION_CREATE_HOST_ACCESS_SEQUENTIAL_WRITE_BIT |
                 VMA_ALLOCATION_CREATE_MAPPED_BIT,
        .usage = VMA_MEMORY_USAGE_AUTO
    };
    vmaCreateBuffer(allocator, &buf_ci, &alloc_ci,
                    &shader_data_buffers[i].buffer,
                    &shader_data_buffers[i].allocation,
                    &shader_data_buffers[i].mapped);

    // Get device address
    VkBufferDeviceAddressInfo addr_info {
        .sType = VK_STRUCTURE_TYPE_BUFFER_DEVICE_ADDRESS_INFO,
        .buffer = shader_data_buffers[i].buffer
    };
    shader_data_buffers[i].device_address =
        vkGetBufferDeviceAddress(device, &addr_info);
}

// 5. Create pipeline with dynamic rendering
VkPipelineRenderingCreateInfo rendering_ci {
    .sType = VK_STRUCTURE_TYPE_PIPELINE_RENDERING_CREATE_INFO,
    .colorAttachmentCount = 1,
    .pColorAttachmentFormats = &swapchain_format,
    .depthAttachmentFormat = depth_format
};
// ... create graphics pipeline with pNext = &rendering_ci ...

// 6. Render loop uses Synchronization2, Dynamic Rendering, BDA, Bindless
```

## Resources

### Official Documentation
- [How to Vulkan in 2026](https://howtovulkan.com) - **Primary reference for this guide**
- [Vulkan 1.3 Specification](https://registry.khronos.org/vulkan/specs/1.3/html/)
- [Vulkan Guide - Best Practices](https://github.com/KhronosGroup/Vulkan-Guide)
- [Vulkan Samples](https://github.com/KhronosGroup/Vulkan-Samples)

### Libraries
- [Vulkan Memory Allocator (VMA)](https://github.com/GPUOpen-LibrariesAndSDKs/VulkanMemoryAllocator)
- [Slang Shading Language](https://shader-slang.com/)
- [Volk - Meta-loader](https://github.com/zeux/volk)
- [SDL3 - Windowing/Input](https://github.com/libsdl-org/SDL)

### Feature References
- [VK_KHR_dynamic_rendering](https://registry.khronos.org/vulkan/specs/1.3-extensions/man/html/VK_KHR_dynamic_rendering.html)
- [VK_EXT_buffer_device_address](https://registry.khronos.org/vulkan/specs/1.3-extensions/man/html/VK_EXT_buffer_device_address.html)
- [VK_EXT_descriptor_indexing](https://registry.khronos.org/vulkan/specs/1.3-extensions/man/html/VK_EXT_descriptor_indexing.html)
- [VK_KHR_synchronization2](https://registry.khronos.org/vulkan/specs/1.3-extensions/man/html/VK_KHR_synchronization2.html)
