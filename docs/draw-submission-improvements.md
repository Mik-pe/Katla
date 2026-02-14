# Draw Submission System Improvements

**Status:** Proposed (Updated 2026-02-14)
**Created:** 2025-02-14
**Priority:** High - Performance & UX Impact

## Overview

The current draw submission system works but has several performance and usability issues. This document outlines the problems and proposes solutions for a more efficient, user-friendly API.

---

## Current Architecture

### App Layer (katla_app)
```rust
// Builds DrawList each frame from ECS queries
let mut draw_list = DrawList::new();
for (entity, transform, drawable) in world.query::<...>() {
    let draw_call = DrawCall::new(mesh, material)
        .with_all_matrices(model, view, proj, inv_vp);
    draw_list.push(draw_call);
}
renderer.render_frame(draw_list);
```

### Renderer Layer (katla_vulkan)
```rust
// Processes DrawList in render graph pass
for draw in &draw_list.draws {
    // 1. Get mesh/material data
    // 2. Update storage buffer with uniforms
    // 3. Bind pipeline & descriptors
    // 4. Bind vertex/index buffers
    // 5. draw_indexed() or draw_array()
}
```

### Storage Buffer Architecture (Already Implemented!)
```
┌─────────────────────────────────────────────────────────────┐
│ Storage Uniform Buffer (~24KB, persistent mapping)          │
├─ [Frame Uniforms: 256 bytes] ← Updated ONCE per frame       │
│  ├─ view: mat4x4 (64 bytes)                                │
│  ├─ proj: mat4x4 (64 bytes)                                │
│  ├─ inv_view_proj: mat4x4 (64 bytes)                       │
│  ├─ camera_position: vec4 (16 bytes)                       │
│  ├─ light_direction: vec4 (16 bytes)                       │
│  ├─ light_color: vec4 (16 bytes)                           │
│  └─ light_intensity: vec4 (16 bytes)                       │
├─ [Object Array: 96 bytes × 256 = 24,576 bytes]             │
│    ├─ Object[0]: model (64) + color (16) + material (16)   │
│    ├─ Object[1]: model (64) + color (16) + material (16)   │
│    └─ ...                                                   │
└─────────────────────────────────────────────────────────────┘
```

**Key Insight:** The storage buffer architecture already separates frame vs per-object uniforms! The `StorageUniformManager.update_frame()` handles view/proj once, and `update_object_with_material()` only takes model matrix.

---

## Identified Issues

### Issue 1: DrawCall API Still Carries Redundant Matrices (MEDIUM) ⚠️

**Problem:** The `DrawCall` struct still has `MaterialParams` with 4 matrices, even though the storage buffer renderer only uses `model_matrix`.

**Current State:**
```rust
pub struct DrawCall {
    pub params: MaterialParams {
        pub model_matrix: [f32; 16],      // ✅ USED
        pub view_matrix: [f32; 16],       // ❌ IGNORED in storage mode
        pub proj_matrix: [f32; 16],       // ❌ IGNORED in storage mode
        pub inv_view_proj_matrix: [f32; 16], // ❌ IGNORED in storage mode
        pub color: Option<[f32; 4]>,      // ✅ USED
        pub metallic: f32,                // ✅ USED
        pub roughness: f32,               // ✅ USED
        pub ao: f32,                      // ✅ USED
    }
}
```

**Impact:**
- Each DrawCall is ~280 bytes (could be ~100 bytes)
- 1000 draws = 280 KB allocated but 100 KB actually needed
- Confusing API - users set view/proj but they're ignored

**Solution:** Create streamlined `DrawCall` for storage mode:

```rust
/// Per-draw data for storage buffer rendering
pub struct DrawCall {
    pub mesh: MeshHandle,
    pub material: MaterialHandle,
    pub model_matrix: [f32; 16],
    pub color: Option<[f32; 4]>,
    pub metallic: f32,
    pub roughness: f32,
    pub ao: f32,
    pub sort_key: Option<u64>,
    pub skeleton: Option<SkeletonHandle>,
    // REMOVED: view_matrix, proj_matrix, inv_view_proj_matrix, object_index
}
```

**Files to modify:**
- `katla_vulkan/src/rendering/types.rs` - Simplify `DrawCall`, remove `MaterialParams` redundancy
- `katla_app/src/application/mod.rs` - Remove `with_all_matrices()` usage
- Update tests

---

### Issue 2: Unsafe transmute_copy in App Layer (MEDIUM) ⚠️

**Problem:** App layer uses unsafe transmute to convert matrices:

```rust
let model_array: [f32; 16] = unsafe { std::mem::transmute_copy(&model_matrix) };
```

**Solution:** Add safe conversion methods to math types

```rust
// In katla_math/src/mat4.rs
impl Mat4 {
    pub fn to_array(&self) -> [f32; 16] {
        // Safe conversion - Mat4 is already [f32; 16] internally
        self.clone().into()
    }
}
```

**Note:** katla_math already has `impl From<Mat4> for [[f32; 4]; 4]`, we just need a convenience method.

**Files to modify:**
- `katla_math/src/mat4.rs` - Add `to_array()` convenience method
- `katla_app/src/application/mod.rs` - Use safe conversions

---

### Issue 3: No Instancing Support (HIGH) 📦

**Problem:** Drawing 1000 identical objects = 1000 draw calls + 1000 storage buffer updates.

**Impact:**
- CPU overhead from draw call API calls
- Can't efficiently render foliage/particles/instances

**Solution:** Add GPU instancing support

```rust
pub struct DrawCall {
    pub mesh: MeshHandle,
    pub material: MaterialHandle,

    /// Instance data for this draw
    /// Multiple instances = single draw call with instance_count
    pub instances: SmallVec<[InstanceData; 1]>,
}

pub struct InstanceData {
    pub model_matrix: [f32; 16],
    pub color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub ao: f32,
}

impl DrawCall {
    /// Single instance (most common case)
    pub fn single(mesh: MeshHandle, material: MaterialHandle) -> Self { ... }

    /// Multiple instances
    pub fn instanced(mesh: MeshHandle, material: MaterialHandle, instances: Vec<InstanceData>) -> Self { ... }
}
```

**Rendering changes:**
- Write all instances to storage buffer at consecutive indices
- Use `draw_indexed_instanced()` with `first_instance` pointing to first object index
- Shader uses `@builtin(instance_index)` to index into objects array

**Files to modify:**
- `katla_vulkan/src/rendering/types.rs` - Add instancing types
- `katla_vulkan/src/lib.rs` - Update draw logic for instancing
- Shaders already support instance_index!

---

### Issue 4: Confusing object_index Field (LOW) 🔄

**Problem:** The `object_index` field is auto-assigned anyway:

```rust
let object_index = draw.object_index.unwrap_or_else(|| {
    let idx = next_object_index;
    next_object_index += 1;  // Auto-assigns anyway
    idx
});
```

**Solution:** Remove the field entirely. It's always auto-assigned in practice.

**Files to modify:**
- `katla_vulkan/src/rendering/types.rs` - Remove `object_index` field

---

### Issue 5: No Frustum Culling (MEDIUM) 🎯

**Problem:** All objects submitted, even off-screen ones.

**Solution:** Add culling at the ECS level, not DrawList level:

```rust
// In application, before building draw list
for (entity, transform, drawable, bounds) in world.query::<(..., &BoundingVolume)>() {
    if !frustum.intersects(&bounds) {
        continue; // Skip culled objects
    }
    // Add to draw list
}
```

**Better:** Add a `CullingSystem` that sets a `Visible` marker component:

```rust
pub struct CullingSystem {
    frustum: Frustum,
}

impl System for CullingSystem {
    fn update(&mut self, world: &mut World, _dt: f32) {
        for (entity, transform, bounds) in world.query::<(&TransformComponent, &BoundingVolume)>() {
            let visible = self.frustum.intersects(&bounds.world_bounds(&transform));
            // Set or remove Visible marker component
            if visible {
                world.add_component(entity, Visible);
            } else {
                world.remove_component::<Visible>(entity);
            }
        }
    }
}

// In render loop, query only visible entities
for (entity, transform, drawable, _visible) in world.query::<(..., &Visible)>() {
    // Build draw list
}
```

**Files to create/modify:**
- `katla_app/src/systems/culling_system.rs` - New system
- `katla_app/src/components/bounding_volume.rs` - Bounding volume component
- `katla_math/src/frustum.rs` - Frustum type

---

### Issue 6: No Sort Key Strategy (NEW) 🔀

**Problem:** The plan mentions sort keys but doesn't define a strategy.

**Solution:** Implement a multi-layer sort key:

```rust
/// Sort key encoding for efficient state sorting
/// Bits: [reserved:8][depth:24][material:16][mesh:16]
pub fn compute_sort_key(
    material: MaterialHandle,
    mesh: MeshHandle,
    depth: f32,  // Distance from camera
    transparent: bool,
) -> u64 {
    if transparent {
        // Back-to-front for transparency
        let depth_bits = (depth * 16777215.0) as u32; // 24-bit depth
        ((depth_bits as u64) << 32) | ((material.0 as u64) << 16) | (mesh.0 as u64)
    } else {
        // Front-to-back for early-z, grouped by material
        let depth_bits = (depth * 16777215.0) as u32;
        ((material.0 as u64) << 40) | ((mesh.0 as u64) << 24) | depth_bits as u64
    }
}
```

**Benefits:**
- Opaque objects: Material grouping reduces state changes
- Transparent objects: Correct back-to-front rendering
- Both: Early-z rejection saves fill rate

---

## Implementation Plan

### Phase 1: Simplify DrawCall API (Issue 1 & 4)

**Goal:** Remove redundant matrices, clean up API

**Steps:**
1. Create new streamlined `DrawCall` struct
2. Remove `MaterialParams` wrapper (flatten into DrawCall)
3. Remove `object_index` field
4. Update renderer to use new structure
5. Update app layer to use simplified API

**Estimated effort:** 3-4 hours
**Risk:** Medium - API breaking changes

---

### Phase 2: Safe Matrix Conversions (Issue 2)

**Goal:** Remove unsafe transmute from app layer

**Steps:**
1. Add `Mat4::to_array()` convenience method
2. Update app layer to use safe conversions

**Estimated effort:** 1 hour
**Risk:** Low - additive changes

---

### Phase 3: Instancing Support (Issue 3)

**Goal:** Support efficient instanced rendering

**Steps:**
1. Design `InstanceData` struct
2. Update `DrawCall` to support multiple instances
3. Update renderer to:
   - Write consecutive instances to storage buffer
   - Use `draw_indexed_instanced()` with `first_instance`
4. Add instancing example

**Estimated effort:** 6-8 hours
**Risk:** Medium - requires testing

---

### Phase 4: Frustum Culling (Issue 5)

**Goal:** Skip off-screen objects

**Steps:**
1. Add `BoundingVolume` component (AABB or sphere)
2. Add `Visible` marker component
3. Implement `CullingSystem`
4. Add `Frustum` extraction from camera
5. Update render loop to query only visible entities

**Estimated effort:** 4-6 hours
**Risk:** Low - additive feature

---

### Phase 5: Sort Key Strategy (Issue 6)

**Goal:** Efficient draw ordering

**Steps:**
1. Implement `compute_sort_key()` function
2. Add `transparent` flag to materials
3. Sort `DrawList` by sort key before submission
4. Test with transparent objects

**Estimated effort:** 2-3 hours
**Risk:** Low - additive feature

---

## Proposed Final API

```rust
// === Frame Setup (once per frame) ===
renderer.set_frame_uniforms(FrameUniforms {
    view: camera.view_matrix(),
    proj: camera.projection_matrix(),
    inv_view_proj: camera.inverse_view_projection(),
    camera_position: camera.position(),
    lighting: &scene.lighting,
});

// === Draw List Building ===
let mut draw_list = DrawList::new();

for (entity, transform, drawable, _visible) in world.query::<(..., &Visible)>() {
    // Single instance
    draw_list.push(DrawCall::single(drawable.mesh, drawable.material)
        .with_transform(transform.matrix())
        .with_color(drawable.color)
        .with_pbr(drawable.metallic, drawable.roughness, drawable.ao)
        .with_skeleton(drawable.skeleton_handle)
        .with_sort_key(compute_sort_key(
            drawable.material,
            drawable.mesh,
            transform.distance_to_camera,
            drawable.transparent,
        )));

    // OR: Multiple instances (for foliage, particles, etc.)
    draw_list.push(DrawCall::instanced(mesh, material, instances
        .iter()
        .map(|i| InstanceData {
            model_matrix: i.transform.matrix(),
            color: i.color,
            metallic: 0.0,
            roughness: 0.5,
            ao: 1.0,
        })
        .collect()));
}

// Sort for optimal rendering
draw_list.sort_by_sort_key();

// Submit
renderer.render_frame(draw_list);
```

---

## Breaking Changes Summary

### DrawCall Changes
| Before | After |
|--------|-------|
| `DrawCall::new(mesh, mat)` | `DrawCall::single(mesh, mat)` |
| `.with_all_matrices(model, view, proj, inv_vp)` | `.with_transform(model)` |
| `params.view_matrix` | REMOVED (use frame uniforms) |
| `params.proj_matrix` | REMOVED (use frame uniforms) |
| `object_index` | REMOVED (auto-assigned) |

### New Types
- `FrameUniforms` - Passed to `set_frame_uniforms()`
- `InstanceData` - Per-instance transform data
- `Visible` - Marker component for culled objects

### Migration Path
1. Bump version
2. Provide `DrawCall::legacy()` constructor for backward compat
3. Deprecate old methods with warnings

---

## Performance Estimates

### Current State (1000 objects)
```
DrawCall size: ~280 bytes
Total for 1000 draws: ~280 KB
Frame uniforms: N/A (in each DrawCall)
```

### After Phase 1 (Simplified DrawCall)
```
DrawCall size: ~100 bytes
Total for 1000 draws: ~100 KB
Memory reduction: 64%
```

### After Phase 3 (Instancing, 1000 instances of 10 meshes)
```
With instancing: 10 DrawCalls instead of 1000
Each with ~100 instances
Draw call reduction: 99%
```

---

## Open Questions

1. **~~Storage Buffer Alignment~~** ✅ Already handled by `StorageUniformLayout`
2. **Multi-threading:** Could parallelize draw list building from ECS queries
3. **Bindless Textures:** Future work - would eliminate texture descriptor binding
4. **Push Constants:** Frame uniforms could use push constants (faster for small data), but storage buffer works well
5. **GPU-driven Culling:** Future work - compute shader culling for very large scenes

---

## References

- Current DrawCall: `katla_vulkan/src/rendering/types.rs`
- Storage uniforms: `katla_vulkan/src/vulkan/material/storage_uniform.rs`
- Usage: `katla_app/src/application/mod.rs`
- Render loop: `katla_vulkan/src/lib.rs` (lines 700-850)
