# Giga Maintainer Improvement Plan - March 2026

## Overview

Review of commits `c67023c` through `dca651d` identified targeted improvements to simplify and clarify the API.

**Core Principle:** NO HYBRID IMPLEMENTATIONS - Remove and simplify, don't add options.

---

## Issues Identified

### 1. Hidden Magic Number ⚠️

**Location:** `katla_gfx/src/renderer.rs`

The storage buffer supports 256 objects per frame, but this limit is buried in the implementation. Users need to know this limit at compile time.

**Current:**
```rust
// Hidden in implementation
for draw_call in &draw_list.draws {
    let index = draw_call.instance_index as usize;
    // What happens if index >= 256? Panic? Silent corruption?
}
```

**Fix:**
```rust
impl VulkanRenderer {
    pub const MAX_OBJECTS_PER_FRAME: u32 = 256;
}
```

---

### 2. Duplicate Documentation 💀

**Location:** `katla_gfx/src/renderer.rs:execute_draw_calls()`

```rust
/// This method writes all per-object data from draw calls to the storage buffer.
/// Frame uniforms should be set separately via `set_frame_uniforms()`.
///
/// This method writes all per-object data from draw calls to the storage buffer.  // DUPLICATE
/// Frame uniforms should be set separately via `set_frame_uniforms()`.            // DUPLICATE
```

This is AI slop from iterative editing. Remove the duplication.

---

### 3. Tonemap Params Update in Wrong Place ⚠️

**Location:** `katla_gfx/src/render_graph/graph.rs:169-183`

```rust
// Update tonemap params for all tonemap passes BEFORE creating frame
for pass in &self.passes {
    if let Some(ref params) = pass.tonemap_params {
        if let Some(hdr_index) = params.hdr_texture_index {
            // Why is the render graph doing storage buffer updates?
            renderer.storage_manager.update_object_bindless(0, ...);
        }
    }
}
```

**Problem:** The render graph is reaching into the renderer's storage manager to update object bindless data. This couples the graph to storage buffer layout details.

**Decision:** The fullscreen pass execution callback (`FullscreenPassData`) should handle its own storage buffer setup. Remove the pre-update loop from `FrameGraph::execute()`.

---

### 4. ImageFormat::Auto Documentation Clarity

**Location:** `katla_gfx/src/render_graph/pass.rs:44-45`

The `output_format` field is set to the first color attachment's format. This works but could be clearer.

**Current:**
```rust
// In GeometryPass::as_builder()
let output_format = color_outputs.first().map(|o| o.format);
```

**Issue:** What if there are multiple color attachments? Which format is used?

**Answer:** First color attachment. This is reasonable (MRT with mixed formats is exotic), but should be documented.

---

### 5. Graph Complexity Growth

**Location:** `katla_gfx/src/render_graph/graph.rs`

The file grew by ~600 lines. Key additions:
- `TransientTexture` struct with Drop impl
- `resolve_materials()` method
- `initialize_transient_textures()` method
- Tonemap params update loop

**Assessment:** The additions are mostly necessary for the new functionality. However:
- Transient texture management could be a separate module (if file continues to grow)
- Tonemap params update should move to pass execution (see #3)

---

## Action Plan

### Priority 1: Quick Wins (12 min)

1. **Expose MAX_OBJECTS_PER_FRAME** (5 min)
   - Add `pub const MAX_OBJECTS_PER_FRAME: u32 = 256;` to `VulkanRenderer`
   - Add bounds checking in `execute_draw_calls` with clear panic message

2. **Fix Duplicate Documentation** (2 min)
   - Remove duplicate lines in `execute_draw_calls` doc comment

3. **Document Format Inference** (5 min)
   - Add doc comment explaining that `output_format` uses first color attachment
   - Note that MRT with mixed formats is not supported for Auto materials

### Priority 2: Architectural Cleanup (30 min)

4. **Move Tonemap Update to Pass Execution** (30 min)
   - Remove pre-update loop from `FrameGraph::execute()`
   - Add tonemap params update to fullscreen pass execution callback
   - This decouples the graph from storage buffer layout

   **Before:**
   ```rust
   // In FrameGraph::execute()
   for pass in &self.passes {
       if let Some(ref params) = pass.tonemap_params {
           renderer.storage_manager.update_object_bindless(0, ...);
       }
   }
   ```

   **After:**
   ```rust
   // In fullscreen pass execution callback
   if let Some(ref params) = self.tonemap_params {
       renderer.storage_manager.update_object_bindless(0, ...);
   }
   ```

---

## Deferred (Not Addressed Now)

### Per-Frame Two-Step API

The `set_frame_uniforms()` + `execute_draw_calls()` pattern was questioned in the review.

**Verdict:** Keep as-is. This separation is correct:
- Frame uniforms set once per frame (camera, lighting)
- Draw calls executed multiple times (viewport splitting, shadow passes)

Adding a combined method would create a hybrid API. The two-step pattern is explicit and Vulkan-idiomatic.

### ImageFormat::Auto

The deferred material compilation was questioned.

**Verdict:** Keep. The implementation is clean and the API is useful. Only needs better documentation.

---

## Summary

| Issue | Type | Effort | Impact |
|-------|------|--------|--------|
| MAX_OBJECTS_PER_FRAME const | Add | 5 min | High |
| Duplicate docs | Remove | 2 min | Low |
| Tonemap update location | Move | 30 min | Medium |
| Format inference docs | Add | 5 min | Medium |
| Graph complexity | Monitor | - | - |

**Total Effort:** ~45 minutes for Priority 1 + 2

**No New APIs:** This plan only exposes hidden information and moves code to the right place. No new public methods are added.
