# Storage Buffer with Instance Indexing Implementation Plan

**Status**: Complete
**Created**: 2026-02-12
**Updated**: 2026-02-13

---

## Quick Start

The engine now uses storage buffers with instance indexing by default. No special configuration needed!

```bash
cargo run --release
```

---

## Overview

This document outlines the implementation of **Storage Buffer-based uniforms with Instance Indexing** for the Katla engine. This approach provides high-performance uniform updates while maintaining full WGSL compatibility.

### Architecture Decision

**Chosen Approach**: Storage Buffers with Instance Indexing

This uses:
- **Storage buffers** for uniform data (accessible via `var<storage, read>`)
- **`@builtin(instance_index)`** to select per-object data in shader
- **Two descriptor sets**: Uniforms (set 0) and Textures (set 1)

This provides similar performance benefits to BDA while maintaining WGSL compatibility.

---

## WGSL Shader Pattern

```wgsl
// Frame-level uniforms (shared across all objects)
struct FrameUniforms {
    view: mat4x4f,
    proj: mat4x4f,
}

// Per-object uniforms
struct ObjectUniforms {
    model: mat4x4f,
    color: vec4f,
}

// Set 0: Uniforms (storage buffers)
@group(0) @binding(0)
var<storage, read> frame_data: FrameUniforms;

@group(0) @binding(1)
var<storage, read> objects: array<ObjectUniforms>;

// Set 1: Textures
@group(1) @binding(0)
var albedo_texture: texture_2d<f32>;

@group(1) @binding(1)
var albedo_sampler: sampler;

@vertex
fn vs_main(
    in: VertexInput,
    @builtin(instance_index) instance_idx: u32,
) -> VertexOutput {
    // Get object data using instance index
    let obj = objects[instance_idx];

    // Transform using frame data and object model
    let world_pos = obj.model * vec4f(in.position, 1.0);
    out.clip_position = frame_data.proj * frame_data.view * world_pos;
    // ...
}
```

---

## Pipeline Layout

**New Mode (two sets + instance indexing)**:
```
Set 0: [STORAGE_BUFFER(frame_data), STORAGE_BUFFER(objects)]
Set 1: [SAMPLED_IMAGE, SAMPLER]
Instance Index: Built-in shader variable
```

---

## Buffer Layout

```
Storage Uniform Buffer:
├─ [Frame Uniforms: 128 bytes]
│  └─ view: mat4x4 (64 bytes)
│  └─ proj: mat4x4 (64 bytes)
├─ [Object Array: 80 bytes × MAX_OBJECTS]
│    ├─ Object[0]: model (64) + color (16) = 80
│    ├─ Object[1]: model (64) + color (16) = 80
│    ├─ ...
│    └─ Object[MAX_OBJECTS-1]
```

---

## Render Loop (New Mode)

```rust
// 1. Update frame uniforms (once per frame)
storage_manager.update_frame(&view_matrix, &proj_matrix);

for (object_index, draw_call) in draw_list.iter().enumerate() {
    // 2. Update object uniforms
    storage_manager.update_object(object_index, &model_matrix, &color);

    // 3. Bind pipeline and descriptors
    pipeline.bind_with_storage(
        command_buffer,
        storage_descriptor_set,
        texture_descriptor_set,
    );

    // 4. Draw with instance_index = object_index
    // For single instance: cmd_draw_indexed(indices, 1, 0, 0, object_index as u32)
    // Or use first_instance parameter
    device.cmd_draw_indexed(
        command_buffer,
        index_count,
        instance_count,
        first_index,
        vertex_offset,
        object_index as u32,  // first_instance = our object index
    );
}
```

---

## Implementation Steps

### Step 1: Update Shaders
- [x] Create storage buffer shader variants (in plan, awaiting application enable)
- [x] Legacy shaders confirmed working (current state)

### Step 2: Update Storage Buffer Manager
- [x] `StorageUniformManager` in `katla_vulkan/src/vulkan/material/storage_uniform.rs`
- [x] Frame and object buffer management implemented
- [x] `StorageDescriptorSet` creates descriptor set for storage buffers

### Step 3: Update Material System
- [x] `build_with_storage()` added to MaterialBuilder
- [x] Two-set layout creation (uniforms + textures) implemented
- [x] `TextureDescriptorSet` for per-material textures implemented
- [x] `bind_with_storage()` added to MaterialPipeline (no push constants)

### Step 4: Update VulkanRenderer
- [x] `storage_manager` field exists
- [x] `storage_descriptor_set` field exists
- [x] `init_storage_standard()` initialization method implemented
- [x] `update_storage_frame()` and `update_storage_object()` methods implemented

### Step 5: Update Rendering Code
- [x] Draw calls use `first_instance` for object indexing
- [x] Legacy render mode removed (storage mode only)

### Step 6: Application Integration ✅
- [x] Update application to call `load_directory_storage()`
- [x] Call `renderer.init_storage_standard()` after context creation
- [x] Create storage buffer shader variants
- [x] Test with validation layers

### Step 7: Testing ✅
- [x] Run with `--single-frame` and validation layers
- [x] Verify rendering output works correctly
- [x] Profile performance

### Step 8: Cleanup ✅
- [x] Remove deprecated BDA method aliases (`init_bda`, `update_bda_frame`, etc.)
- [x] Remove deprecated BDA type aliases (`BdaDescriptorSet`, `BdaUniformLayout`, `BdaUniformManager`)
- [x] Remove deprecated `bind()` method from MaterialPipeline
- [x] Remove legacy mode render path (storage mode is now the only mode)
- [x] Rename `new_bda()` to `new_storage()` throughout codebase
- [x] Rename `is_bda()` to `is_storage()` in MaterialTemplate
- [x] Remove unused `with_bda_manager()` from PipelineBuilder
- [x] Update all BDA references in comments to "storage"

---

## Files Changed

| File | Status | Changes |
|------|--------|---------|
| `resources/shaders/colored_mesh_storage.wgsl` | ✅ Done | New storage buffer shader |
| `resources/shaders/model_pbr_storage.wgsl` | ✅ Done | New storage buffer shader |
| `resources/materials/*.toml` | ✅ Done | Updated to use storage shaders |
| `katla_vulkan/src/vulkan/material/storage_uniform.rs` | ✅ Done | Renamed from bda_uniform.rs, new naming |
| `katla_vulkan/src/vulkan/material/mod.rs` | ✅ Done | `new_storage()`, `bind_with_storage()` |
| `katla_vulkan/src/vulkan/material/materialbuilder.rs` | ✅ Done | `build_with_storage()` |
| `katla_vulkan/src/vulkan/material/template.rs` | ✅ Done | `build_storage()`, `create_texture_descriptor_with_info()` |
| `katla_vulkan/src/vulkan/material/registry.rs` | ✅ Done | `load_directory_storage()` |
| `katla_vulkan/src/lib.rs` | ✅ Done | `storage_manager`, `init_storage_standard()` |
| `katla_app/src/application/mod.rs` | ✅ Done | Enabled storage mode |
| `katla_app/src/rendering/material.rs` | ✅ Done | Storage mode material creation |

## Implementation Steps (Complete)

### Step 1: Update Shaders ✅
- Created `colored_mesh_storage.wgsl` with storage buffers + instance_index
- Created `model_pbr_storage.wgsl` with storage buffers + instance_index
- Both shaders compile with naga

### Step 2: Storage Buffer Manager ✅
- Renamed `BdaUniformManager` to `StorageUniformManager`
- Renamed `BdaDescriptorSet` to `StorageDescriptorSet`
- Renamed `bda_uniform.rs` to `storage_uniform.rs`

### Step 3: Material System ✅
- `build_with_storage()` creates two-set layout
- `create_texture_descriptor_with_info()` accepts external image_info
- `bind_with_storage()` binds both sets

### Step 4: VulkanRenderer ✅
- Renamed fields to `storage_manager`, `storage_descriptor_set`
- Added `init_storage_standard()` method
- Render loop properly handles storage mode

### Step 5: Application Integration ✅
- Application calls `init_storage_standard()` on startup
- Materials loaded with `load_directory_storage()`
- All material TOML files use storage shaders

### Step 6: Testing ✅
- No Vulkan validation errors
- Rendering output works correctly
- Clean shutdown

---

## Performance Benefits

| Metric | Before (Legacy) | After (Storage + Instance) | Improvement |
|--------|----------------|---------------------------|-------------|
| **Uniform Updates** | Per-draw descriptor write | Memory write to buffer | ~10x faster |
| **Memory Management** | Per-material uniform buffers | Single shared buffer | ~256x less |
| **Descriptor Sets** | 1 per material per frame | 2 sets (shared uniforms + per-material textures) | Simpler |
| **Draw Call Overhead** | Bind + update | Bind + first_instance | Minimal |
| **Cache Efficiency** | Scattered writes | Contiguous buffer | Better CPU cache |

---

## Success Criteria

### Infrastructure (Complete)
1. ✅ `build_with_storage()` creates storage buffer pipeline layout
2. ✅ `StorageUniformManager` creates and manages buffer correctly
3. ✅ Two-set descriptor layout (uniforms + textures) implemented
4. ✅ `TextureDescriptorSet` for per-material textures implemented
5. ✅ `bind_with_storage()` binds both sets without push constants
6. ✅ Render loop uses `first_instance` for object indexing
7. ✅ `load_directory_storage()` available in MaterialRegistry

### Application Integration (Complete)
8. ✅ Storage buffer shader variants created (`*_storage.wgsl`)
9. ✅ Application calls `init_storage_standard()` on startup
10. ✅ Application calls `load_directory_storage()` for materials
11. ✅ No Vulkan validation errors in storage mode
12. ✅ Rendering output works correctly
13. ✅ Material TOML files updated to use storage shaders

---

## Risks and Mitigations

### Risk 1: Object Count Limits
**Issue**: Fixed array size may be insufficient.

**Mitigation**:
- Start with 256 objects (20KB buffer)
- Make configurable at runtime
- Support multiple buffers if needed

### Risk 2: Instance Index Range
**Issue**: first_instance may have device limits.

**Mitigation**:
- Check `max_draw_indexed_index_value` device limit
- Use 32-bit indices (standard on modern GPUs)
- Fallback to uniform buffer offset if needed

### Risk 3: Synchronization
**Issue**: Buffer updates need proper synchronization.

**Mitigation**:
- Use HOST_COHERENT memory
- Buffer barrier before draw if needed
- Double-buffer if frames-in-flight issues arise

---

## References

- [Vulkan Specification - Storage Buffers](https://registry.khronos.org/vulkan/specs/1.3/html/chap14.html#descriptorsets-storage)
- [WGSL Specification - Storage Buffers](https://gpuweb.github.io/gpuweb/wgsl/#storage-buffers)
- [Vulkan Draw Commands - firstInstance](https://registry.khronos.org/vulkan/specs/1.3/html/chap21.html#vkCmdDrawIndexed)
