# Opaque Handle Unification Plan

## Executive Summary

Refactor katla_vulkan to use a **single unified opaque handle pattern**, removing all hybrid implementations and ensuring zero `ash::vk` types in the public API.

---

## Progress Status (Updated 2026-02-26)

### Completed in katla_vulkan

| Phase | Status | Notes |
|-------|--------|-------|
| Phase 1: Create handle.rs | ✅ Done | `Handle<T>`, `ResourceStorage<T>`, all markers/aliases |
| Phase 2: Delete handles.rs | ✅ Done | File removed, content consolidated |
| Phase 3: Make Vk Wrappers internal | ⏸️ Deferred | Still public to allow katla_app migration |
| Phase 4: Update lib.rs exports | ✅ Done | Exports `Handle`, public handles from handle module |
| Phase 5: Update types.rs | ✅ Done | Uses handles from handle module |
| Phase 5.5: Particle System (vulkan) | ✅ Done | `frame.rs` resolves handles via storages, `renderer.rs` has `particle_pipelines`, `particle_layouts`, `particle_descriptors` storages |
| Phase 6: render_graph/resource.rs | ✅ Done | Full handle-based external resources with deferred resolution |
| Phase 7: viewport.rs | ✅ N/A | `ViewportHandle(usize)` works, no changes needed |
| Phase 8: Internal call sites | ✅ Done | Updated for handle resolution |

### Remaining in katla_app

| Phase | Status | Notes |
|-------|--------|-------|
| Phase 5.5: Particle System (app) | ❌ TODO | `ParticleEmitter` stores VkXxx, needs handles |
| Phase 9: Update katla_app | ❌ TODO | Remove VkXxx imports, use handles |

### Current Build Status

- **katla_vulkan**: ✅ Compiles (warnings only)
- **katla_app**: ❌ Broken - imports `VkRenderPass`, `VkBuffer`, `VkDescriptorSet`, etc.

---

## Key Insight

Applications should not see GPU implementation details. The public API exposes only high-level resource handles:

| Layer | Handles |
|-------|---------|
| **Public (katla_app)** | `MeshHandle`, `MaterialHandle`, `TextureHandle`, `SkeletonHandle` |
| **Internal (katla_vulkan)** | `BufferHandle`, `ImageHandle`, `PipelineHandle`, `DescriptorSetHandle`, etc. |

Applications work with Materials. Pipelines are selected internally by the material system based on `MaterialKey`.

---

## Current Problems

### 1. Hybrid Implementation (7 different patterns)

| Pattern | Location | Issue |
|---------|----------|-------|
| `VkXxx` wrappers | `sync.rs` | Exposes `ash::vk` via `From`/`Into` |
| `PipelineHandle` | `renderer/handles.rs` | Uses `u32`, correct pattern |
| `TextureHandle` | `renderer/handles.rs` | Uses `u32`, correct pattern |
| `MeshHandle` | `renderer/types.rs` | Uses `usize` - inconsistent |
| `MaterialHandle` | `renderer/types.rs` | Uses `usize` - inconsistent |
| `SkeletonHandle` | `renderer/types.rs` | Uses `u32` - inconsistent |
| `ResourceId` | `render_graph/resource.rs` | Exposes `VkBuffer`/`VkImage` in `ResourceKind` |

### 2. Public API Leaks `ash::vk`

```rust
// lib.rs - EXPOSES raw-ish Vulkan types
pub use sync::{
    VkBuffer, VkImage, VkPipeline, ...  // These allow From<ash::vk>
};
```

### 3. Render Graph Uses Vk Types

```rust
// render_graph/resource.rs
pub enum ResourceKind {
    ExternalBuffer { buffer: VkBuffer },  // Public exposure
    ExternalImage { image: VkImage, ... }, // Public exposure
}
```

### 4. Particle System Uses Vk Types Directly

```rust
// renderer/types.rs
pub struct ParticleDispatch {
    pub pipeline: VkPipeline,           // Public exposure
    pub pipeline_layout: VkPipelineLayout,
    pub descriptor_set: VkDescriptorSet,
}
```

**Critical Issue**: These types are used directly with Vulkan command buffer operations:
```rust
// renderer/frame.rs
command_buffer.bind_pipeline(particle.pipeline.vk(), vk::PipelineBindPoint::COMPUTE);
```

The plan's `Handle<T>` is opaque with no `.vk()` method. To get the raw Vulkan handle, we need to:
1. Look up the handle in a storage
2. Call `.vk()` on the stored `VkXxx` type

This requires the **Handle Resolution Pattern** (see Target Architecture).

---

## Target Architecture

### Single Generic Handle Type

```rust
// NEW: katla_vulkan/src/handle.rs

use std::marker::PhantomData;

/// Opaque handle to a GPU resource.
/// 
/// - Copy type (just a u32 index)
/// - Type-safe via phantom types
/// - No access to underlying Vulkan types
/// - Resources accessed through storage types only
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Handle<T> {
    index: u32,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Handle<T> {
    pub const NONE: Self = Self { 
        index: u32::MAX, 
        _marker: PhantomData 
    };
    
    pub fn is_none(self) -> bool {
        self.index == u32::MAX
    }
    
    pub fn is_some(self) -> bool {
        self.index != u32::MAX
    }
    
    pub(crate) fn new(index: u32) -> Self {
        Self { index, _marker: PhantomData }
    }
    
    pub(crate) fn index(self) -> u32 {
        self.index
    }
}

impl<T> Default for Handle<T> {
    fn default() -> Self {
        Self::NONE
    }
}
```

### Marker Types - Internal

```rust
// katla_vulkan/src/handle.rs

// INTERNAL: GPU resource markers (not exported)
pub(crate) struct BufferMarker;
pub(crate) struct ImageMarker;
pub(crate) struct ImageViewMarker;
pub(crate) struct SamplerMarker;
pub(crate) struct PipelineMarker;
pub(crate) struct PipelineLayoutMarker;
pub(crate) struct DescriptorSetMarker;
pub(crate) struct DescriptorSetLayoutMarker;
pub(crate) struct RenderPassMarker;
pub(crate) struct FramebufferMarker;
pub(crate) struct SemaphoreMarker;
pub(crate) struct FenceMarker;
pub(crate) struct CommandBufferMarker;

// INTERNAL: Type aliases
pub(crate) type BufferHandle = Handle<BufferMarker>;
pub(crate) type ImageHandle = Handle<ImageMarker>;
pub(crate) type ImageViewHandle = Handle<ImageViewMarker>;
pub(crate) type SamplerHandle = Handle<SamplerMarker>;
pub(crate) type PipelineHandle = Handle<PipelineMarker>;
pub(crate) type PipelineLayoutHandle = Handle<PipelineLayoutMarker>;
pub(crate) type DescriptorSetHandle = Handle<DescriptorSetMarker>;
```

### Marker Types - Public

```rust
// katla_vulkan/src/handle.rs

// PUBLIC: High-level resource markers (exported)
pub struct MeshMarker;
pub struct MaterialMarker;
pub struct TextureMarker;
pub struct SkeletonMarker;

// PUBLIC: Type aliases (exported)
pub type MeshHandle = Handle<MeshMarker>;
pub type MaterialHandle = Handle<MaterialMarker>;
pub type TextureHandle = Handle<TextureMarker>;
pub type SkeletonHandle = Handle<SkeletonMarker>;
```

### Resource Storage

```rust
// katla_vulkan/src/handle.rs (consolidated from handles.rs)

/// Central storage for GPU resources.
pub struct ResourceStorage<T> {
    resources: Vec<Option<T>>,
    free_indices: Vec<u32>,
    _marker: PhantomData<T>,
}

impl<T> ResourceStorage<T> {
    pub fn new() -> Self { ... }
    pub fn with_capacity(capacity: usize) -> Self { ... }
    pub fn insert(&mut self, resource: T) -> u32 { ... }
    pub fn get(&self, handle: u32) -> Option<&T> { ... }
    pub fn get_mut(&mut self, handle: u32) -> Option<&mut T> { ... }
    pub fn remove(&mut self, handle: u32) -> Option<T> { ... }
    pub fn contains(&self, handle: u32) -> bool { ... }
    pub fn len(&self) -> usize { ... }
    pub fn is_empty(&self) -> bool { ... }
    pub fn clear(&mut self) { ... }
    pub fn iter(&self) -> impl Iterator<Item = &T> { ... }
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> { ... }
}
```

### Handle Resolution for Command Buffers

The key challenge: command buffer operations like `cmd_bind_pipeline()` require raw Vulkan handles, but the new `Handle<T>` type is opaque with no `.vk()` method.

**Solution: Storage Resolution Pattern** (used by wgpu, Bevy, bgfx)

Handles are resolved to actual GPU objects at command buffer recording time through a storage:

```rust
// In VulkanRenderer
pub struct VulkanRenderer {
    // ... existing fields ...
    
    /// Internal GPU resource storages (pub(crate) access only)
    pub(crate) compute_pipelines: ResourceStorage<VkPipeline>,
    pub(crate) graphics_pipelines: ResourceStorage<VkPipeline>,
    pub(crate) pipeline_layouts: ResourceStorage<VkPipelineLayout>,
    pub(crate) descriptor_sets: ResourceStorage<VkDescriptorSet>,
}

// In frame.rs - command buffer recording
for dispatch in &draw_list.particle_dispatches {
    // Resolve handles to actual Vulkan objects
    let pipeline = renderer.compute_pipelines.get(dispatch.pipeline);
    let layout = renderer.pipeline_layouts.get(dispatch.pipeline_layout);
    let descriptor = renderer.descriptor_sets.get(dispatch.descriptor_set);
    
    // Now we can call .vk() on the stored objects
    command_buffer.bind_pipeline(pipeline.vk(), vk::PipelineBindPoint::COMPUTE);
    command_buffer.bind_descriptor_sets(
        vk::PipelineBindPoint::COMPUTE,
        layout.vk(),
        &[descriptor.vk()],
    );
}
```

This pattern:
- Keeps handles opaque (no `.vk()` on `Handle<T>`)
- Resolves to stored objects only at command recording time
- Follows the same pattern as wgpu/Bevy/bgfx

---

## Refactoring Steps

### Phase 1: Create handle.rs (New File)

**File to CREATE:**
- `katla_vulkan/src/handle.rs`

**Contents:**
1. `Handle<T>` generic type
2. All marker types (internal `pub(crate)` and public `pub`)
3. All type aliases
4. `ResourceStorage<T>` (moved from `handles.rs`)

### Phase 2: Delete handles.rs

**File to DELETE:**
- `katla_vulkan/src/renderer/handles.rs`

**Contents moved to `handle.rs`:**
- `ResourceStorage<T>`

**Contents deleted (replaced by `Handle<T>`):**
- `PipelineHandle` struct
- `TextureHandle` struct

### Phase 3: Make Vk Wrappers Internal-Only

**Files to MODIFY:**
- `katla_vulkan/src/sync.rs`

**Changes:**
1. Remove all `impl From<VkXxx> for ash::vk::Xxx`
2. Remove all `impl From<ash::vk::Xxx> for VkXxx`
3. Remove `impl AsRef<ash::vk::Xxx> for VkXxx`
4. Keep `pub(crate) fn vk(&self)` for internal use
5. Make structs `pub(crate)` instead of `pub`

**Delete from sync.rs:**
```rust
// DELETE these impls:
impl From<vk::Semaphore> for VkSemaphore { ... }
impl From<VkSemaphore> for vk::Semaphore { ... }
impl AsRef<vk::Semaphore> for VkSemaphore { ... }
// (same for all VkXxx types)
```

### Phase 4: Update lib.rs Exports

**Files to MODIFY:**
- `katla_vulkan/src/lib.rs`

**Delete:**
```rust
// DELETE these lines:
pub use sync::{
    VkBuffer, VkCommandBuffer, VkDescriptorPool, VkDescriptorSet, VkDescriptorSetLayout, VkFence,
    VkFramebuffer, VkImage, VkImageView, VkPipeline, VkPipelineLayout, VkRenderPass, VkSampler,
    VkSemaphore,
};
```

**Add:**
```rust
// PUBLIC API - application-level handles only
pub use handle::{Handle, MeshHandle, MaterialHandle, TextureHandle, SkeletonHandle};
```

**Update module declaration:**
```rust
pub mod handle;  // NEW
// Remove: pub mod renderer::handles; (file deleted)
```

### Phase 5: Update renderer/types.rs

**Files to MODIFY:**
- `katla_vulkan/src/renderer/types.rs`

**Changes:**
1. DELETE `MeshHandle(pub usize)` - use `crate::handle::MeshHandle`
2. DELETE `MaterialHandle(pub usize)` - use `crate::handle::MaterialHandle`
3. DELETE `SkeletonHandle(pub u32)` - use `crate::handle::SkeletonHandle`
4. REMOVE `VkDescriptorSet`, `VkPipeline`, `VkPipelineLayout` imports
5. Update `ParticleDispatch` and `ParticleRender` to use internal handles

**Before:**
```rust
use crate::sync::{VkDescriptorSet, VkPipeline, VkPipelineLayout};

pub struct MeshHandle(pub usize);
pub struct MaterialHandle(pub usize);
pub struct SkeletonHandle(pub u32);

pub struct ParticleDispatch {
    pub pipeline: VkPipeline,
    pub pipeline_layout: VkPipelineLayout,
    pub descriptor_set: VkDescriptorSet,
    ...
}
```

**After:**
```rust
use crate::handle::{PipelineHandle, PipelineLayoutHandle, DescriptorSetHandle};

// Re-export from handle module
pub use crate::handle::{MeshHandle, MaterialHandle, SkeletonHandle};

pub struct ParticleDispatch {
    pub pipeline: PipelineHandle,
    pub pipeline_layout: PipelineLayoutHandle,
    pub descriptor_set: DescriptorSetHandle,
    ...
}
```

### Phase 5.5: Update Particle System for Handle-based Dispatch

This phase addresses the critical issue: `ParticleDispatch`/`ParticleRender` contain `VkPipeline`, `VkPipelineLayout`, `VkDescriptorSet` which are used directly with command buffer operations. The solution is to use internal handles and resolve them at command recording time.

**Files to MODIFY:**
- `katla_vulkan/src/renderer/types.rs`
- `katla_vulkan/src/renderer/frame.rs`
- `katla_vulkan/src/renderer.rs`
- `katla_app/src/components/rendering/particle.rs`
- `katla_app/src/application/renderer/mod.rs`

**Changes:**

1. **types.rs**: Update `ParticleDispatch`/`ParticleRender` to use `Handle<T>` types:
   ```rust
   use crate::handle::{PipelineHandle, PipelineLayoutHandle, DescriptorSetHandle};
   
   pub struct ParticleDispatch {
       pub pipeline: PipelineHandle,
       pub pipeline_layout: PipelineLayoutHandle,
       pub descriptor_set: DescriptorSetHandle,
       pub frame_data: [f32; 4],
       pub workgroup_count: u32,
   }
   
   pub struct ParticleRender {
       pub pipeline: PipelineHandle,
       pub pipeline_layout: PipelineLayoutHandle,
       pub frame_descriptor_set: DescriptorSetHandle,
       pub particle_descriptor_set: DescriptorSetHandle,
       pub particle_count: u32,
   }
   ```

2. **renderer.rs**: Add storages for internal GPU resources:
   ```rust
   pub struct VulkanRenderer {
       // ... existing fields ...
       
       /// Compute pipelines for particle systems (internal)
       pub(crate) compute_pipelines: ResourceStorage<VkPipeline>,
       /// Pipeline layouts (internal)
       pub(crate) pipeline_layouts: ResourceStorage<VkPipelineLayout>,
       /// Descriptor sets (internal)
       pub(crate) descriptor_sets: ResourceStorage<VkDescriptorSet>,
   }
   
   impl VulkanRenderer {
       /// Register a compute pipeline and return a handle
       pub fn register_compute_pipeline(&mut self, pipeline: VkPipeline) -> PipelineHandle {
           self.compute_pipelines.insert(pipeline)
       }
       
       /// Register a pipeline layout and return a handle
       pub fn register_pipeline_layout(&mut self, layout: VkPipelineLayout) -> PipelineLayoutHandle {
           self.pipeline_layouts.insert(layout)
       }
       
       /// Register a descriptor set and return a handle
       pub fn register_descriptor_set(&mut self, set: VkDescriptorSet) -> DescriptorSetHandle {
           self.descriptor_sets.insert(set)
       }
   }
   ```

3. **frame.rs**: Resolve handles when recording commands:
   ```rust
   // In render_frame(), particle dispatch section:
   for particle in draw_list.particle_dispatches.iter() {
       // Resolve handles to actual Vulkan objects
       let pipeline = self.compute_pipelines.get(particle.pipeline);
       let layout = self.pipeline_layouts.get(particle.pipeline_layout);
       let descriptor = self.descriptor_sets.get(particle.descriptor_set);
       
       command_buffer.bind_pipeline(pipeline.vk(), vk::PipelineBindPoint::COMPUTE);
       command_buffer.bind_descriptor_sets(
           vk::PipelineBindPoint::COMPUTE,
           layout.vk(),
           &[descriptor.vk()],
       );
       // ... push constants and dispatch
   }
   ```

4. **particle.rs (katla_app)**: Store handles instead of direct objects:
   ```rust
   pub struct ParticleEmitter {
       /// Handle to compute pipeline (registered with VulkanRenderer)
       pub pipeline_handle: PipelineHandle,
       /// Handle to pipeline layout
       pub layout_handle: PipelineLayoutHandle,
       /// Handle to descriptor set
       pub descriptor_handle: DescriptorSetHandle,
       /// Handle to render pipeline
       pub render_pipeline_handle: PipelineHandle,
       /// Handle to render pipeline layout
       pub render_layout_handle: PipelineLayoutHandle,
       /// Handle to render particle descriptor
       pub render_descriptor_handle: DescriptorSetHandle,
       // ... other fields
   }
   
   impl ParticleEmitter {
       // Remove methods that return VkXxx directly
       // Instead, provide handle getters:
       pub fn pipeline(&self) -> PipelineHandle { self.pipeline_handle }
       pub fn layout(&self) -> PipelineLayoutHandle { self.layout_handle }
       pub fn descriptor(&self) -> DescriptorSetHandle { self.descriptor_handle }
   }
   ```

5. **renderer/mod.rs (katla_app)**: Update particle dispatch creation:
   ```rust
   // When creating ParticleDispatch:
   let dispatch = ParticleDispatch {
       pipeline: emitter.pipeline(),
       pipeline_layout: emitter.layout(),
       descriptor_set: emitter.descriptor(),
       frame_data: [0.0; 4],
       workgroup_count: emitter.workgroup_count(),
   };
   ```

### Phase 6: Update render_graph/resource.rs

**Files to MODIFY:**
- `katla_vulkan/src/render_graph/resource.rs`

**Changes:**
1. Remove `VkBuffer`, `VkImage`, `VkImageView` from `ResourceKind::ExternalXxx`
2. Use internal `BufferHandle`, `ImageHandle`, `ImageViewHandle`

**Before:**
```rust
pub enum ResourceKind {
    ExternalBuffer { buffer: VkBuffer },
    ExternalImage { image: VkImage, image_view: VkImageView, ... },
}
```

**After:**
```rust
use crate::handle::{BufferHandle, ImageHandle, ImageViewHandle};

pub enum ResourceKind {
    ExternalBuffer { buffer: BufferHandle },
    ExternalImage { image: ImageHandle, image_view: ImageViewHandle, ... },
}
```

### Phase 7: Update viewport.rs

**Files to MODIFY:**
- `katla_vulkan/src/viewport.rs`

**Changes:**
1. Update `ViewportHandle` to use `Handle<T>` pattern or keep as simple index

### Phase 8: Update All Internal Call Sites

**Files to MODIFY:**
- All files in `katla_vulkan/src/`

**Pattern:**
- Replace `VkXxx::from(vk_value)` with internal-only conversions
- Replace direct `handle.vk()` calls with storage access
- Update imports to use `crate::handle::XxxHandle`

### Phase 9: Update katla_app

**Files to MODIFY:**
- All files in `katla_app/src/` that reference vulkan handles

**Changes:**
- Update imports to use public handle types (`MeshHandle`, `MaterialHandle`, etc.)
- Remove any direct `ash::vk` usage
- Access resources through `AssetRegistry` or `VulkanRenderer`

### Phase 10: Update Tests

**Files to MODIFY:**
- `katla_vulkan/src/sync.rs` tests
- `katla_vulkan/src/handle.rs` tests (moved from handles.rs)
- `katla_vulkan/src/renderer/types.rs` tests

**Changes:**
- Move `handles.rs` tests to `handle.rs`
- Update tests to use new handle types
- Remove tests for `From`/`Into` conversions that were deleted

---

## Files Summary

### CREATE
| File | Purpose |
|------|---------|
| `katla_vulkan/src/handle.rs` | Unified `Handle<T>`, markers, `ResourceStorage<T>` |

### DELETE (entire files)
| File | Reason |
|------|--------|
| `katla_vulkan/src/renderer/handles.rs` | Consolidated into `handle.rs` |

### DELETE (content from files)
| Location | What |
|----------|------|
| `sync.rs` | `From<VkXxx>` / `From<ash::vk::Xxx>` impls |
| `sync.rs` | `AsRef<ash::vk::Xxx>` impls |
| `lib.rs` | `pub use sync::{VkBuffer, ...}` |
| `renderer/types.rs` | `MeshHandle`, `MaterialHandle`, `SkeletonHandle` structs |

### MODIFY
| File | Changes |
|------|---------|
| `sync.rs` | Make wrappers `pub(crate)`, remove conversion impls |
| `lib.rs` | Export public handle types only, remove VkXxx exports |
| `renderer/types.rs` | Re-export handles from handle module, update ParticleDispatch/Render |
| `renderer/frame.rs` | Resolve handles to VkXxx at command recording time |
| `renderer.rs` | Add compute_pipelines, pipeline_layouts, descriptor_sets storages |
| `render_graph/resource.rs` | Use internal `BufferHandle`/`ImageHandle` in ResourceKind |
| `render_graph/types.rs` | Remove VkXxx re-exports |
| `viewport.rs` | Update `ViewportHandle` if needed |
| `katla_app/components/rendering/particle.rs` | Store handles instead of VkXxx, update getters |
| `katla_app/application/renderer/mod.rs` | Update ParticleDispatch/ParticleRender creation |
| All internal call sites | Update to use `crate::handle::XxxHandle` |

---

## API Comparison

### Before (Hybrid)
```rust
// Public API exposes VkXxx wrappers - BAD
pub use sync::{VkBuffer, VkImage, VkPipeline, ...};

// Different handle patterns - INCONSISTENT
pub struct PipelineHandle(pub u32);
pub struct MeshHandle(pub usize);
pub struct SkeletonHandle(pub u32);

// Can convert to ash::vk - LEAKS ABSTRACTION
let vk_buffer: vk::Buffer = VkBuffer::from(raw).into();
```

### After (Unified)
```rust
// Public API - application-level handles only
pub use handle::{Handle, MeshHandle, MaterialHandle, TextureHandle, SkeletonHandle};

// Internal handles (pub(crate))
use crate::handle::{BufferHandle, ImageHandle, PipelineHandle, ...};

// Single pattern, type-safe
pub type MeshHandle = Handle<MeshMarker>;
pub type MaterialHandle = Handle<MaterialMarker>;

// No conversion to ash::vk possible from outside crate
// Access only through storage:
let mesh = asset_registry.meshes().get(handle);
```

---

## Layer Responsibilities

### katla_app (Application Layer)
```
Uses: MeshHandle, MaterialHandle, TextureHandle, SkeletonHandle
Creates: DrawCall with MeshHandle + MaterialHandle
Never sees: Pipeline, Buffer, Image, DescriptorSet
```

### katla_vulkan (Render Layer)
```
Public: MeshHandle, MaterialHandle, TextureHandle, SkeletonHandle
Internal: BufferHandle, ImageHandle, PipelineHandle, DescriptorSetHandle, etc.
Creates: Pipelines based on MaterialKey
Manages: All GPU resources
```

---

## Validation Checklist

After refactoring:

- [ ] `cargo build` succeeds
- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy` has no warnings
- [ ] No `ash::vk` types in public API (check `cargo doc`)
- [ ] katla_app compiles without direct `ash` dependency
- [ ] Render graph works with new handles
- [ ] Particle system works with new handles
- [ ] `handles.rs` deleted, `handle.rs` created

---

## Estimated Effort

| Phase | Complexity | Risk |
|-------|------------|------|
| Phase 1: Create handle.rs | Low | Low |
| Phase 2: Delete handles.rs | Low | Low |
| Phase 3: Modify sync.rs | Medium | Medium |
| Phase 4: Update lib.rs | Low | Low |
| Phase 5: Update types.rs | Medium | Medium |
| Phase 5.5: Particle System | High | High |
| Phase 6: Update render_graph | High | High |
| Phase 7-9: Update call sites | High | Medium |
| Phase 10: Update tests | Medium | Low |

**Total: ~3-4 days of focused work**

---

## Key Insight: Handle Resolution Pattern

The most important architectural decision is the **Handle Resolution Pattern**:

1. **Handles are opaque** - `Handle<T>` has no `.vk()` method
2. **Storages hold actual objects** - `ResourceStorage<VkPipeline>` holds the real Vulkan objects
3. **Resolution at command recording** - Handles are resolved to objects only when needed for Vulkan calls

This pattern is used by:
- **wgpu**: `Id<T>` resolves to backend objects via `Hub` storage
- **Bevy**: `CachedPipelineId` resolves via `PipelineCache`
- **bgfx**: 16-bit handles resolve to backend objects at submission time

The benefit: applications never see raw Vulkan types, but the renderer can still access them internally for command buffer recording.
