# Descriptor Pool Consolidation

Architectural decision and migration plan for consolidating descriptor pool management in `katla_gfx`.

**Status:** Approved (GFX + App maintainer consensus)
**Scope:** `katla_gfx` internal only -- zero impact on `katla_app`

## Problem

Descriptor pool creation is scattered across 11 sites in 9 files. Each component independently creates its own `vk::DescriptorPool`, manages its own lifecycle, and there's no centralized budgeting. Adding a new rendering feature requires ~30 lines of repetitive boilerplate (pool sizing, creation, allocation, cleanup).

## Decision

Implement a `DescriptorSetBuilder` backed by a `DescriptorPoolAllocator` stored in `VulkanContext`. The builder auto-detects pool category from layout flags, so component authors never specify pool type manually.

### Pool Architecture (3 categories)

| Category | Vulkan Flags | Auto-Detection | Examples |
|----------|-------------|----------------|----------|
| **Persistent** | `UPDATE_AFTER_BIND` | Layout has `UPDATE_AFTER_BIND_POOL` flag | Bindless textures, compositing, shadows |
| **Transient** | `FREE_DESCRIPTOR_SET \| UPDATE_AFTER_BIND` | Layout needs per-frame updates + freeing | Particles, compute dispatches, pose compute |
| **Simple-owned** | none | Default for all other layouts | Per-material storage uniforms, UI |

### API Design

**Primary API** (developer velocity):
```rust
let desc_set = DescriptorSetBuilder::new(&context)
    .storage_buffer(0, &particle_buffer)
    .uniform_buffer(1, &frame_data_buffer)
    .build(layout)?;  // auto-detects category, allocates, writes, returns owned DescriptorSet
```

**Low-level escape hatch** (explicit control):
```rust
let desc_set = context.allocate_descriptor_set(layout, &bindings)?;
```

## Affected Files

### Simple-owned (migrate first, lowest risk)

| File | Pool flags | Description |
|------|-----------|-------------|
| `src/vulkan/material/storage_uniform.rs:129` | none | Per-material storage uniform, max_sets=1 |
| `src/render_graph/frame/ui_rendering.rs:224` | none | Per-frame UI descriptor set, max_sets=1 |

### Transient (migrate second, medium risk)

| File | Pool flags | Description |
|------|-----------|-------------|
| `src/particles/descriptors.rs:372` | `FREE_DESCRIPTOR_SET \| UPDATE_AFTER_BIND` | 4 pools (compute+render, double-buffered), max_sets=1 each |
| `src/compute.rs:472` | `FREE_DESCRIPTOR_SET \| UPDATE_AFTER_BIND` | Per-ComputePass, variable sizes, max_sets=1 |
| `src/animation/pose_compute.rs:898` | `FREE_DESCRIPTOR_SET \| UPDATE_AFTER_BIND` | Singleton per pipeline, max_sets=1 |

### Persistent (migrate last, careful lifetime)

| File | Pool flags | Description |
|------|-----------|-------------|
| `src/vulkan/bindless_texture.rs:160` | `UPDATE_AFTER_BIND` | 4096 SAMPLED_IMAGE + 1 SAMPLER, max_sets=1 |
| `src/render_graph/descriptor_sets/compositing.rs:183` | `UPDATE_AFTER_BIND` | 8 SAMPLED_IMAGE, max_sets=1 |
| `src/renderer/shadow.rs:151` | `UPDATE_AFTER_BIND` | FRAMES_IN_FLIGHT sets, max_sets=FRAMES_IN_FLIGHT |
| `src/renderer/shadow.rs:314` | `UPDATE_AFTER_BIND` | Cascade descriptors, max_sets=FRAMES_IN_FLIGHT |

### Edge case (handle separately)

| File | Pool flags | Description |
|------|-----------|-------------|
| `src/vulkan/material/compiler.rs:177` | none | Shared pool, max_sets=1024. Pool handle passed to `SkeletonDescriptorSet::new()` in `skeleton_api.rs:27`. Multiple instances allocate from same pool. Keep as dedicated shared pool in allocator. |

### Excluded

| File | Reason |
|------|--------|
| `src/examples/particle_validation_helpers.rs:253` | Test-only code |

## Implementation

### New types

- `DescriptorPoolAllocator` -- separate struct stored in `VulkanContext`, manages persistent + transient pools
- `DescriptorSetBuilder` -- builder in `src/vulkan/descriptor_set.rs`, auto-routes to allocator

### Key design points

- Pool allocator is a **separate struct** (not inlined into `VulkanContext`)
- Category selection is **automatic** from layout flags -- callers never specify
- Transient pool uses **pool chaining** -- create new pool when full, destroy empty pools at frame boundaries
- No code currently uses `vkResetDescriptorPool` -- continue with explicit destroy
- Thread safety not a concern -- `VulkanContext` uses `Rc` (single-threaded)

### Risks

1. **Skeleton shared pool** -- one pool allocates for many `SkeletonDescriptorSet` instances; needs dedicated shared pool in allocator
2. **Varied pool sizes** -- ranges from 1 set to 1024 sets with very different descriptor type mixes; allocator needs generous initial sizes
3. **UI per-frame replacement** -- replaces descriptor sets each frame without freeing old pools (relies on `DescriptorSet::Drop`)

## Migration Order

1. Implement `DescriptorPoolAllocator` with three categorized pools
2. Implement `DescriptorSetBuilder` in `descriptor_set.rs`
3. Store allocator in `VulkanContext`
4. Migrate simple-owned: `storage_uniform.rs`, `ui_rendering.rs`
5. Migrate transient: `particles/descriptors.rs`, `compute.rs`, `pose_compute.rs`
6. Migrate persistent: `bindless_texture.rs`, `compositing.rs`, `shadow.rs`
7. Handle skeleton shared pool edge case in `material/compiler.rs`
8. Remove unused `VkDescriptorPool` wrapper from `sync.rs` if no longer needed
9. Update `storage_uniform.rs` doc-comment that references non-existent `DescriptorSetBuilder`
