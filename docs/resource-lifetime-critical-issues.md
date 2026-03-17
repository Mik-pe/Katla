# Vulkan Resource Lifetime Management - Critical Issues

**Date**: 2026-03-16
**Status**: CRITICAL - Multiple resource lifetime violations
**Root Cause**: RAII Drop implementations destroying resources without GPU synchronization

## Executive Summary

Army of 12+ agents revealed **critical Vulkan resource lifetime violations** throughout the Katla codebase. The fundamental issue: **all Vulkan resources are destroyed via Drop implementations without waiting for GPU completion**.

This violates Vulkan's core requirement: **resources must not be destroyed while the GPU is using them**.

## Critical Issues

### 1. Systematic Drop Without Synchronization (13+ locations)

**Problem**: Every Vulkan resource wrapper uses Drop to destroy resources, but none wait for GPU idle.

**Impact**: HIGH - Resources destroyed while GPU may still be using them, causing:
- Validation errors
- GPU crashes
- Memory corruption
- Undefined behavior

**Affected Resources**:
- Textures (image, image view, sampler)
- Vertex/index buffers
- Descriptor sets and pools
- Pipelines and pipeline layouts
- Framebuffer attachments
- Particle system resources

### 2. swap_alive_lists Missing Pre-Copy Barrier

**Location**: `katla_gfx/src/particles/buffer.rs:923-1009`

**Problem**: vkCmdCopyBuffer reads from `alive_next` without ensuring SIMULATE pass writes are visible.

**Current Flow**:
```
SIMULATE (writes to alive_next)
    ↓ [NO BARRIER]
vkCmdCopyBuffer (reads from alive_next) ← READ_AFTER_WRITE hazard
```

**Required**:
```
SIMULATE (writes to alive_next)
    ↓ [BARRIER: SHADER_WRITE → TRANSFER_READ]
vkCmdCopyBuffer (reads from alive_next) ← Safe
```

### 3. Incorrect Barrier Access Masks in swap_alive_lists

**Location**: `katla_gfx/src/particles/buffer.rs:967-995`

**Problem**: Source barrier uses `TRANSFER_READ` but should be `TRANSFER_WRITE`.

**Current (Wrong)**:
```rust
vk::BufferMemoryBarrier::default()
    .src_access_mask(vk::AccessFlags::TRANSFER_READ)  // ❌ Wrong
    .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
```

**Correct**:
```rust
vk::BufferMemoryBarrier::default()
    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)  // ✅ Correct
    .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
```

## All Drop Violations

| File | Lines | Resource | Severity |
|------|-------|----------|----------|
| `vulkan/texture.rs` | 592-600 | Image, ImageView, Sampler | CRITICAL |
| `vulkan/vertexbuffer.rs` | 60-68 | Buffer | CRITICAL |
| `vulkan/skeleton_buffer.rs` | 99-106 | Buffer | CRITICAL |
| `vulkan/descriptor_set.rs` | 62-72 | DescriptorPool, Layout | CRITICAL |
| `vulkan/bindless_texture.rs` | 551-559 | DescriptorPool, Layout, Sampler | CRITICAL |
| `vulkan/material/builder.rs` | 346-354 | Pipeline, PipelineLayout | CRITICAL |
| `vulkan/material/compute_pipeline.rs` | 164-168 | Pipeline | CRITICAL |
| `vulkan/material/compiler.rs` | 563-565 | DescriptorPool, Layouts | CRITICAL |
| `particles/mod.rs` | 2521-2530 | ParticleSystem resources | CRITICAL |
| `particles/buffer.rs` | 1066-1069 | ParticleBuffer | CRITICAL |
| `particles/debug_readback.rs` | 508-512 | DebugReadback resources | CRITICAL |
| `render_graph/graph.rs` | 95-107 | TransientTexture | CRITICAL |
| `render_graph/descriptor_sets/compositing.rs` | 363+ | DescriptorSet | CRITICAL |

## Root Cause

**VulkanContext::drop()** correctly calls `device_wait_idle()`, but it's the **LAST** thing to drop. All other resources are destroyed via their Drop impls **before** VulkanContext::drop() runs.

**Drop Order**:
1. Texture → Drop → destroys sampler/image_view/image ❌
2. DescriptorSet → Drop → destroys pool/layout ❌
3. Pipeline → Drop → destroys pipeline/layout ❌
4. ... (all other resources)
5. VulkanContext → Drop → device_wait_idle() ✅ (too late!)

## Recommended Fixes

### Option 1: Explicit Cleanup Phase (RECOMMENDED)

Add explicit cleanup before renderer destruction:

```rust
impl VulkanRenderer {
    pub fn explicit_cleanup(&mut self) {
        // Wait for GPU to finish ALL work
        unsafe {
            self.context.device.device_wait_idle().unwrap();
        }
        
        // Now safe to destroy resources in controlled order
        self.particle_system.destroy();
        self.material_compiler.destroy();
        self.storage_uniforms.destroy();
        self.bindless_textures.destroy();
        self.frame_context.destroy();
        // ... etc
    }
}
```

**Call this before Drop runs**:
```rust
impl Drop for VulkanRenderer {
    fn drop(&mut self) {
        // Cleanup already done, just destroy context
        self.context.pre_destroy();
    }
}
```

### Option 2: Remove Drop Impls (More Work)

Remove Drop from all resource wrappers, require explicit destroy():

```rust
impl Texture {
    pub fn destroy(self, device: &Device) {
        unsafe {
            device.device_wait_idle().unwrap(); // Sync
            device.destroy_image_view(self.image_view, None);
            device.destroy_image(self.image, None);
            // ...
        }
    }
}
```

**Pros**: Explicit, safe
**Cons**: Massive refactoring, error-prone

### Option 3: Resource Retirement System (Best Long-Term)

Track resources per-frame and retire after N frames:

```rust
struct RetirementManager {
    retired_resources: VecDeque<(Box<dyn Destroyable>, u32)>, // (resource, frame_when_safe_to_destroy)
    current_frame: u32,
    frames_until_safe: u32,  // e.g., 4
}

impl RetirementManager {
    fn retire(&mut self, resource: Box<dyn Destroyable>) {
        self.retired_resources.push_back((resource, self.current_frame + self.frames_until_safe));
    }
    
    fn cleanup(&mut self, device: &Device) {
        while let Some((resource, safe_frame)) = self.retired_resources.front() {
            if self.current_frame >= *safe_frame {
                resource.destroy(device);
                self.retired_resources.pop_front();
            } else {
                break;
            }
        }
    }
}
```

**Pros**: Clean, efficient, no stalls
**Cons**: Complex to implement

## Immediate Fix Required

### Fix swap_alive_lists Barrier

**File**: `katla_gfx/src/particles/buffer.rs`

**Before** (line 923):
```rust
pub fn swap_alive_lists(&self, command_buffer: vk::CommandBuffer, frame_idx: usize) -> Result<(), String> {
    // Calculate offsets...
    
    // ❌ MISSING: Barrier before copy to ensure SIMULATE writes are visible
    
    // Copy alive_next to alive_list
    unsafe {
        device.cmd_copy_buffer(...);
    }
```

**After**:
```rust
pub fn swap_alive_lists(&self, command_buffer: vk::CommandBuffer, frame_idx: usize) -> Result<(), String> {
    let device = &self.context.device;
    
    // Calculate offsets...
    
    // ✅ ADD: Barrier before copy to ensure SIMULATE writes are visible to TRANSFER
    let pre_copy_barrier = vk::BufferMemoryBarrier::default()
        .src_access_mask(vk::AccessFlags::SHADER_WRITE)
        .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
        .src_stage_mask(vk::PipelineStageFlags::COMPUTE_SHADER)
        .dst_stage_mask(vk::PipelineStageFlags::TRANSFER)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .buffer(self.particle_buffer)
        .offset(alive_next_offset)
        .size(alive_list_size);
    
    unsafe {
        device.cmd_pipeline_barrier(
            command_buffer,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[],
            &[pre_copy_barrier],
            &[],
        );
    }
    
    // Now safe to copy
    unsafe {
        device.cmd_copy_buffer(...);
    }
```

**Also fix source barrier access mask** (line 975):
```rust
// Change from:
.src_access_mask(vk::AccessFlags::TRANSFER_READ)  // ❌

// To:
.src_access_mask(vk::AccessFlags::TRANSFER_WRITE)  // ✅
```

## Testing

After implementing fixes:

1. Run `cargo run -- -s -v` and verify NO validation errors
2. Run for 100+ frames to catch delayed errors
3. Test with multiple emitters
4. Test with maximum particle count
5. Test rapid emitter creation/destruction

## Related Documentation

- `docs/particle-sync-analysis.md` - Particle system synchronization
- `docs/particle-validation-fixes.md` - Previous validation fixes
- `docs/validation-error-analysis.md` - Descriptor lifecycle issues
- `descriptor_set_lifecycle_analysis.md` - Descriptor lifecycle deep dive
- `barrier_audit_report.md` - Complete barrier audit

## Conclusion

The Katla codebase has a **systemic issue** with resource lifetime management. The RAII pattern works for Rust memory but **violates Vulkan's lifetime rules**.

**Immediate action required**:
1. Fix swap_alive_lists missing barrier
2. Implement explicit cleanup phase or remove Drop impls
3. Test thoroughly with validation layers enabled

**Long-term**: Implement resource retirement system for optimal performance.
