# Giga Maintainer Plan: Per-Frame Data API Design
**Date**: 2025-03-05
**Status**: ✅ ALL PHASES COMPLETE!

All 5 phases successfully implemented. The per-frame data API is fully functional with:
- Automatic instance allocation via FrameContext
- Storage buffer based per-object data (no push constants)
- Explicit skeleton binding for skinned meshes
- Comprehensive documentation

## Progress Tracking

- ✅ **Phase 1**: Remove Dead Code (COMPLETE - 2025-03-05)
- ✅ **Phase 2**: Expose Instance Allocation (COMPLETE - 2025-03-05)
- ✅ **Phase 3**: Update Storage Manager (COMPLETE - 2025-03-05)
- ✅ **Phase 4**: Skeleton API Cleanup (COMPLETE - 2025-03-05)
- ✅ **Phase 5**: Documentation (COMPLETE - 2025-03-05)

## Problem Statement

The current material system uses push constants for `object_index` and `material_index`, but:
1. WGSL/WebGPU does NOT support push constants
2. We need a clean API for the app to supply per-frame data (transforms, skeletal animation, etc.)
3. Implementation details (descriptor sets) should be hidden from the app layer
4. No hybrid systems - single approach across the board

---

## Current State Analysis

### What We Already Have (and it's good!)

Looking at the actual WGSL shaders, we're **already using the correct pattern**:

```wgsl
// Set 0: Frame data (binding 0) + Object array (binding 1)
@group(0) @binding(0) var<storage, read> frame_data: FrameUniforms;
@group(0) @binding(1) var<storage, read> objects: array<ObjectUniforms>;

// Set 1: Bindless textures
@group(1) @binding(0) var bindless_textures: binding_array<texture_2d<f32>, 4096>;
@group(1) @binding(1) var shared_sampler: sampler;

// Set 2: Skeleton joint matrices (for skinned meshes)
@group(2) @binding(0) var<storage, read> joint_matrices: array<mat4x4f>;
```

**The shaders use `instance_index` to access per-object data:**
```wgsl
@vertex
fn vs_main(in: VertexInput, @builtin(instance_index) instance_idx: u32) -> VertexOutput {
    let obj = objects[instance_idx];  // ← Instance indexing, no push constants!
    // ...
}
```

### What's Wrong (the leftover cruft)

In `material/compiler.rs`, we're still creating push constant ranges:
```rust
.add_push_constant_range(
    vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
    0,
    8, // u32 object_index + u32 material_index
)
```

This is:
1. **Unused** by WGSL shaders (they use instance_index instead)
2. **Incompatible** with WebGPU/WGSL
3. **Dead code** that should be removed

---

## Industry Best Practices (from research)

### WebGPU/WGSL Patterns

**Descriptor Set Layout:**
- **Set 0**: Per-frame data + per-object array (storage buffers, instance-indexed)
- **Set 1**: Global resources (bindless textures, samplers)
- **Set 2**: Optional features (skeletal animation, compute shader data)

**Why Storage Buffers for Per-Object Data?**
- Uniform buffers: 64KB limit, too small for many objects
- Storage buffers: 128MB+ limit, can handle thousands of objects
- Instance indexing: Each invocation gets different data via `instance_index`

### Skeletal Animation Pattern

```wgsl
// Joint matrices in storage buffer (Set 2)
@group(2) @binding(0) var<storage, read> joint_matrices: array<mat4x4f>;

// Vertex shader blends matrices by weights
fn compute_skin_matrix(joint_indices: vec4u, joint_weights: vec4f) -> mat4x4f {
    let m0 = joint_matrices[joint_indices[0]] * joint_weights[0];
    let m1 = joint_matrices[joint_indices[1]] * joint_weights[1];
    let m2 = joint_matrices[joint_indices[2]] * joint_weights[2];
    let m3 = joint_matrices[joint_indices[3]] * joint_weights[3];
    return m0 + m1 + m2 + m3;
}
```

---

## Giga Assessment

### GFX Perspective (Graphics Engineer)

**What's Good:**
- The current storage buffer + instance indexing pattern is Vulkan/WebGPU native
- Bindless textures Set 1 design is clean and scalable
- Skeleton data in separate descriptor set (Set 2) allows conditional binding

**What Needs Fixing:**
1. Remove dead push constant ranges from pipeline creation
2. The API doesn't clearly expose the instance indexing pattern to app developers
3. Frame/Object uniform update flow could be more explicit

### APP Perspective (Game Developer)

**What's Good:**
- `StorageUniformManager` has convenient methods: `update_frame()`, `update_object()`
- InstanceData struct for GPU instancing is clean
- DrawCall builder pattern is discoverable

**What's Painful:**
1. The relationship between `instance_index` and array position isn't obvious
2. No clear documentation on "when to use instancing vs single draws"
3. Skeleton binding is opaque (how do I know which Set 2 to use?)

---

## Giga Recommendation: The Unified Per-Frame API

### Core Principle: Explicit Instance Allocation

**Problem**: Current code uses `instance_index` but doesn't explicitly allocate/track instances.

**Solution**: Make instance allocation part of the draw submission API.

### API Design

```rust
// ===== APP LAYER (katla_app) =====

/// Per-frame context for submitting draws
pub struct FrameContext {
    // Private: instance allocation tracking
    next_instance_index: u32,
    // ... renderer reference, etc.
}

impl FrameContext {
    /// Submit a single draw (allocates 1 instance slot)
    pub fn draw(&mut self, mesh: MeshHandle, material: MaterialHandle) -> DrawBuilder {
        let instance_idx = self.next_instance_index;
        self.next_instance_index += 1;
        DrawBuilder::new(instance_idx, mesh, material)
    }

    /// Submit instanced draw (allocates N instance slots)
    pub fn draw_instanced(
        &mut self,
        mesh: MeshHandle,
        material: MaterialHandle,
        instances: Vec<InstanceData>,
    ) -> DrawBuilder {
        let start_idx = self.next_instance_index;
        let count = instances.len() as u32;
        self.next_instance_index += count;
        DrawBuilder::new_instanced(start_idx, mesh, material, instances)
    }

    /// Submit skinned mesh draw (binds skeleton Set 2)
    pub fn draw_skinned(
        &mut self,
        mesh: MeshHandle,
        material: MaterialHandle,
        skeleton: SkeletonHandle,
    ) -> SkinnedDrawBuilder {
        let instance_idx = self.next_instance_index;
        self.next_instance_index += 1;
        SkinnedDrawBuilder::new(instance_idx, mesh, material, skeleton)
    }
}

/// Fluent builder for single draws
pub struct DrawBuilder {
    instance_index: u32,
    mesh: MeshHandle,
    material: MaterialHandle,
    transform: Option<[f32; 16]>,
    color: Option<[f32; 4]>,
    // ... pbr params
}

impl DrawBuilder {
    /// Set the transform for this instance
    pub fn with_transform(mut self, matrix: [f32; 16]) -> Self {
        self.transform = Some(matrix);
        self
    }

    /// Set material color
    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = Some(color);
        self
    }

    /// Submit to internal draw list (called by FrameContext)
    pub(crate) fn build(self, list: &mut DrawList) {
        // Write to storage buffer at self.instance_index
        storage_manager.update_object(
            self.instance_index as usize,
            &self.transform.unwrap_or_identity(),
            &self.color.unwrap_or_white(),
            // ... pbr params
        );

        list.push(DrawCall {
            mesh: self.mesh,
            material: self.material,
            instance_index: self.instance_index,
            // ...
        });
    }
}

/// Fluent builder for skinned mesh draws
pub struct SkinnedDrawBuilder {
    instance_index: u32,
    mesh: MeshHandle,
    material: MaterialHandle,
    skeleton: SkeletonHandle,
    transform: Option<[f32; 16]>,
    // ...
}

impl SkinnedDrawBuilder {
    pub fn with_transform(mut self, matrix: [f32; 16]) -> Self {
        self.transform = Some(matrix);
        self
    }

    /// Submit - this binds Set 2 with the skeleton descriptor set
    pub(crate) fn build(self, list: &mut DrawList) {
        // Write to storage buffer
        storage_manager.update_object(/* ... */);

        list.push(DrawCall {
            mesh: self.mesh,
            material: self.material,
            skeleton: Some(self.skeleton),
            instance_index: self.instance_index,
        });
    }
}

// ===== USAGE EXAMPLE =====

fn render_scene(renderer: &mut VulkanRenderer, frame: &mut FrameContext) {
    // Set frame-level uniforms once
    renderer.set_frame_uniforms(&camera.view_matrix, &camera.proj_matrix);

    // Draw a static cube
    frame.draw(cube_mesh, pbr_material)
        .with_transform(cube_transform)
        .with_color([1.0, 0.0, 0.0, 1.0])
        .submit(); // ← Submits to frame's internal list

    // Draw instanced grass
    let grass_instances = generate_grass_transforms();
    frame.draw_instanced(grass_mesh, grass_material, grass_instances)
        .submit();

    // Draw animated character
    frame.draw_skinned(character_mesh, pbr_material, character_skeleton)
        .with_transform(character_transform)
        .submit();

    // Finally, submit all draws to renderer
    renderer.execute_frame(frame.take_draw_list());
}
```

---

## Implementation Plan

### Phase 1: Remove Dead Code (GFX layer) ✅ COMPLETE

**Completed**: 2025-03-05

**Changes Made**:
1. ✅ Removed push constant ranges from `MaterialCompiler::build_pipeline()`
2. ✅ Removed `push_object_constants()` function from `render_graph/graph.rs`
3. ✅ Fixed MaterialAsset to use new MaterialData structure
4. ✅ Always bind Set 1 (bindless textures) - all current materials use it
5. ✅ Added legacy notes to embedded UI shader strings
6. ✅ All tests passing (7 passed, 0 failed)
7. ✅ Full workspace compiles

**Files Modified**:
- `katla_gfx/src/vulkan/material/compiler.rs` - Removed push constant range
- `katla_gfx/src/render_graph/graph.rs` - Removed push_object_constants function
- `katla_gfx/src/renderer.rs` - Fixed MaterialAsset initialization
- `katla_gfx/src/material/ui.rs` - Added legacy notes

### Phase 2: Expose Instance Allocation (APP layer) ✅ COMPLETE

**Completed**: 2025-03-05

**Changes Made**:
1. ✅ Created `FrameContext` struct in `katla_app/src/rendering/frame_context.rs`
2. ✅ Implemented automatic instance index allocation (next_instance_index counter)
3. ✅ Added fluent builder API (`DrawBuilder` with `with_transform`, `with_color`, `with_pbr`, `submit`)
4. ✅ Added `draw()`, `draw_instanced()`, and `draw_skinned()` methods
5. ✅ Added `FrameUniforms` struct for camera and lighting configuration
6. ✅ Added `take_draw_list()` method to flush accumulated draws
7. ✅ All tests passing (161 passed, 0 failed)

**Files Created**:
- `katla_app/src/rendering/frame_context.rs` - Complete FrameContext implementation

**Files Modified**:
- `katla_app/src/rendering/mod.rs` - Export FrameContext
- `katla_app/src/lib.rs` - Export FrameContext at crate level

**API Example**:
```rust
let mut frame = FrameContext::new();
frame.set_camera(&view_matrix, &proj_matrix);

// Single draw
frame.draw(cube_mesh, pbr_material)
    .with_transform(cube_transform)
    .with_color([1.0, 0.0, 0.0, 1.0])
    .submit();

// Instanced draw
frame.draw_instanced(grass_mesh, grass_material, instances).submit();

// Skinned mesh
frame.draw_skinned(character_mesh, pbr_material, skeleton)
    .with_transform(character_transform)
    .submit();

// Get draw list for renderer
let draw_list = frame.take_draw_list();
```

### Phase 3: Update Storage Manager ✅ COMPLETE

**Completed**: 2025-03-05

**Changes Made**:
1. ✅ Added `instance_index` field to DrawCall struct
2. ✅ Added `with_instance_index()` builder method to DrawCall
3. ✅ Updated FrameContext to set instance_index on submit
4. ✅ Added `ObjectData` struct to StorageUniformManager
5. ✅ Added `update_objects_bulk()` method for efficient batch writes
6. ✅ Added `execute_draw_calls()` method to VulkanRenderer
7. ✅ Updated app renderer to use new FrameContext API
8. ✅ All tests passing (285 katla_gfx + 161 katla_app = 446 total)

**Files Created**:
- None (all changes were to existing files)

**Files Modified**:
- `katla_gfx/src/renderer/types.rs` - Added instance_index field and builder method
- `katla_gfx/src/vulkan/material/storage_uniform.rs` - Added ObjectData and bulk update method
- `katla_gfx/src/renderer.rs` - Added execute_draw_calls() method
- `katla_app/src/rendering/frame_context.rs` - Updated submit() to set instance_index
- `katla_app/src/application/renderer.rs` - Integrated FrameContext with renderer

**API Example**:
```rust
// App layer - automatic instance allocation
let mut frame = FrameContext::new();
frame.set_camera(&view, &proj);

frame.draw(mesh, material)
    .with_transform(matrix)
    .with_color([1.0, 0.0, 0.0, 1.0])
    .submit(); // instance_index automatically set

// Gfx layer - write to storage buffer
renderer.execute_draw_calls(&draw_list);
// Internally writes each draw's data at its instance_index
```

**Storage Buffer Layout**:
- Offset 0: Frame uniforms (256 bytes)
- Offset 256+: Object array (112 bytes × 256 objects = 28,672 bytes)
- Total: 28,928 bytes (~28 KB)

### Phase 4: Skeleton API Cleanup ✅ COMPLETE

**Completed**: 2025-03-05

**Changes Made**:
1. ✅ Made Set 2 (skeleton) binding explicit in render_graph bind_descriptor_sets()
2. ✅ Added skeleton descriptor binding when draw_call.skeleton is Some
3. ✅ Added InvalidSkeletonHandle error variant
4. ✅ Added documentation for descriptor set layout in code comments
5. ✅ All tests passing (446 total passed)

**Files Modified**:
- `katla_gfx/src/render_graph/graph.rs` - Updated bind_descriptor_sets() to conditionally bind Set 2
- `katla_gfx/src/render_graph/error.rs` - Added InvalidSkeletonHandle variant

**API Behavior**:
- Set 2 (skeleton joint matrices) is now automatically bound when `draw_call.skeleton` is present
- The `draw_skinned()` API in FrameContext handles skeleton allocation
- No hybrid systems - skeleton binding is explicit and consistent

### Phase 5: Documentation ✅ COMPLETE

**Completed**: 2025-03-05

**Documentation Added**:
1. ✅ All new types have inline documentation (FrameContext, DrawBuilder, FrameUniforms)
2. ✅ Descriptor set layout documented in CLAUDE.md for shader authors
3. ✅ bind_descriptor_sets() has comprehensive doc comments
4. ✅ DrawCall instance_index field documented with Set 0/Binding 1 reference
5. ✅ Skeleton binding documented in code and comments

**Files Updated**:
- `katla_gfx/CLAUDE.md` - Comprehensive descriptor set layout documentation
- `katla_app/src/rendering/frame_context.rs` - Full API documentation with examples
- `katla_gfx/src/render_graph/graph.rs` - Descriptor set binding documentation

---

## Descriptor Set Layout (Final)

```
┌─────────────────────────────────────────────────────────────┐
│ Set 0: Per-Frame & Per-Object Data (Storage Buffers)        │
├─ Binding 0: FrameUniforms (view, proj, lighting, etc.)      │
│  - Updated once per frame via renderer.set_frame_uniforms() │
├─ Binding 1: array<ObjectUniforms> (instance-indexed)       │
│  - Each draw allocates instance_index slot                  │
│  - Updated via FrameContext draw builders                   │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│ Set 1: Global Resources (Bindless Textures)                 │
├─ Binding 0: binding_array<texture_2d, 4096>                 │
│  - Pre-populated at startup, stays constant                 │
├─ Binding 1: shared_sampler                                  │
│  - Single sampler for all textures                          │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│ Set 2: Optional Features (Skeletal Animation)               │
├─ Binding 0: array<mat4x4f> joint_matrices                   │
│  - Only bound for skinned mesh draws                        │
│  - Updated per-frame via skeleton system                    │
└─────────────────────────────────────────────────────────────┘
```

---

## Why This Works For Everyone

### For GFX (Maintainability):
- ✅ Clean separation: frame data (Set 0) vs resources (Set 1) vs optional (Set 2)
- ✅ No push constants = WebGPU compatible
- ✅ Instance indexing is explicit and trackable
- ✅ Single buffer pattern = efficient memory usage

### For APP (Developer Experience):
- ✅ Fluent builder API = discoverable and composable
- ✅ Instance allocation is automatic = no manual index tracking
- ✅ Clear distinction between static/instanced/skinned draws
- ✅ No descriptor set boilerplate = "just draw stuff"

### No Hybrid Systems:
- ✅ All draws go through the same instance-allocated path
- ✅ Single storage buffer pattern for all per-object data
- ✅ Skeleton binding is explicit, not a special case

---

## Migration Path

**Old Code:**
```rust
draw_list.push(DrawCall::new(mesh, material)
    .with_transform(matrix)
    .with_color(color));
```

**New Code:**
```rust
frame.draw(mesh, material)
    .with_transform(matrix)
    .with_color(color)
    .submit();
```

The API surface is similar, but now the instance allocation is explicit and tracked.

---

## Open Questions

1. **Frame reset**: Should `FrameContext` auto-reset instance counter at frame start?
   - **Answer**: Yes, reset at `begin_frame()` time

2. **Overflow handling**: What if we exceed 256 instances in a frame?
   - **Answer**: Panic in debug, wrap/overflow check in release

3. **Multi-threading**: Can we allocate instance indices from multiple threads?
   - **Answer**: Not initially. Add `AtomicU32` counter later if needed

4. **Backwards compatibility**: Should we keep the old DrawList API?
   - **Answer**: No. Single API = no hybrids.

---

## Summary

The current architecture is **already close to ideal** - we just need to:
1. Remove the dead push constant code
2. Expose instance allocation through a clean API
3. Make skeleton binding explicit

The end result is a **single, unified API** that:
- Works with WGSL/WebGPU (no push constants)
- Hides descriptor set details from the app
- Uses instance indexing for all per-object data
- Clearly separates static, instanced, and skinned draws

🚀 **Let's ship this!**
