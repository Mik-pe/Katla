# Vulkan 1.3 Migration Plan - Removal of Legacy Patterns

**Date:** 2026-02-12
**Status:** Ready for Execution

## Executive Summary

The Katla engine has **partially migrated** to modern Vulkan 1.3 patterns. Dynamic Rendering is used in production, but significant legacy code remains from the Vulkan 1.0 era.

### Key Findings

| Component | Status | Recommendation |
|------------|----------|---------------|
| **Dynamic Rendering** | ✅ Implemented & used | Production ready |
| **RenderPass struct** | ❌ Tests only | **REMOVE** |
| **Synchronization2** | ✅ Implemented & used | Production ready |
| **Legacy barriers** | ⚠️ Mixed (legacy in CommandBuffer, texture.rs) | **REMOVE** |
| **Material builders** | ⚠️ Carry legacy render_pass param | **SIMPLIFY** |
| **Render graph** | ⚠️ Has legacy render pass creation path | **CLEANUP** |
| **BDA/Bindless** | ⚠️ Infrastructure exists, not used | Future enhancement |

---

## Phase 1: Remove Test-Only Legacy Code

### 1.1 Remove `RenderPass` struct and test file

**Files to delete:**
- `katla_vulkan/src/vulkan/renderpass.rs` (entire file)
- `katla_vulkan/tests/validation_negative.rs` (entire test file)

**Justification:**
- `RenderPass::create_opaque()` and `create_from_config()` are **only used in validation tests**
- Production code uses `vk::RenderPass::null()` for Dynamic Rendering
- Tests exist to verify validation callback system - can be replaced with simpler tests

**Actions:**
```bash
rm katla_vulkan/src/vulkan/renderpass.rs
rm katla_vulkan/tests/validation_negative.rs
```

**Files to modify:**
- `katla_vulkan/src/vulkan/mod.rs` - Remove `pub use renderpass::*;`
- `katla_vulkan/src/lib.rs` - Already uses `pub use vulkan::*` so nothing needed

**Risk:** Low - test-only code with no production usage

---

## Phase 2: Simplify Material System

### 2.1 Remove `render_pass` parameter from material builders

**Current pattern (legacy):**
```rust
pub fn build(self, render_pass: Option<&RenderPass>) -> Result<MaterialPipeline>
pub fn build_with_desc_layout(self, render_pass: Option<&RenderPass>, ...) -> ...
```

**The `render_pass` parameter is misleading:**
- When `None`: Uses Dynamic Rendering (modern) ✅
- When `Some(rp)`: Converts to `vk::RenderPass` for pipeline creation
- **In practice: Production code ALWAYS passes `None`**

**Files to modify:**

#### `katla_vulkan/src/vulkan/material/materialbuilder.rs`

**Before:**
```rust
pub fn build(self, render_pass: Option<&RenderPass>) -> Result<MaterialPipeline>
pub fn build_with_desc_layout(self, render_pass: Option<&RenderPass>, existing_desc_layout: vk::DescriptorSetLayout)
```

**After:**
```rust
pub fn build(self) -> Result<MaterialPipeline>  // No render_pass param
pub fn build_with_desc_layout(self, existing_desc_layout: vk::DescriptorSetLayout)

// Internal changes:
let vk_render_pass = vk::RenderPass::null();  // Always use dynamic rendering
let color_format = self.color_format;  // Always use builder's formats
let depth_format = self.depth_format;
```

#### `katla_vulkan/src/vulkan/material/registry.rs`

**Update call sites:**
- Line 103: `builder.build(Some(render_pass))` → `builder.build()`
- Line 132: `builder.build(Some(render_pass))` → `builder.build()`
- Line 190: `builder.build(render_pass)` → `builder.build()`
- Line 262: `builder.build_with_desc_layout(render_pass, desc_layout)` → `builder.build_with_desc_layout(desc_layout)`
- Line 328: `builder.build_with_desc_layout(render_pass, desc_layout)` → `builder.build_with_desc_layout(desc_layout)`

#### `katla_vulkan/src/vulkan/material/template.rs`

**Update:**
- Line 377: `build(render_pass: Option<&RenderPass>)` → `build()`
- Line 398: Internal call to `build()` - update to match new signature

### 2.2 Remove RenderPass import from material modules

**Files:**
- `katla_vulkan/src/vulkan/material/hot_reload.rs` - Remove `use crate::RenderPass;`
- `katla_vulkan/src/vulkan/material/materialbuilder.rs` - Remove `use crate::RenderPass;`
- `katla_vulkan/src/vulkan/material/registry.rs` - Remove `use crate::RenderPass;`
- `katla_vulkan/src/vulkan/material/template.rs` - Remove `use crate::RenderPass;`

---

## Phase 3: Clean Up Render Graph Legacy Code

### 3.1 Remove legacy render pass generation in `compiled.rs`

**Current behavior:**
- `generate_render_passes()` creates traditional render passes
- These are then stored in `vk_render_passes: Vec<vk::RenderPass>`
- But modern rendering uses null render passes!

**File: `katla_vulkan/src/render_graph/compiled.rs`**

**Option A: Remove render pass generation entirely**
- Delete `generate_render_passes()` function
- Delete `vk_render_passes` field
- All passes use `vk::RenderPass::null()`

**Option B: Stub with null passes**
- Simplify `generate_render_passes()` to return `vec![vk::RenderPass::null()]`
- Minimal changes

**Recommendation: Option B** (safer, keeps code structure)

### 3.2 Fix ash type leakage

**File: `katla_vulkan/src/render_graph/compiled.rs`**

**Line 26:**
```rust
// BEFORE:
vk_render_passes: Vec<vk::RenderPass>,

// AFTER:
vk_render_passes: Vec<VkRenderPass>,
```

**Line 315, 619, 701:**
- Update function signatures to use `VkRenderPass` instead of `vk::RenderPass`

**Line 743, 750:**
- Add wrapper: `VkRenderPass::new(vk_render_pass_raw)`

---

## Phase 4: Remove Legacy Pipeline Barriers

### 4.1 Remove legacy `pipeline_barrier()` from CommandBuffer

**File: `katla_vulkan/src/vulkan/commandbuffer.rs`**

**Delete lines 200-220** (legacy `pipeline_barrier()` method)

**Justification:**
- Modern `pipeline_barrier2()` (lines 257-262) provides superior API
- Render graph already uses `pipeline_barrier2`
- Only remaining usage is in `texture.rs` (one location)

### 4.2 Update remaining legacy barrier usage

**File: `katla_vulkan/src/vulkan/texture.rs:83`

**Before:**
```rust
context.device.cmd_pipeline_barrier(
    command_buffer,
    src_stage_mask,
    dst_stage_mask,
    dependency_flags,
    &[],
    &[],
    &[image_memory_barrier],
);
```

**After:**
```rust
use crate::sync::{ImageMemoryBarrier2, PipelineStage2Flags, AccessFlags2, DependencyInfo};

let barrier = ImageMemoryBarrier2::new(image)
    .src_stage(src_stage_mask.into())
    .dst_stage(dst_stage_mask.into())
    .src_access(src_access_mask.into())
    .dst_access(dst_access_mask.into())
    .old_layout(old_layout)
    .new_layout(new_layout)
    .subresource_range(subresource_range);

let dep_info = DependencyInfo::new().add_image_barrier(barrier);
command_buffer.pipeline_barrier2(dep_info);
```

---

## Phase 5: Remove Legacy Render Pass Commands

### 5.1 Deprecate traditional render pass methods in CommandBuffer

**File: `katla_vulkan/src/vulkan/commandbuffer.rs`**

**Add deprecation notices:**
```rust
#[deprecated(since = "0.1.0", note = "Use begin_rendering() for Dynamic Rendering (Vulkan 1.3)")]
pub fn begin_render_pass(...)

#[deprecated(since = "0.1.0", note = "Use end_rendering() for Dynamic Rendering (Vulkan 1.3)")]
pub fn end_render_pass(&self)
```

**Keep for now** - they may be used by external code, but warn users to migrate

---

## Phase 6: Public API Cleanup

### 6.1 Verify public API after changes

**Check:**
```bash
# Ensure no RenderPass in public API
grep -r "pub.*RenderPass" katla_vulkan/src/

# Verify no raw ash::vk types in public API
grep -r "pub.*vk::" katla_vulkan/src/lib.rs
```

**Expected clean public exports:**
```rust
// Keep these:
pub use sync::{VkRenderPass, ...};  // Wrapper type

// Remove this:
pub use vulkan::*;  // This includes RenderPass!
```

**Fix:**
- Change `pub use renderpass::*;` in `vulkan/mod.rs` to NOT export RenderPass
- Or don't delete `renderpass.rs` file, just make it `pub(crate)`

---

## Phase 7: Future Enhancements (Post-Migration)

### 7.1 Buffer Device Address (BDA) for uniform buffers

**Current state:** Infrastructure exists in `katla_vulkan/src/vulkan/bda.rs`

**Migration path:**
1. Replace descriptor-based uniforms with push-constant buffer addresses
2. Update shaders to accept `DeviceAddress` pointer
3. Eliminate per-frame descriptor updates

### 7.2 Bindless textures

**Current state:** Single descriptor set per texture

**Migration path:**
1. Create single texture array descriptor set
2. Update shaders to index by material ID
3. Use `NonUniformResourceIndex` in shaders

---

## Implementation Order

| Phase | Effort | Risk | Value |
|--------|----------|-------|-------|
| 1. Remove test-only code | Low | Low - cleanup only |
| 2. Simplify material system | Medium | Low - API change, but all callers internal |
| 3. Clean render graph | Low | Low - minimal logic change |
| 4. Remove legacy barriers | Low | Low - one usage site |
| 5. Deprecate render pass commands | Low | Low - deprecation only |
| 6. Public API cleanup | Low | Low - verification |
| 7. BDA/Bindless (future) | High | High - major feature |

**Recommended implementation sequence:** 1 → 2 → 3 → 4 → 5 → 6

---

## Summary of Changes

### Files to DELETE (2):
1. `katla_vulkan/src/vulkan/renderpass.rs`
2. `katla_vulkan/tests/validation_negative.rs`

### Files to MODIFY (8):
1. `katla_vulkan/src/vulkan/mod.rs` - Remove renderpass export
2. `katla_vulkan/src/vulkan/commandbuffer.rs` - Remove legacy barrier, deprecate render pass methods
3. `katla_vulkan/src/vulkan/texture.rs` - Convert to barrier2
4. `katla_vulkan/src/vulkan/material/materialbuilder.rs` - Remove render_pass param
5. `katla_vulkan/src/vulkan/material/registry.rs` - Update builder calls
6. `katla_vulkan/src/vulkan/material/template.rs` - Remove render_pass param
7. `katla_vulkan/src/vulkan/material/hot_reload.rs` - Remove RenderPass import
8. `katla_vulkan/src/render_graph/compiled.rs` - Use VkRenderPass wrapper, simplify render pass generation

### Public API Impact:
- ✅ **Removes:** `RenderPass` legacy struct
- ✅ **Simplifies:** Material builder API (fewer parameters)
- ✅ **Deprecates:** Traditional render pass commands
- ✅ **Modernizes:** All barrier usage to Synchronization2
