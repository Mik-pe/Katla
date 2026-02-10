---
name: vulkan-memory-leak-detector
description: Detect potential memory leaks in Vulkan applications by analyzing resource creation/destruction patterns, RAII wrapper usage, and cleanup paths. Use when implementing new Vulkan features or reviewing code for memory issues.
allowed-tools: Read, Grep, Glob, Bash
---

# Vulkan Memory Leak Detector

## Overview

This skill helps identify potential memory leaks in Vulkan applications by analyzing resource allocation patterns, RAII wrapper usage, and cleanup paths in the Katla engine.

## Quick Leak Detection

### 1. Resource Allocation Scan
```bash
# Count Vulkan allocations vs deallocations
echo "=== Resource Creation ==="
grep -rn "vkCreate\|vkAllocate" katla_vulkan/src/ | wc -l

echo "=== Resource Destruction ==="
grep -rn "vkDestroy\|vkFree" katla_vulkan/src/ | wc -l

# These should be roughly balanced (excluding global resources like device/instance)
```

### 2. Missing RAII Wrappers
```bash
# Find Vulkan handles that aren't wrapped in RAII types
grep -rn "vk::" katla_vulkan/src/vulkan/ | \
  grep -v "impl Drop\|fn drop\|vkDestroy\|vkFree" | \
  grep -E "(Handle|Image|Buffer|View|Sampler|Framebuffer|RenderPass|Pipeline|DescriptorSet|ShaderModule)" | \
  head -20
```

### 3. Error Path Cleanup Check
```bash
# Find functions that create resources but might not clean up on error
grep -A 20 "fn create_\|fn new_" katla_vulkan/src/vulkan/*.rs | \
  grep -B 5 "return Err\|?" | \
  grep -v "cleanup\|destroy\|drop"
```

## Common Memory Leak Patterns

### Pattern 1: Unwrapped Vulkan Handles

**Problem:** Direct Vulkan handle storage without RAII
```rust
// WRONG: Direct handle storage
struct Texture {
    image: vk::Image,
    memory: vk::DeviceMemory,
    view: vk::ImageView,
}

// CORRECT: RAII wrappers
struct Texture {
    image: VulkanoImage,  // implements Drop
    memory: VulkanoDeviceMemory,  // implements Drop
    view: VulkanoImageView,  // implements Drop
}
```

**Detection:**
```bash
# Find struct fields with raw Vulkan handles
grep -rn "vk::" katla_vulkan/src/ | grep ":" | \
  grep -E "Image|Buffer|View|Sampler|Framebuffer|RenderPass|Pipeline|ShaderModule" | \
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

// CORRECT: Cleanup on error path
fn create_pipeline(device: &Device) -> Result<vk::Pipeline> {
    let shader = create_shader_module(device)?;
    defer! { device.destroy_shader_module(shader); }  // Cleanup on exit

    let layout = create_pipeline_layout(device)?;
    defer! { device.destroy_pipeline_layout(layout); }  // Cleanup on exit

    let pipeline = device.create_pipeline(/* ... */)?;
    Ok(pipeline)
    // defer blocks run automatically
}
```

**Detection:**
```bash
# Find functions with multiple resource creations
grep -rn "vkCreate\|vkAllocate" katla_vulkan/src/vulkan/*.rs -A 3 | \
  grep -B 3 "vkCreate\|vkAllocate" | \
  grep -c "vkCreate"
# If count > 1, verify there's cleanup
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
```

**CORRECT:**
```rust
// Use descriptor pool reset (reclaims all sets)
fn reset_frame_resources(&mut self) {
    self.device.reset_descriptor_pool(self.descriptor_pool);
}

// Or free individual sets
fn cleanup_descriptor_sets(&mut self, sets: &[vk::DescriptorSet]) {
    self.device.free_descriptor_sets(self.descriptor_pool, sets);
}
```

**Detection:**
```bash
# Find descriptor set allocation without deallocation
grep -rn "allocate_descriptor_sets" katla_vulkan/src/
# Verify corresponding reset_descriptor_pool or free_descriptor_sets
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

// CORRECT: Reset and reuse
fn record_commands(&mut self) -> vk::CommandBuffer {
    self.device.reset_command_pool(self.command_pool);
    let cmd = self.allocate_command_buffer()?;
    // ... record commands ...
    cmd
}
```

**Detection:**
```bash
# Find command buffer allocation
grep -rn "allocate_command_buffers" katla_vulkan/src/
# Verify reset_command_pool or free_command_buffers
```

## Katla-Specific Resource Tracking

### VulkanContext Resources

These global resources should persist for app lifetime:
- `vk::Instance` - Destroyed on app exit
- `vk::PhysicalDevice` - No cleanup needed
- `vk::Device` - Destroyed on app exit
- `vk::Queue` - No cleanup needed

**Check:**
```bash
# Ensure these are only created once (not in loops)
grep -rn "create_instance\|create_device" katla_vulkan/src/
```

### Per-Frame Resources

These should be reset/freed each frame:
- `vk::CommandBuffer` - Reset command pool or free buffers
- `vk::Framebuffer` - Destroyed on swapchain recreation
- `vk::Semaphore` - Destroyed per-frame
- `vk::Fence` - Destroyed per-frame

**Check:**
```bash
# Find per-frame resource creation
grep -rn "Frame\|frame" katla_vulkan/src/ | \
  grep -E "create|allocate" | \
  grep -v "comment"
# Verify cleanup in frame end/resize handlers
```

### Dynamic Resources

These should have explicit lifecycle management:
- `vk::Buffer` - Destroyed when no longer needed
- `vk::Image` - Destroyed when no longer needed
- `vk::DeviceMemory` - Destroyed when buffer/image destroyed
- `vk::ImageView` - Destroyed when image destroyed
- `vk::Sampler` - Destroyed when no longer needed
- `vk::ShaderModule` - Destroyed after pipeline creation

**Check:**
```bash
# Find buffer/image creation
grep -rn "create_buffer\|create_image" katla_vulkan/src/
# Verify corresponding destroy calls in Drop impls
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

## RAII Wrapper Template

### Standard RAII Pattern

```rust
pub struct VkBuffer<T> {
    handle: vk::Buffer,
    device: Rc<Device>,
    _phantom: PhantomData<T>,
}

impl VkBuffer {
    pub fn new(device: Rc<Device>, create_info: &vk::BufferCreateInfo) -> Result<Self> {
        let handle = unsafe { device.create_buffer(create_info, None)? };
        Ok(Self {
            handle,
            device,
            _phantom: PhantomData,
        })
    }

    pub fn handle(&self) -> vk::Buffer {
        self.handle
    }
}

impl Drop for VkBuffer {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_buffer(self.handle, None);
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
- [ ] Every `vkCreate*` has corresponding RAII wrapper
- [ ] Every `vkAllocate*` has cleanup path
- [ ] Resources are not allocated in loops without reuse strategy
- [ ] Per-frame resources are reset/freed each frame

### Cleanup
- [ ] All RAII types implement `Drop`
- [ ] `Drop` calls correct `vkDestroy*` function
- [ ] Error paths clean up partial allocations
- [ ] No raw `vk::` handles stored in structs (use wrappers)
- [ ] Parent resources outlive children (via `Rc` or lifetimes)

### Validation
- [ ] Run with `VK_LAYER_KHRONOS_VALIDATION=1`
- [ ] Check for "LEAK" messages
- [ ] Verify no "InvalidObj" messages
- [ ] Test with valgrind (Linux) or LeakSanitizer

### Hot Reload
- [ ] Shader modules destroyed after pipeline creation
- [ ] Old pipelines destroyed when hot reloading
- [ ] Descriptor pools reset on reload
- [ ] Command buffers reset after recording

## Profiling Tools

### Vulkan Memory Allocator (VMA)
Consider integrating [Vulkan Memory Allocator](https://github.com/GPUOpen-LibrariesAndSDKs/VulkanMemoryAllocator) for advanced memory tracking.

### RenderDoc
Use RenderDoc to capture frames and check:
- Resource creation count
- Memory usage per resource
- Resource lifetime

### Validation Layer Settings
```bash
# Enable all validation features
export VK_LAYER_KHRONOS_VALIDATION=1
export VK_LAYER_FLAGS_KHRONOS_VALIDATION=validations,best-practices,debugprintf, synchronize
cargo run 2>&1 | tee validation_output.txt

# Filter for leak messages
grep -i "leak\|not freed" validation_output.txt
```

## Code Review Checklist for Memory Leaks

- [ ] All Vulkan handles wrapped in RAII types
- [ ] All RAII types implement `Drop`
- [ ] `Drop` implementations call correct destroy functions
- [ ] No early returns without cleanup (use defer blocks or guard pattern)
- [ ] Descriptor sets are freed or pools reset
- [ ] Command buffers are reset or freed
- [ ] No circular references with `Rc`
- [ ] Per-frame resources have cleanup path
- [ ] Validation layers run without leak messages
- [ ] Shader modules destroyed after pipeline creation

## Resources

- [Vulkan Spec - Resource Lifetime](https://registry.khronos.org/vulkan/specs/1.3/html/chap7.html)
- [Vulkan Memory Allocator - Documentation](https://gpuopen-librariesandsdk.github.io/VulkanMemoryAllocator/html/)
- [Vulkan Validation Layers - Leaks](https://github.com/KhronosGroup/Vulkan-ValidationLayers/blob/master/docs/object_lifetimes.md)
