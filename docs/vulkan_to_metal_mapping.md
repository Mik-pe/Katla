# Vulkan-to-Metal API Mapping Reference

> **Scope (August 2026):** this maps the *Vulkan* API surface (`ash` backend) to
> Metal equivalents. The native Metal backend does not pass through
> MoltenVK/SPIR-V — it compiles WGSL directly to MSL via naga. For the current
> Metal architecture see [`metal_backend.md`](metal_backend.md); this document is
> a historical migration aid.

Comprehensive mapping of Vulkan API calls used in the Katla engine to their Metal equivalents.
Based on actual usage patterns found in the `katla_gfx` crate via the `ash` Rust bindings.

---

## 1. Device Initialization and Queues

### Instance and Device Creation

| Vulkan (`ash`) | Metal | Notes |
|---|---|---|
| `Entry::load()` | N/A (Metal is always available on Apple platforms) | No loader needed; Metal is a system framework |
| `vk::ApplicationInfo` | `MTLCreateSystemDefaultDevice()` | Metal has no application info concept |
| `vk::InstanceCreateInfo` → `entry.create_instance()` | `MTLCreateSystemDefaultDevice()` | Metal returns the default device; use `MTLCopyAllDevices()` for multi-GPU |
| `vk::DeviceCreateInfo` → `instance.create_device()` | The `MTLDevice` returned above IS the device | No separate logical device creation in Metal |
| `vk::PhysicalDevice` | `MTLDevice` | Metal uses `MTLDevice` directly; query via `MTLCopyAllDevices()` |
| `instance.enumerate_physical_devices()` | `MTLCopyAllDevices()` | Returns `NSArray<MTLDevice>` |
| `instance.get_physical_device_properties()` | `device.name`, `device.maxThreadsPerThreadgroup`, etc. | Scattered across `MTLDevice` properties |
| `instance.get_physical_device_format_properties()` | `device.supportsTexturePixelFormat()`, `device.supportsVertexPixelFormat()` | Per-format queries; no combined struct |
| `instance.get_physical_device_queue_family_properties()` | N/A | Metal queues are created objects, not queried |

### Queue Families and Queues

| Vulkan (`ash`) | Metal | Notes |
|---|---|---|
| `vk::QueueFlags::GRAPHICS` | Any `MTLCommandQueue` supports graphics | Metal has no queue family concept |
| `vk::QueueFlags::TRANSFER` | Any `MTLCommandQueue` supports blit operations | No dedicated transfer queue type |
| `vk::QueueFlags::COMPUTE` | Any `MTLCommandQueue` supports compute | No dedicated compute queue type |
| `device.get_device_queue(family, index)` | `device.newCommandQueue()` or `device.newCommandQueueWithMaxCommandBufferCount()` | Create as many queues as needed |
| `vk::Queue` | `MTLCommandQueue` | |
| `device.queue_submit(queue, submit_infos, fence)` | `[commandQueue commitCommandBuffer:cmdBuffer]` | Per-command-buffer, not batched |
| `device.queue_wait_idle(queue)` | `[commandBuffer waitUntilCompleted]` | Per-command-buffer wait |
| `device.device_wait_idle()` | No direct equivalent; wait on all in-flight command buffers | Must track all command buffers manually |

**Gotcha:** Metal has no queue families. Any `MTLCommandQueue` can perform graphics, compute, and blit. Vulkan's queue family selection logic can be completely dropped.

---

## 2. Memory Allocation

Katla uses `gpu_allocator` (Vulkan Memory Allocator-like) for sub-allocation.

| Vulkan (`ash`) | Metal | Notes |
|---|---|---|
| `vk::MemoryRequirements` (from `get_buffer_memory_requirements`) | N/A (Metal manages memory internally) | Metal buffers/images are backed by system-managed memory |
| `gpu_allocator::Allocator` with `AllocationCreateDesc` | N/A | No explicit memory allocation in Metal |
| `vk::MemoryLocation::GpuOnly` | `MTLResourceStorageModePrivate` | GPU-only, not CPU accessible |
| `vk::MemoryLocation::CpuToGpu` | `MTLResourceStorageModeShared` | Shared CPU/GPU memory (Unified Memory Architecture on Apple Silicon) |
| `vk::MemoryLocation::GpuToCpu` | `MTLResourceStorageModeShared` | Same as above on UMA |
| `device.bind_buffer_memory(buffer, memory, offset)` | N/A | Metal binds memory at creation time |
| `device.bind_image_memory(image, memory, offset)` | N/A | Metal binds memory at creation time |
| `allocation.mapped_ptr()` | `[buffer contents]` | For `MTLStorageModeShared`, CPU pointer is always available |
| `device.flush_mapped_memory_ranges()` | `[buffer didModifyRange:]` | Only needed for `MTLStorageModeManaged` (non-UMA discrete GPUs) |
| `device.invalidate_mapped_memory_ranges()` | `[buffer synchronizeResource:]` (non-coherent managed memory) | Usually not needed on Apple Silicon |
| `vk::DeviceSize non_coherent_atom_size` | N/A on Apple Silicon; 256 bytes on Intel Mac with discrete GPU | Alignment requirement for flush/invalidate |
| `vk::MemoryPropertyFlags::DEVICE_COHERENT` | Always coherent on Apple Silicon | |

**Gotcha:** Metal on Apple Silicon uses unified memory. `MTLStorageModeShared` is the norm for CPU-accessible data. The entire `gpu_allocator` sub-allocation layer can be replaced by simply creating Metal buffers with the right storage mode. For GPU-only resources, use `MTLStorageModePrivate`.

---

## 3. Command Buffers and Pools

| Vulkan (`ash`) | Metal | Notes |
|---|---|---|
| `vk::CommandPool` | `MTLCommandQueue` (loosely) | Metal has no separate pool concept |
| `device.create_command_pool()` | N/A (command queue acts as the pool) | |
| `device.reset_command_pool()` | N/A | Metal command buffers are one-shot |
| `device.destroy_command_pool()` | Release `MTLCommandQueue` | |
| `vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER` | N/A | Metal command buffers are always single-use |
| `vk::CommandBufferAllocateInfo` → `device.allocate_command_buffers()` | `[commandQueue commandBuffer]` or `[commandQueue commandBufferWithUnretainedReferences]` | |
| `vk::CommandBufferLevel::PRIMARY` | `MTLCommandBuffer` | |
| `vk::CommandBufferLevel::SECONDARY` | `MTLParallelRenderCommandEncoder` (partial) | Metal has no true secondary command buffers |
| `device.begin_command_buffer()` | `[commandQueue commandBuffer]` + begin encoding | Metal command buffers are created ready-to-use |
| `device.end_command_buffer()` | `[encoder endEncoding]` + `[commandBuffer commit]` | |
| `device.reset_command_buffer()` | Create a new `MTLCommandBuffer` | Metal command buffers are not reset; create new ones |
| `device.free_command_buffers()` | Release `MTLCommandBuffer` | ARC manages lifetime |
| `vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT` | Always the case in Metal | |

**Secondary Command Buffers:**
| Vulkan | Metal | Notes |
|---|---|---|
| `vk::CommandBufferInheritanceInfo` | N/A | Metal has no inheritance concept |
| `device.cmd_execute_commands(secondary)` | `MTLParallelRenderCommandEncoder` | Limited equivalence; Metal parallel encoders share a render pass |

**Gotcha:** Metal command buffers are inherently one-shot. The entire pool/allocate/reset pattern collapses to "get a new command buffer from the queue." Secondary command buffers for parallel recording have limited support via `MTLParallelRenderCommandEncoder`.

---

## 4. Buffers

### Buffer Creation

| Vulkan (`ash`) | Metal | Notes |
|---|---|---|
| `vk::BufferCreateInfo` → `device.create_buffer()` | `device.newBufferWithLength:options:` | Metal creates buffer + memory in one call |
| `vk::BufferUsageFlags::VERTEX_BUFFER` | `MTLBuffer` (usage is implicit at bind time) | Metal buffers have no explicit usage flags |
| `vk::BufferUsageFlags::INDEX_BUFFER` | `MTLBuffer` | Same as above |
| `vk::BufferUsageFlags::UNIFORM_BUFFER` | `MTLBuffer` | Same as above |
| `vk::BufferUsageFlags::STORAGE_BUFFER` | `MTLBuffer` | Same as above |
| `vk::BufferUsageFlags::TRANSFER_SRC` | `MTLBuffer` | Same as above |
| `vk::BufferUsageFlags::TRANSFER_DST` | `MTLBuffer` | Same as above |
| `vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS` | `buffer.gpuAddress` (Metal 3+) | Requires `MTLGPUAddress` support |
| `vk::SharingMode::EXCLUSIVE` | Default behavior | |

### Buffer Binding

| Vulkan (`ash`) | Metal | Notes |
|---|---|---|
| `cmd_bind_vertex_buffers(cmd, first, buffers, offsets)` | `[renderEncoder setVertexBuffer:offset:atIndex:]` | |
| `cmd_bind_index_buffer(cmd, buffer, offset, type)` | `[renderEncoder setIndexBuffer:offset:]` with draw call index type | |
| `vk::IndexType::UINT16` | `MTLIndexTypeUInt16` | |
| `vk::IndexType::UINT32` | `MTLIndexTypeUInt32` | |
| `vk::IndexType::UINT8_EXT` | No direct equivalent | Metal doesn't support 8-bit indices natively |

### Buffer Data Upload

| Vulkan (`ash`) | Metal | Notes |
|---|---|---|
| `vk::BufferCreateInfo` with `TRANSFER_SRC` + mapped staging buffer | `device.newBufferWithBytes:length:options:` for initial data | |
| `cmd_copy_buffer_to_image()` | `[blitEncoder copyFromBuffer:toTexture:]` | |
| `std::ptr::copy_nonoverlapping()` into mapped buffer | `[buffer contents]` + memcpy, then `[buffer didModifyRange:]` | For `MTLStorageModeShared`, just write |
| `context.flush_mapped_memory()` | `[buffer didModifyRange:]` | Only for managed storage mode |

---

## 5. Textures / Images

### Image Creation

| Vulkan (`ash`) | Metal | Notes |
|---|---|---|
| `vk::ImageCreateInfo` → `device.create_image()` | `device.newTextureWithDescriptor:` | |
| `vk::ImageType::TYPE_2D` | `MTLTextureType2D` | |
| `vk::Format::R8G8B8A8_SRGB` | `MTLPixelFormatRGBA8Unorm_sRGB` | |
| `vk::Format::R8G8B8A8_UNORM` | `MTLPixelFormatRGBA8Unorm` | |
| `vk::Format::B8G8R8A8_SRGB` | `MTLPixelFormatBGRA8Unorm_sRGB` | |
| `vk::Format::D32_SFLOAT` | `MTLPixelFormatDepth32Float` | |
| `vk::Format::D32_SFLOAT_S8_UINT` | `MTLPixelFormatDepth32Float_Stencil8` | |
| `vk::Format::D24_UNORM_S8_UINT` | `MTLPixelFormatDepth24Unorm_Stencil8` | |
| `vk::Format::R16G16B16A16_SFLOAT` | `MTLPixelFormatRGBA16Float` | For HDR render targets |
| `vk::Format::R32_SFLOAT` | `MTLPixelFormatR32Float` | |
| `vk::Format::R32_UINT` | `MTLPixelFormatR32Uint` | |
| `vk::ImageTiling::OPTIMAL` | Default (no tiling choice in Metal) | |
| `vk::ImageUsageFlags::COLOR_ATTACHMENT` | `MTLTextureUsageRenderTarget` | |
| `vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT` | `MTLTextureUsageRenderTarget` | |
| `vk::ImageUsageFlags::SAMPLED` | `MTLTextureUsageShaderRead` | |
| `vk::ImageUsageFlags::STORAGE` | `MTLTextureUsageShaderWrite` (or `ShaderReadWrite`) | |
| `vk::ImageUsageFlags::TRANSFER_DST` | Implicit (blit encoder handles transfers) | |
| `vk::ImageUsageFlags::TRANSFER_SRC` | Implicit | |
| `vk::SampleCountFlags::TYPE_1` | `MTLSampleCount1` | |
| `vk::SharingMode::EXCLUSIVE` | Default | |

### Image View Creation

| Vulkan (`ash`) | Metal | Notes |
|---|---|---|
| `vk::ImageViewCreateInfo` → `device.create_image_view()` | `texture.newTextureViewWithPixelFormat:` | Metal texture views are created from existing textures |
| `vk::ImageViewType::TYPE_2D` | `MTLTextureType2D` | |
| `vk::ComponentMapping` (swizzle) | `texture.newTextureViewWithPixelFormat:textureType:levels:slices:swizzle:` | Metal supports component swizzle |
| `vk::ImageSubresourceRange` | `levels:` and `slices:` parameters | |
| `vk::ImageAspectFlags::COLOR` | Implicit by pixel format | |
| `vk::ImageAspectFlags::DEPTH` | Implicit by pixel format | |
| `vk::ImageAspectFlags::STENCIL` | Implicit by pixel format | |

### Image Layout Transitions

| Vulkan Layout | Metal Equivalent | Notes |
|---|---|---|
| `vk::ImageLayout::UNDEFINED` | No Metal equivalent (start of life) | Metal doesn't track layouts; synchronization is explicit |
| `vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL` | Render encoder with texture as color attachment | Implicit by usage in `MTLRenderPassDescriptor` |
| `vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL` | Render encoder with texture as depth attachment | Implicit by usage |
| `vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL` | Texture bound to shader via `[encoder setTexture:]` | Implicit by usage |
| `vk::ImageLayout::TRANSFER_DST_OPTIMAL` | Blit encoder destination | Implicit by usage |
| `vk::ImageLayout::TRANSFER_SRC_OPTIMAL` | Blit encoder source | Implicit by usage |
| `vk::ImageLayout::PRESENT_SRC_KHR` | `presentDrawable:` | Implicit by presentation call |

**Gotcha:** Metal has NO image layout concept. Synchronization is achieved via `MTLFence`, `MTLEvent`, or resource barriers (Metal 3). The entire `ImageBarrier` system must be translated to Metal synchronization primitives. On Apple Silicon with unified memory, many transitions become no-ops.

---

## 6. Render Passes (Dynamic Rendering)

Katla uses Vulkan 1.3 dynamic rendering (no `VkRenderPass` objects).

| Vulkan (`ash`) | Metal | Notes |
|---|---|---|
| `vk::RenderingInfo` → `device.cmd_begin_rendering()` | `MTLRenderPassDescriptor` + `[commandBuffer renderCommandEncoderWithDescriptor:]` | |
| `vk::RenderingAttachmentInfo` (color) | `renderPassDescriptor.colorAttachments[0]` | |
| `vk::RenderingAttachmentInfo` (depth) | `renderPassDescriptor.depthAttachment` | |
| `vk::RenderingAttachmentInfo` (stencil) | `renderPassDescriptor.stencilAttachment` | |
| `vk::AttachmentLoadOp::LOAD` | `MTLLoadActionLoad` | |
| `vk::AttachmentLoadOp::CLEAR` | `MTLLoadActionClear` | |
| `vk::AttachmentLoadOp::DONT_CARE` | `MTLLoadActionDontCare` | |
| `vk::AttachmentStoreOp::STORE` | `MTLStoreActionStore` | |
| `vk::AttachmentStoreOp::DONT_CARE` | `MTLStoreActionDontCare` | |
| `vk::RenderingInfo.render_area` | `renderPassDescriptor.renderTargetArrayLength` + viewport/scissor | |
| `device.cmd_end_rendering()` | `[renderEncoder endEncoding]` | |
| `vk::PipelineRenderingCreateInfo` | N/A (format info is in render pass descriptor) | |

**Gotcha:** Metal's render pass model is more structured than Vulkan dynamic rendering. Every render pass in Metal requires a `MTLRenderPassDescriptor` that specifies all attachments, their load/store actions, and clear values. This is conceptually between Vulkan's `VkRenderPass` and dynamic rendering.

---

## 7. Pipeline State (Graphics)

| Vulkan (`ash`) | Metal | Notes |
|---|---|---|
| `vk::GraphicsPipelineCreateInfo` | `MTLRenderPipelineDescriptor` + `device.newRenderPipelineStateWithDescriptor:` | |
| `vk::PipelineLayoutCreateInfo` → `device.create_pipeline_layout()` | N/A (Metal has no pipeline layout concept) | Resource binding is done directly at encode time |
| `vk::PipelineShaderStageCreateInfo` (vertex) | `pipelineDescriptor.vertexFunction` | |
| `vk::PipelineShaderStageCreateInfo` (fragment) | `pipelineDescriptor.fragmentFunction` | |
| `vk::PipelineVertexInputStateCreateInfo` | `MTLVertexDescriptor` | |
| `vk::VertexInputBindingDescription` | `vertexDescriptor.layouts[index]` | |
| `vk::VertexInputAttributeDescription` | `vertexDescriptor.attributes[index]` | |
| `vk::PipelineInputAssemblyStateCreateInfo` | Set at draw time in Metal | Primitive topology is per-draw in Metal |
| `vk::PrimitiveTopology::TRIANGLE_LIST` | `MTLPrimitiveTypeTriangle` | |
| `vk::PipelineViewportStateCreateInfo` | `[renderEncoder setViewport:]` | Dynamic state in Metal |
| `vk::PipelineRasterizationStateCreateInfo` | Various properties on `MTLRenderPipelineDescriptor` + rasterizer state | |
| `vk::PolygonMode::FILL` | Default | |
| `vk::PolygonMode::LINE` | `[renderEncoder setTriangleFillMode:MTLTriangleFillModeLines]` | |
| `vk::CullMode::BACK` | `[renderEncoder setCullMode:MTLCullModeBack]` | |
| `vk::CullMode::NONE` | `[renderEncoder setCullMode:MTLCullModeNone]` | |
| `vk::FrontFace::COUNTER_CLOCKWISE` | `[renderEncoder setFrontFacingVertexWinding:MTLWindingCounterClockwise]` | |
| `vk::PipelineMultisampleStateCreateInfo` | `pipelineDescriptor.sampleCount` | |
| `vk::SampleCountFlags::TYPE_1` | `sampleCount = 1` | |
| `vk::PipelineDepthStencilStateCreateInfo` | `MTLDepthStencilDescriptor` + `device.newDepthStencilStateWithDescriptor:` | Separate object in Metal |
| `vk::PipelineColorBlendStateCreateInfo` | `pipelineDescriptor.colorAttachments[0]` (blend state) | |
| `vk::PipelineColorBlendAttachmentState` | `MTLRenderPipelineColorAttachmentDescriptor.blendingEnabled`, `rgbBlendOperation`, etc. | |
| `vk::BlendFactor::SRC_ALPHA` | `MTLBlendFactorSourceAlpha` | |
| `vk::BlendFactor::ONE_MINUS_SRC_ALPHA` | `MTLBlendFactorOneMinusSourceAlpha` | |
| `vk::BlendFactor::ONE` | `MTLBlendFactorOne` | |
| `vk::BlendFactor::ZERO` | `MTLBlendFactorZero` | |
| `vk::BlendOp::ADD` | `MTLBlendOperationAdd` | |
| `vk::ColorComponentFlags` | `MTLColorWriteMask` | |
| `vk::PipelineDynamicStateCreateInfo` | N/A (Metal has no dynamic state enum) | Most states are set dynamically by default |
| `device.create_graphics_pipelines()` | `device.newRenderPipelineStateWithDescriptor:error:` | Synchronous; use `...withDescriptor:options:completionHandler:` for async |
| `vk::PipelineCache` | N/A | Metal handles pipeline caching internally (binary archives in Metal 3) |

**Depth-Stencil State (separate in Metal):**

| Vulkan | Metal | Notes |
|---|---|---|
| `depth_test_enable` | `depthDescriptor.depthCompareFunction` | Set to `MTLCompareFunctionAlways` to disable |
| `depth_write_enable` | `depthDescriptor.depthWriteEnabled` | |
| `depth_compare_op: GREATER_OR_EQUAL` | `MTLCompareFunctionGreaterEqual` | For reverse-Z depth |
| `depth_compare_op: ALWAYS` | `MTLCompareFunctionAlways` | |
| `depth_compare_op: NEVER` | `MTLCompareFunctionNever` | |
| `stencil_test_enable` | `stencilDescriptor` properties | |
| `vk::StencilOpState` | `MTLStencilDescriptor` | |
| `depth_bias_enable` | `[renderEncoder setDepthBias:slopeScale:clamp:]` | Dynamic in Metal |

**Gotcha:** Metal separates depth-stencil state from the graphics pipeline. In Vulkan, it's part of `VkGraphicsPipelineCreateInfo`. In Metal, create a `MTLDepthStencilState` separately and bind it with `[renderEncoder setDepthStencilState:]`.

---

## 8. Pipeline State (Compute)

| Vulkan (`ash`) | Metal | Notes |
|---|---|---|
| `vk::ComputePipelineCreateInfo` | `MTLComputePipelineDescriptor` + `device.newComputePipelineStateWithDescriptor:` | |
| `device.create_compute_pipelines()` | `device.newComputePipelineStateWithFunction:error:` | Usually just a function, not a full descriptor |
| `vk::PipelineShaderStageCreateInfo` (compute) | `MTLFunction` from compute shader library | |
| `device.cmd_dispatch(x, y, z)` | `[computeEncoder dispatchThreadgroups:threadsPerThreadgroup:]` | |
| `device.cmd_dispatch_indirect(buffer, offset)` | `[computeEncoder dispatchThreadgroupsWithIndirectBuffer:indirectBufferOffset:threadsPerThreadgroup:]` | |

**Workgroup Mapping:**
- Vulkan `vkCmdDispatch(group_count_x, group_count_y, group_count_z)` maps directly to Metal `dispatchThreadgroups:MTLSizeMake(group_count_x, group_count_y, group_count_z) threadsPerThreadgroup:MTLSizeMake(workgroup_size_x, workgroup_size_y, workgroup_size_z)`

---

## 9. Shader Compilation

Katla compiles WGSL → SPIR-V via `naga`, then uses SPIR-V with MoltenVK/SPIRV-Cross.

| Vulkan (`ash`) | Metal | Notes |
|---|---|---|
| `vk::ShaderModuleCreateInfo` (SPIR-V) → `device.create_shader_module()` | `device.newLibraryWithSource:options:error:` (MSL source) | Metal compiles MSL directly |
| `vk::ShaderModule` | `MTLLibrary` | |
| `vk::PipelineShaderStageCreateInfo.name` ("vs_main", "fs_main") | `library.newFunctionWithName:` | |
| SPIR-V binary | MSL source code or metallib | Must convert SPIR-V to MSL (use SPIRV-Cross or write MSL directly) |
| `naga` WGSL → SPIR-V | `naga` WGSL → MSL (naga supports MSL backend) | Alternative: keep `naga` but target MSL output |
| `vk::ShaderStageFlags::VERTEX` | `MTLFunction` with vertex stage | |
| `vk::ShaderStageFlags::FRAGMENT` | `MTLFunction` with fragment stage | |
| `vk::ShaderStageFlags::COMPUTE` | `MTLFunction` with kernel stage | |

**Shader Compilation Pipeline:**
- **Current Katla:** WGSL → `naga` → SPIR-V → `vk::ShaderModule` → MoltenVK → Metal
- **Direct Metal:** WGSL → `naga` → MSL → `MTLLibrary` → `MTLFunction`
- **Alternative:** Write MSL directly → `device.newLibraryWithSource:`

**Gotcha:** Metal Shading Language (MSL) uses different entry point declarations. `vs_main` becomes a function with `vertex` qualifier, `fs_main` gets `fragment`, `cs_main` gets `kernel`. Resource binding uses `[[buffer(n)]]`, `[[texture(n)]]`, `[[sampler(n)]]` attributes instead of descriptor sets.

---

## 10. Descriptor Sets / Resource Binding

Katla uses a 3-set descriptor layout:
- **Set 0:** Storage buffers (frame data + per-object array)
- **Set 1:** Bindless textures + shared sampler
- **Set 2:** Skeleton/joint matrices (optional)

| Vulkan (`ash`) | Metal | Notes |
|---|---|---|
| `vk::DescriptorSetLayoutCreateInfo` → `device.create_descriptor_set_layout()` | N/A | Metal has no descriptor set layouts |
| `vk::DescriptorPoolCreateInfo` → `device.create_descriptor_pool()` | N/A | Metal has no descriptor pools |
| `vk::DescriptorSetAllocateInfo` → `device.allocate_descriptor_sets()` | N/A | Metal binds resources directly |
| `vk::WriteDescriptorSet` → `device.update_descriptor_sets()` | `[encoder setBuffer:offset:atIndex:]`, `[encoder setTexture:atIndex:]`, `[encoder setSamplerState:atIndex:]` | Direct binding at encode time |
| `device.cmd_bind_descriptor_sets()` | Individual `[encoder set*]` calls | |
| `vk::DescriptorType::STORAGE_BUFFER` | `[encoder setBuffer:offset:atIndex:]` (vertex/compute/fragement stage) | |
| `vk::DescriptorType::UNIFORM_BUFFER` | `[encoder setBuffer:offset:atIndex:]` | Same API as storage buffer |
| `vk::DescriptorType::SAMPLED_IMAGE` | `[encoder setTexture:atIndex:]` | |
| `vk::DescriptorType::SAMPLER` | `[encoder setSamplerState:atIndex:]` | |
| `vk::DescriptorType::STORAGE_IMAGE` | `[encoder setTexture:atIndex:]` with `MTLTextureUsageShaderWrite` | |
| `vk::PushConstantRange` → `device.cmd_push_constants()` | `[encoder setBytes:length:atIndex:]` | Metal push constants = setBytes with fixed-size data |
| `vk::DescriptorSetLayoutBinding` | Binding index in `[[buffer(N)]]` / `[[texture(N)]]` / `[[sampler(N)]]` | MSL attribute syntax |

**Resource Binding Mapping (Per-Set):**

| Vulkan Descriptor Set | Metal Binding Pattern |
|---|---|
| Set 0, Binding 0: Frame data (storage buffer) | `[encoder setBuffer:frameDataBuffer offset:0 atIndex:0]` (vertex + fragment) |
| Set 0, Binding 1: Object array (storage buffer) | `[encoder setBuffer:objectBuffer offset:0 atIndex:1]` |
| Set 1, Binding 0: Bindless textures | `[encoder setTexture:atIndex:N]` for each texture | 
| Set 1, Binding 1: Shared sampler | `[encoder setSamplerState:sharedSampler atIndex:0]` |
| Set 2, Binding 0: Joint matrices | `[encoder setBuffer:jointBuffer offset:0 atIndex:N]` |

**VK_KHR_push_descriptor Usage:**
| Vulkan | Metal | Notes |
|---|---|---|
| `vkCmdPushDescriptorSetKHR()` | `[encoder setTexture:atIndex:]` per-draw | Metal always binds resources per-draw; push descriptors are redundant |

---

## 11. Bindless Textures

Katla uses `VK_EXT_descriptor_indexing` for bindless textures (array of 4096 `SAMPLED_IMAGE` descriptors).

| Vulkan (`ash`) | Metal | Notes |
|---|---|---|
| `vk::DescriptorBindingFlags::PARTIALLY_BOUND` | N/A | Metal arrays are always partially bound |
| `vk::DescriptorBindingFlags::UPDATE_AFTER_BIND` | N/A | Metal textures can be updated any time |
| `vk::DescriptorSetLayoutBinding` with `descriptor_count = 4096` | `array<texture2d<float>, 4096>` in MSL | Metal supports argument buffer arrays |
| `vk::DescriptorType::SAMPLED_IMAGE` array | `MTLArgumentEncoder` + argument buffers | Metal's bindless mechanism |
| `vk::WriteDescriptorSet` per texture slot | `[argumentBuffer setTexture:atIndex:N]` | Write to argument buffer |
| `device.update_descriptor_sets()` | Modify argument buffer contents | |
| Shader: `bindless_textures[index]` | `texture_array[index]` in MSL | MSL uses `array<texture2d<float>, N>` |

**Metal Bindless Pattern:**
```metal
// Define an argument buffer with texture array
struct BindlessTextures {
    array<texture2d<float, access::sample>, 4096> textures;
    sampler shared_sampler;
};

// In fragment function:
fragment float4 fs_main(VertexOutput in [[stage_in]],
                        constant BindlessTextures& bindless [[buffer(0)]]) {
    return bindless.textures[in.texture_index].sample(bindless.shared_sampler, in.uv);
}
```

**Gotcha:** Metal's bindless requires `MTLArgumentEncoder` to define the argument buffer layout. This is more structured than Vulkan's descriptor indexing but achieves the same result. On Metal 3, `MTLHeap` + resource heaps can further optimize bindless access.

---

## 12. Synchronization (Fences, Semaphores, Barriers)

### Fences and Semaphores

| Vulkan (`ash`) | Metal | Notes |
|---|---|---|
| `vk::FenceCreateInfo` → `device.create_fence()` | `[commandBuffer addCompletedHandler:]` | Metal uses callbacks, not fences |
| `device.wait_for_fences()` | `[commandBuffer waitUntilCompleted]` | Blocking wait |
| `device.reset_fences()` | N/A | Metal command buffers auto-reset on completion |
| `vk::SemaphoreCreateInfo` → `device.create_semaphore()` | `MTLEvent` or `MTLFence` | |
| `vk::SubmitInfo.wait_semaphores` | `[commandBuffer encodeWaitForEvent:value:]` | |
| `vk::SubmitInfo.signal_semaphores` | `[commandBuffer encodeSignalEvent:value:]` | |

### Pipeline Barriers (Vulkan 1.3 Sync2)

Katla uses `vkCmdPipelineBarrier2` with `vk::DependencyInfo`.

| Vulkan (`ash`) | Metal | Notes |
|---|---|---|
| `vkCmdPipelineBarrier2()` | `MTLFence` or `[encoder updateFence:]` / `[encoder waitForFence:]` | |
| `vk::ImageMemoryBarrier2` | `MTLFence` between encoders | Metal tracks resource state implicitly |
| `vk::BufferMemoryBarrier2` | `MTLFence` between encoders | |
| `vk::PipelineStageFlags2` | Implicit in Metal encoder ordering | |
| `vk::AccessFlags2` | Implicit in Metal | |
| `ImageBarrier::transition_from_undefined()` | Usually a no-op on Metal | Metal doesn't have layouts |
| `ImageBarrier::transition(old, new)` | `[encoder updateFence:]` + next encoder `[encoder waitForFence:]` | If ordering matters between encoders |
| `ImageBarrier::depth_render_pass_sync()` | `MTLFence` between render passes | |

**Pipeline Stage Mapping:**
| Vulkan Stage | Metal Equivalent |
|---|---|
| `TOP_OF_PIPE` | Start of command buffer |
| `BOTTOM_OF_PIPE` | End of command buffer |
| `VERTEX_SHADER` | Vertex stage |
| `FRAGMENT_SHADER` | Fragment stage |
| `COMPUTE_SHADER` | Compute stage (kernel) |
| `TRANSFER` | Blit stage |
| `COLOR_ATTACHMENT_OUTPUT` | Render encoder color output |
| `EARLY_FRAGMENT_TESTS` / `LATE_FRAGMENT_TESTS` | Depth/stencil testing in render encoder |

**Gotcha:** Metal's synchronization model is simpler. Fences (`MTLFence`) ensure ordering between encoders within a command buffer. Events (`MTLEvent`) synchronize across command buffers. Many Vulkan pipeline barriers (especially within the same queue) become implicit in Metal due to its command buffer ordering guarantees. On Apple Silicon, unified memory makes many barriers unnecessary.

---

## 13. Swapchain / Presentation

| Vulkan (`ash`) | Metal | Notes |
|---|---|---|
| `vk::SurfaceKHR` | `CAMetalLayer` | Core Animation layer |
| `vk::SwapchainCreateInfoKHR` → `swapchain_loader.create_swapchain()` | `CAMetalLayer` configuration (`drawableSize`, `pixelFormat`, `maximumDrawableCount`) | |
| `swapchain_loader.get_swapchain_images()` | `[metalLayer nextDrawable]` | Returns `CAMetalDrawable` (conforms to `MTLTexture`) |
| `vk::SwapchainKHR` | No direct object; `CAMetalLayer` manages drawables | |
| `swapchain_loader.acquire_next_image()` | `[metalLayer nextDrawable]` | Blocking or async via `nextDrawableAsync` |
| `vk::PresentInfoKHR` → `queue.present()` | `[commandBuffer presentDrawable:drawable]` | |
| `vk::PresentModeKHR::MAILBOX` | `CAMetalLayer.displaySyncEnabled = NO` + `presentsWithTransaction = NO` | Approximate; Metal doesn't have exact present modes |
| `vk::PresentModeKHR::FIFO` | Default `CAMetalLayer` behavior (vsync) | |
| `vk::CompositeAlphaFlagsKHR::OPAQUE` | Default | |
| `vk::SurfaceFormatKHR` (B8G8R8A8_SRGB) | `CAMetalLayer.pixelFormat = MTLPixelFormatBGRA8Unorm_sRGB` | |
| `vk::ImageUsageFlags::COLOR_ATTACHMENT \| TRANSFER_DST` | Implicit; drawable textures support all usages | |
| Swapchain recreation (resize) | Update `CAMetalLayer.drawableSize` | Simpler in Metal |
| `vk::SurfaceCapabilitiesKHR` | `CAMetalLayer.drawableSize`, `maximumDrawableCount` | |
| `FRAMES_IN_FLIGHT = 2` | `CAMetalLayer.maximumDrawableCount = 3` (triple buffering) | Metal recommends 3 drawables |

**Gotcha:** Metal swapchain management is done through Core Animation (`CAMetalLayer`), not a dedicated API. There's no explicit swapchain creation/destruction — just configure the layer and request drawables. `nextDrawable` can block if no drawable is available.

---

## 14. Buffer Device Address (BDA)

| Vulkan (`ash`) | Metal | Notes |
|---|---|---|
| `vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS` | `buffer.gpuAddress` (Metal 3+, macOS 13+) | |
| `vk::BufferDeviceAddressInfo` → `device.get_buffer_device_address()` | `buffer.gpuAddress` | Direct property access |
| GPU pointer in shader (`uint64_t` address) | `device_ref<T>` or raw pointer in MSL | Metal 3 introduces `MTLGPUAddress` |
| `SHADER_DEVICE_ADDRESS \| UNIFORM_BUFFER \| STORAGE_BUFFER` | `MTLBuffer` with `gpuAddress` | Single buffer can serve all purposes |

**MSL Shader Usage:**
```metal
// Accessing buffer via device address
kernel void cs_main(device float* data [[buffer(0)]],
                    constant uint64_t& address [[buffer(1)]]) {
    device float* other = (device float*)(address);
    // ...
}
```

**Gotcha:** `buffer.gpuAddress` requires Metal 3 (macOS 13+, iOS 16+). On earlier versions, BDA is not available and you must use traditional descriptor/binding approaches.

---

## 15. Samplers

| Vulkan (`ash`) | Metal | Notes |
|---|---|---|
| `vk::SamplerCreateInfo` → `device.create_sampler()` | `MTLSamplerDescriptor` + `device.newSamplerStateWithDescriptor:` | |
| `vk::Filter::LINEAR` | `MTLSamplerMinMagFilterLinear` | |
| `vk::Filter::NEAREST` | `MTLSamplerMinMagFilterNearest` | |
| `vk::SamplerAddressMode::REPEAT` | `MTLSamplerAddressModeRepeat` | |
| `vk::SamplerAddressMode::CLAMP_TO_EDGE` | `MTLSamplerAddressModeClampToEdge` | |
| `vk::SamplerAddressMode::MIRRORED_REPEAT` | `MTLSamplerAddressModeMirrorRepeat` | |
| `vk::SamplerMipmapMode::LINEAR` | `MTLSamplerMipFilterLinear` | |
| `anisotropy_enable = true, max_anisotropy = 16.0` | `samplerDescriptor.maxAnisotropy = 16` | |
| `vk::CompareOp::NEVER` (compare_enable = false) | `samplerDescriptor.compareFunction = MTLCompareFunctionNever` | |
| `vk::BorderColor::INT_OPAQUE_WHITE` | `MTLSamplerBorderColorOpaqueWhite` | |
| `vk::LOD_CLAMP_NONE` | `samplerDescriptor.lodMaxClamp = FLT_MAX` | |
| `unnormalized_coordinates = false` | Default | |

**Repeat Anisotropic Sampler (Katla's `create_sampler_repeat_anisotropic`):**
```swift
let descriptor = MTLSamplerDescriptor()
descriptor.minMagFilter = .linear
descriptor.mipFilter = .linear
descriptor.sAddressMode = .repeat
descriptor.tAddressMode = .repeat
descriptor.rAddressMode = .repeat
descriptor.maxAnisotropy = 16
descriptor.compareFunction = .never
let sampler = device.makeSamplerState(descriptor: descriptor)
```

**Clamp Edge Linear Sampler (Katla's `create_sampler_clamp_edge_linear`):**
```swift
let descriptor = MTLSamplerDescriptor()
descriptor.minMagFilter = .linear
descriptor.mipFilter = .linear
descriptor.sAddressMode = .clampToEdge
descriptor.tAddressMode = .clampToEdge
descriptor.rAddressMode = .clampToEdge
descriptor.maxAnisotropy = 1
descriptor.compareFunction = .never
let sampler = device.makeSamplerState(descriptor: descriptor)
```

---

## 16. Draw Commands

| Vulkan (`ash`) | Metal | Notes |
|---|---|---|
| `cmd_draw(vertex_count, instance_count, first_vertex, first_instance)` | `[renderEncoder drawPrimitives:vertexStart:vertexCount:instanceCount:]` | |
| `cmd_draw_indexed(index_count, instance_count, first_index, vertex_offset, first_instance)` | `[renderEncoder drawIndexedPrimitives:indexCount:indexType:indexBuffer:indexBufferOffset:instanceCount:]` | |
| `device.cmd_bind_pipeline(cmd, GRAPHICS, pipeline)` | Implicit (render encoder state is set per-encoder) | Metal doesn't have pipeline bind; state is set on encoder |
| `device.cmd_bind_pipeline(cmd, COMPUTE, pipeline)` | `[computeEncoder setComputePipelineState:]` | |

---

## 17. Render Graph / Transient Resources

The frame graph creates transient textures with per-frame-in-flight tracking.

| Vulkan Concept | Metal Equivalent | Notes |
|---|---|---|
| `vk::Image` with `UNDEFINED` initial layout | `MTLTexture` created via `device.newTextureWithDescriptor:` | No initial layout concept |
| Image layout tracking (`current_layout: Cell<vk::ImageLayout>`) | Not needed | Metal doesn't have layouts |
| `vk::ImageSubresourceRange` | N/A | Metal operates on the whole texture |
| `FRAMES_IN_FLIGHT = 2` transient textures | Same pattern applies | Keep multiple textures per frame |
| Resource state tracking (`ResourceState` enum) | `MTLFence` or `MTLEvent` between encoders | Track with Metal sync primitives instead |
| Transient texture resize (swapchain recreation) | Destroy old `MTLTexture`, create new with updated descriptor | |

---

## 18. Feature Flags Used

| Vulkan Feature | Metal Equivalent | Notes |
|---|---|---|
| `vk::PhysicalDeviceVulkan12Features::buffer_device_address` | `MTLDevice.supportsGPUAddress` (Metal 3) | |
| `vk::PhysicalDeviceVulkan12Features::descriptor_indexing` | Always supported | Metal has no feature flag for this |
| `vk::PhysicalDeviceVulkan12Features::shader_sampled_image_array_non_uniform_indexing` | Always supported (argument buffers) | |
| `vk::PhysicalDeviceVulkan12Features::runtime_descriptor_array` | Always supported | |
| `vk::PhysicalDeviceVulkan13Features::dynamic_rendering` | Always the case in Metal | No render pass objects needed |
| `vk::PhysicalDeviceVulkan13Features::synchronization2` | Always the case in Metal | |
| `vk::PhysicalDeviceFeatures::sampler_anisotropy` | Always supported on Metal | |
| `VK_KHR_push_descriptor` | N/A (Metal always binds directly) | |
| `VK_KHR_swapchain` | `CAMetalLayer` | |
| `VK_KHR_maintenance4` | N/A | |

---

## Summary: Architectural Differences

### What Gets Simpler in Metal
1. **No descriptor sets/pools** — bind resources directly on encoders
2. **No image layouts** — implicit by how the texture is used
3. **No memory allocation** — Metal manages memory (especially on UMA)
4. **No command pools** — create command buffers directly from queue
5. **No queue families** — any queue does everything
6. **No render pass objects** — `MTLRenderPassDescriptor` is lightweight

### What Gets Different
1. **Pipeline state split** — depth-stencil state is separate from graphics pipeline
2. **Shader language** — MSL instead of SPIR-V (use `naga` MSL backend or SPIRV-Cross)
3. **Synchronization** — `MTLFence`/`MTLEvent` instead of barriers
4. **Bindless textures** — argument buffers with `MTLArgumentEncoder`
5. **Presentation** — `CAMetalLayer` + `nextDrawable` instead of swapchain
6. **Secondary command buffers** — limited support via parallel render encoders

### What Needs Complete Rewrite
1. `gpu_allocator` integration → Not needed (Metal manages memory)
2. `ImageBarrier` system → Replace with `MTLFence`/`MTLEvent` (or remove on UMA)
3. Descriptor set management → Direct resource binding on encoders
4. Swapchain abstraction → `CAMetalLayer` configuration
5. Shader compilation pipeline → `naga` WGSL→MSL or direct MSL compilation
