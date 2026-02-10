---
name: vulkan-memory-leak-detector
description: Detect potential memory leaks in Vulkan applications by analyzing resource creation/destruction patterns, RAII wrapper usage, and cleanup paths. Updated for Vulkan 1.3 (2026) with VMA best practices, persistent mapping, and frames in-flight patterns. Use when implementing new Vulkan features or reviewing code for memory issues.
allowed-tools: Read, Grep, Glob, Bash
---

# Vulkan Memory Leak Detector (2026 Edition)

## Overview

This skill helps identify potential memory leaks in Vulkan applications by analyzing resource allocation patterns, RAII wrapper usage, and cleanup paths in the Katla engine, with a focus on **Vulkan 1.3 and VMA best practices**.

## Modern Memory Management Checklist

Before deep-diving into leak detection, verify the code follows modern memory patterns:

### VMA Integration
- [ ] **VMA_MEMORY_USAGE_AUTO** - Automatic memory type selection
- [ ] **Persistent mapping** - Map once, use many times
- [ ] **VMA allocation flags** - Proper host access flags
- [ ] **Dedicated allocations** - For large images

### Frames in Flight
- [ ] **Per-frame resource duplication** - 2-3 copies for CPU/GPU shared resources
- [ ] **GPU-only resources** - Not duplicated (e.g., depth images)
- [ ] **Proper synchronization** - Fences before cleanup

### Cleanup Patterns
- [ ] **RAII wrappers** - All resources wrapped with Drop
- [ ] **Descriptor pool reset** - Instead of individual frees
- [ ] **Command pool reset** - Instead of buffer frees
- [ ] **Hot reload cleanup** - Destroy old pipelines/shaders

## Quick Leak Detection

### 1. Resource Allocation Scan
```bash
# Count Vulkan allocations vs deallocations
echo "=== Resource Creation ==="
grep -rn "vkCreate\|vkAllocate\|vmaCreate" katla_vulkan/src/ | wc -l

echo "=== Resource Destruction ==="
grep -rn "vkDestroy\|vkFree\|vmaDestroy" katla_vulkan/src/ | wc -l

# These should be roughly balanced (excluding global resources like device/instance)
```

### 2. VMA Usage Check
```bash
# Check for VMA_MEMORY_USAGE_AUTO (modern approach)
grep -rn "VMA_MEMORY_USAGE_AUTO" katla_vulkan/src/

# Check for manual memory type selection (legacy)
grep -rn "find_memory_type\|get_memory_type" katla_vulkan/src/

# Check for VMA allocation
grep -rn "vmaCreate\|vmaAllocate" katla_vulkan/src/
grep -rn "vmaDestroy\|vmaFree" katla_vulkan/src/
```

### 3. Missing RAII Wrappers
```bash
# Find Vulkan handles that aren't wrapped in RAII types
grep -rn "vk::" katla_vulkan/src/vulkan/ | \
  grep -v "impl Drop\|fn drop\|vkDestroy\|vkFree\|vmaDestroy" | \
  grep -E "(Handle|Image|Buffer|View|Sampler|Framebuffer|Pipeline|DescriptorSet|ShaderModule)" | \
  head -20
```

### 4. Error Path Cleanup Check
```bash
# Find functions that create resources but might not clean up on error
grep -A 20 "fn create_\|fn new_" katla_vulkan/src/vulkan/*.rs | \
  grep -B 5 "return Err\|?" | \
  grep -v "cleanup\|destroy\|drop\|guard"
```

## Common Memory Leak Patterns

### Pattern 1: Unwrapped Vulkan Handles

**Problem:** Direct Vulkan handle storage without RAII
```rust
// WRONG: Direct handle storage
struct Texture {
    image: vk::Image,
    allocation: VmaAllocation,
    view: vk::ImageView,
}

// CORRECT: RAII wrappers
struct Texture {
    image: VmaImage,  // implements Drop, calls vmaDestroyImage
    view: VkImageView,  // implements Drop, calls vkDestroyImageView
}
```

**Detection:**
```bash
# Find struct fields with raw Vulkan handles
grep -rn "vk::" katla_vulkan/src/ | grep ":" | \
  grep -E "Image|Buffer|View|Sampler|Framebuffer|Pipeline|ShaderModule" | \
  grep -v "impl\|fn drop\|Drop"
```

### Pattern 2: Early Return Without Cleanup

**Problem:** Function returns early without cleaning up partially created resources
```rust
// WRONG: No cleanup on error
fn create_pipeline(device: &Device) -> Result<vk::Pipeline> {
    let shader = create_shader_module(device)?;
    let layout = create_pipeline_layout(device)?;
    let pipeline = device.create_pipeline(/* ... */)?;  // May fail
    // shader and layout leak if pipeline creation fails!
    Ok(pipeline)
}

// CORRECT: RAII ensures cleanup
fn create_pipeline(device: &Device) -> Result<VkPipeline> {
    let shader = VkShaderModule::new(device)?;  // RAII
    let layout = VkPipelineLayout::new(device)?;  // RAII
    let pipeline = VkPipeline::new(device, &shader, &layout)?;
    Ok(pipeline)
    // shader and layout Drop automatically even if pipeline creation fails
}
```

**Detection:**
```bash
# Find functions with multiple resource creations
grep -rn "vkCreate\|vkAllocate\|vmaCreate" katla_vulkan/src/vulkan/*.rs -A 3 | \
  grep -B 3 "vkCreate\|vkAllocate\|vmaCreate" | \
  grep -c "vkCreate"
# If count > 1, verify RAII is used
```

### Pattern 3: Circular References

**Problem:** Parent holds child, child holds reference to parent, neither drops
```rust
// WRONG: Circular reference
struct Framebuffer {
    device: Rc<Device>,
    render_pass: Rc<RenderPass>,
    // render_pass may also hold Rc<Device>
}

// CORRECT: Weak reference or bare reference
struct Framebuffer {
    device: Rc<Device>,
    render_pass: Weak<RenderPass>,  // Weak reference
    // or store render_pass: vk::RenderPass (raw handle, no ownership)
}
```

**Detection:**
```bash
# Find Rc<T> usage in struct definitions
grep -rn "Rc<" katla_vulkan/src/ | grep "struct\|pub struct"
# Verify there are no circular reference chains
```

### Pattern 4: Descriptor Set Leaks

**Problem:** Allocating descriptor sets without freeing
```rust
// WRONG: Pool grows without bound
fn create_descriptor_sets(device: &Device, pool: vk::DescriptorPool, layouts: &[vk::DescriptorSetLayout]) -> Vec<vk::DescriptorSet> {
    device.allocate_descriptor_sets(&vk::DescriptorSetAllocateInfo {
        descriptor_pool: pool,
        // ...
    }).unwrap()
    // Sets are allocated but never freed!
}

// CORRECT: Reset pool (bindless pattern)
// With bindless, you allocate once at startup, never reallocate
fn bindless_textures_init(&mut self) -> Result<()> {
    // Create one large descriptor array
    self.texture_descriptor_set = self.allocate_bindless_descriptor_set()?;
    // Never needs to be freed until shutdown
}

// CORRECT: Reset for per-frame descriptors
fn reset_frame_resources(&mut self) {
    self.device.reset_descriptor_pool(self.descriptor_pool);
    // Reclaims all sets from pool
}
```

**Detection:**
```bash
# Find descriptor set allocation
grep -rn "allocate_descriptor_sets" katla_vulkan/src/
# Verify corresponding reset_descriptor_pool or RAII wrapper
```

### Pattern 5: Command Pool Reset Leaks

**Problem:** Command buffers allocated but not reset/freed
```rust
// WRONG: Allocate without reset
fn record_commands(&mut self) -> vk::CommandBuffer {
    let cmd = self.allocate_command_buffer()?;
    // ... record commands ...
    cmd  // Old buffers accumulate
}

// CORRECT: Reset and reuse (frames in flight)
fn record_commands(&mut self, frame_index: usize) -> vk::CommandBuffer {
    // Command buffers are allocated once per frame
    let cmd = self.command_buffers[frame_index];
    self.device.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())?;
    // ... record commands ...
    cmd
}
```

**Detection:**
```bash
# Find command buffer allocation
grep -rn "allocate_command_buffers" katla_vulkan/src/
# Verify reset_command_buffer or reset_command_pool
```

### Pattern 6: Not Using Persistent Mapping

**Problem:** Frequently mapping/unmapping buffers (inefficient, potential for leaks)
```rust
// LEGACY: Map/unmap every frame
fn update_buffer(&mut self, data: &[u8]) -> Result<()> {
    let mut ptr = std::ptr::null_mut();
    vmaMapMemory(self.allocator, self.allocation, &mut ptr);
    std::ptr::copy_nonoverlapping(data.as_ptr(), ptr as *mut u8, data.len());
    vmaUnmapMemory(self.allocator, self.allocation);
    // If this returns early, memory stays mapped!
    Ok(())
}

// MODERN: Persistent mapping (safe in Vulkan)
struct MappedBuffer {
    buffer: vk::Buffer,
    allocation: VmaAllocation,
    mapped_ptr: *mut c_void,  // Mapped once, stays mapped
}

impl MappedBuffer {
    fn update(&mut self, data: &[u8]) -> Result<()> {
        // No map/unmap, just memcpy
        std::ptr::copy_nonoverlapping(data.as_ptr(), self.mapped_ptr as *mut u8, data.len());
        Ok(())
    }
}
```

**Detection:**
```bash
# Find frequent map/unmap patterns
grep -rn "vmaMapMemory" katla_vulkan/src/
grep -rn "vmaUnmapMemory" katla_vulkan/src/
# If these are in the same function frequently, consider persistent mapping
```

### Pattern 7: Frames in Flight Resource Duplication

**Problem:** Not duplicating CPU/GPU shared resources
```rust
// WRONG: Single shared resource
struct Renderer {
    shader_data_buffer: Buffer,  // CPU and GPU both access!
}

// CORRECT: Per-frame duplication
struct Renderer {
    shader_data_buffers: Vec<Buffer>,  // 2-3 copies
    current_frame: usize,
}

impl Renderer {
    fn update_shader_data(&mut self, data: &ShaderData) {
        // Wait for GPU to finish with this frame's resources
        self.wait_for_frame(self.current_frame);

        // Safe to update now
        let buffer = &self.shader_data_buffers[self.current_frame];
        buffer.update(data);

        // Advance frame
        self.current_frame = (self.current_frame + 1) % self.shader_data_buffers.len();
    }
}
```

**Detection:**
```bash
# Check for per-frame resource patterns
grep -rn "Vec<.*Buffer>\|Vec<.*CommandBuffer>" katla_vulkan/src/
# Verify synchronization (fences) are present
```

## Katla-Specific Resource Tracking

### VulkanContext Resources

These global resources should persist for app lifetime:
- `vk::Instance` - Destroyed on app exit
- `vk::PhysicalDevice` - No cleanup needed
- `vk::Device` - Destroyed on app exit
- `vk::Queue` - No cleanup needed
- `VmaAllocator` - Destroyed on app exit

**Check:**
```bash
# Ensure these are only created once (not in loops)
grep -rn "create_instance\|create_device\|vmaCreateAllocator" katla_vulkan/src/
```

### Per-Frame Resources

These should be duplicated (2-3 copies) for frames in flight:
- `vk::CommandBuffer` - One per frame
- `vk::Semaphore` - One per frame for presentation
- `vk::Fence` - One per frame
- **CPU/GPU shared buffers** - One per frame (e.g., uniform buffers)

These should NOT be duplicated:
- `vk::Framebuffer` - Recreated on swapchain resize
- `vk::Semaphore` - Per-swapchain-image for render complete

**Check:**
```bash
# Find per-frame resource creation
grep -rn "Frame\|frame" katla_vulkan/src/ | \
  grep -E "create|allocate" | \
  grep -v "comment"
# Verify duplication for CPU/GPU shared resources
```

### GPU-Only Resources

These should NOT be duplicated (GPU-only access):
- `vk::Image` (depth attachment) - Single copy is fine
- `vk::ImageView` (depth) - Single copy is fine
- Texture images - Single copy is fine

**Check:**
```bash
# Verify GPU-only resources aren't unnecessarily duplicated
grep -rn "depth\|Depth" katla_vulkan/src/ | \
  grep -E "Vec\|vec\|array\|Array"
```

### Dynamic Resources

These should have explicit lifecycle management with VMA:
- `vk::Buffer` - Use `vmaDestroyBuffer`
- `vk::Image` - Use `vmaDestroyImage`
- `vk::ImageView` - Use `vkDestroyImageView`
- `vk::Sampler` - Use `vkDestroySampler`
- `vk::ShaderModule` - Use `vkDestroyShaderModule`

**Check:**
```bash
# Find buffer/image creation
grep -rn "vmaCreateBuffer\|vmaCreateImage" katla_vulkan/src/
# Verify corresponding vmaDestroyBuffer/vmaDestroyImage in Drop impls
```

## VMA-Specific Leak Detection

### Enable VMA Statistics

```c
// Enable VMA statistics in allocator creation
VmaAllocatorCreateInfo allocator_ci = {
    // ...
    .flags = VMA_ALLOCATOR_CREATE_BUFFER_DEVICE_ADDRESS_BIT,
};

#if VMA_STATS_ENABLED
VmaStatistics stats;
vmaCalculateStatistics(allocator, &stats);
printf("VMA: %zu blocks, %zu allocations\n",
       stats.memoryHeap->blockCount, stats.memoryHeap->allocationCount);
#endif
```

### Common VMA Leaks

**1. Not Destroying VMA Allocations**
```rust
// WRONG: Destroying buffer/image but not allocation
vmaDestroyBuffer(allocator, buffer, nullptr);  // Leaks allocation!

// CORRECT: Destroy both
vmaDestroyBuffer(allocator, buffer, allocation);
```

**2. Memory Fragmentation**
```bash
# Check for many small allocations
grep -rn "vmaCreateBuffer\|vmaCreateImage" katla_vulkan/src/ | \
  grep -E "size.*1024|size.*512|size.*256"

# Consider using suballocation or larger buffers
```

**3. Not Using Dedicated Allocations for Large Images**
```rust
// For large images like swapchain attachments
VmaAllocationCreateInfo alloc_ci {
    .flags = VMA_ALLOCATION_CREATE_DEDICATED_MEMORY_BIT,
    .usage = VMA_MEMORY_USAGE_AUTO
};
```

## Validation Layer Leak Detection

### Enable VK_KHRONOS_validation Leaks Detection

```bash
# Run with validation layer leak detection
export VK_LAYER_KHRONOS_VALIDATION=1
cargo run

# Look for messages like:
# "LEAK: Object 0x123 is a VkDeviceMemory that was not freed"
```

### Common Leak Messages

**1. "UNASSIGNED-CoreValidation-MemTrack-InvalidObj"**
- Using destroyed or invalid object handle
- Fix: Check object lifetime, ensure not double-freed

**2. "LEAK: Object type X is still referenced"**
- Resource not freed before destruction
- Fix: Ensure dependent resources freed first

**3. "Object was created but never destroyed"**
- Missing cleanup in Drop implementation
- Fix: Add destroy call to Drop impl

**4. "VUID-vkFreeMemory-memory-00677"**
- Freeing memory while bound objects still exist
- Fix: Destroy buffers/images before freeing memory (VMA handles this)

## RAII Wrapper Template

### Standard RAII Pattern with VMA

```rust
pub struct VmaBuffer {
    buffer: vk::Buffer,
    allocation: VmaAllocation,
    allocator: Rc<VmaAllocator>,
    mapped_ptr: Option<*mut c_void>,
}

impl VmaBuffer {
    pub fn new(
        allocator: Rc<VmaAllocator>,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
    ) -> Result<Self> {
        let buffer_ci = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage);

        let alloc_ci = VmaAllocationCreateInfo {
            .usage = VMA_MEMORY_USAGE_AUTO,
            .flags = VMA_ALLOCATION_CREATE_HOST_ACCESS_SEQUENTIAL_WRITE_BIT |
                     VMA_ALLOCATION_CREATE_MAPPED_BIT,
            ..Default::default()
        };

        let mut buffer = vk::Buffer::null();
        let mut allocation = VmaAllocation::null();
        let mut mapped_ptr = std::ptr::null_mut();

        unsafe {
            vmaCreateBuffer(
                allocator.as_ptr(),
                &buffer_ci as *const _,
                &alloc_ci as *const _,
                &mut buffer as *mut _,
                &mut allocation as *mut _,
                &mut mapped_ptr as *mut _,
            )?;
        }

        Ok(Self {
            buffer,
            allocation,
            allocator,
            mapped_ptr: if mapped_ptr.is_null() { None } else { Some(mapped_ptr) },
        })
    }

    pub fn update(&self, data: &[u8]) {
        if let Some(ptr) = self.mapped_ptr {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    data.as_ptr(),
                    ptr as *mut u8,
                    data.len(),
                );
            }
        }
    }
}

impl Drop for VmaBuffer {
    fn drop(&mut self) {
        unsafe {
            vmaDestroyBuffer(
                self.allocator.as_ptr(),
                self.buffer,
                self.allocation,
            );
        }
    }
}
```

### Parent-Child Resource Pattern

```rust
pub struct VkImageView {
    handle: vk::ImageView,
    device: Rc<Device>,
}

impl VkImageView {
    pub fn new(device: Rc<Device>, image: vk::Image, create_info: &vk::ImageViewCreateInfo) -> Result<Self> {
        let handle = unsafe { device.create_image_view(create_info, None)? };
        Ok(Self { handle, device })
    }
}

impl Drop for VkImageView {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_image_view(self.handle, None);
        }
    }
}
```

## Leak Detection Checklist

When implementing new Vulkan code:

### Allocation
- [ ] Every `vmaCreate*` has corresponding RAII wrapper
- [ ] RAII wrappers implement `Drop` with proper `vmaDestroy*`
- [ ] Resources use `VMA_MEMORY_USAGE_AUTO`
- [ ] Per-frame resources are duplicated (2-3 copies)
- [ ] GPU-only resources are NOT duplicated
- [ ] Persistent mapping used for frequently updated buffers

### Cleanup
- [ ] All RAII types implement `Drop`
- [ ] `Drop` calls correct `vmaDestroy*` or `vkDestroy*` function
- [ ] Error paths clean up partial allocations (RAII ensures this)
- [ ] No raw `vk::` handles stored in structs (use wrappers)
- [ ] Parent resources outlive children (via `Rc` or lifetimes)
- [ ] Descriptor pools reset instead of individual frees

### Validation
- [ ] Run with `VK_LAYER_KHRONOS_VALIDATION=1`
- [ ] Check for "LEAK" messages
- [ ] Verify no "InvalidObj" messages
- [ ] Test with RenderDoc or similar tools

### Hot Reload
- [ ] Shader modules destroyed after pipeline creation
- [ ] Old pipelines destroyed when hot reloading
- [ ] Descriptor pools reset on reload
- [ ] Command buffers reset after recording

### Frames in Flight
- [ ] CPU/GPU shared resources duplicated
- [ ] Fences used before reusing frame resources
- [ ] Semaphores used for GPU-GPU synchronization
- [ ] No resource sharing between concurrent frames

## Profiling Tools

### Vulkan Memory Allocator (VMA)
VMA provides built-in statistics and profiling:

```c
// Enable statistics
VmaAllocatorCreateInfo allocator_ci = {
    .flags = VMA_ALLOCATOR_CREATE_BUFFER_DEVICE_ADDRESS_BIT,
    .pRecordSettings = &record_settings,  // Enable recording
};

// Dump statistics
VmaStatistics stats;
vmaCalculateStatistics(allocator, &stats);
vmaPrintDetailedStatistics(allocator);
```

### RenderDoc
Use RenderDoc to capture frames and check:
- Resource creation count
- Memory usage per resource
- Resource lifetime
- Memory allocation patterns

### Validation Layer Settings
```bash
# Enable all validation features
export VK_LAYER_KHRONOS_VALIDATION=1
export VK_LAYER_FLAGS_KHRONOS_VALIDATION=validations,best-practices,debugprintf
cargo run 2>&1 | tee validation_output.txt

# Filter for leak messages
grep -i "leak\|not freed" validation_output.txt
```

## Code Review Checklist for Memory Leaks

### RAII and Cleanup
- [ ] All Vulkan handles wrapped in RAII types
- [ ] All RAII types implement `Drop`
- [ ] `Drop` implementations call correct destroy functions
- [ ] No early returns without cleanup (RAII ensures this)
- [ ] VMA used for all buffer/image allocations

### VMA Best Practices
- [ ] Using `VMA_MEMORY_USAGE_AUTO` everywhere
- [ ] Using `VMA_ALLOCATION_CREATE_MAPPED_BIT` for persistent mapping
- [ ] Using `VMA_ALLOCATION_CREATE_DEDICATED_MEMORY_BIT` for large images
- [ ] Not calling `vmaMapMemory`/`vmaUnmapMemory` frequently

### Frames in Flight
- [ ] CPU/GPU shared resources duplicated (2-3 copies)
- [ ] GPU-only resources NOT duplicated
- [ ] Fences used for synchronization
- [ ] No resource sharing between concurrent frames

### Descriptor Management
- [ ] Using bindless textures (allocate once)
- [ ] Descriptor pools reset instead of individual frees
- [ ] No per-frame descriptor set allocation

### Command Buffers
- [ ] Command buffers reset instead of reallocated
- [ ] Command pools reset when appropriate
- [ ] Per-frame command buffers allocated once

### Validation
- [ ] No circular references with `Rc`
- [ ] Per-frame resources have cleanup path
- [ ] Validation layers run without leak messages
- [ ] Shader modules destroyed after pipeline creation
- [ ] Hot reload properly cleans up old resources

## Resources

### Memory Management
- [Vulkan Memory Allocator (VMA)](https://github.com/GPUOpen-LibrariesAndSDKs/VulkanMemoryAllocator)
- [VMA Documentation](https://gpuopen-librariesandsdk.github.io/VulkanMemoryAllocator/html/)
- [Vulkan Spec - Resource Lifetime](https://registry.khronos.org/vulkan/specs/1.3/html/chap7.html)

### Modern Best Practices
- [How to Vulkan in 2026 - VMA Section](https://howtovulkan.com)
- [Vulkan Guide - Memory Allocation](https://github.com/KhronosGroup/Vulkan-Guide/blob/main/chapters/memory.adoc)

### Validation
- [Vulkan Validation Layers - Leaks](https://github.com/KhronosGroup/Vulkan-ValidationLayers/blob/master/docs/object_lifetimes.md)
- [Vulkan Best Practices - Resource Management](https://github.com/KhronosGroup/Vulkan-Guide/blob/main/chapters/adoption_recommendations.adoc)
