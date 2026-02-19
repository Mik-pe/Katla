# Performance Optimization Plan: Bindless Textures & GPU Culling

## Status

### Bindless Textures: ✅ FULLY INTEGRATED (2026-02-19)

Bindless textures are now the default rendering mode:
- ✅ Feature detection (already enabled in `context.rs`)
- ✅ `BindlessTextureManager` - texture slot allocation, descriptor management, default textures
- ✅ `ObjectUniforms.texture_indices` - per-object texture index storage (112 bytes)
- ✅ `MaterialBuilder::build_bindless()` - bindless pipeline creation
- ✅ `MaterialPipeline::new_bindless()` - bindless material pipeline
- ✅ `VulkanRenderer::init_bindless()` - renderer integration
- ✅ `model_pbr_bindless.wgsl` - bindless PBR shader
- ✅ `model_simple_bindless.wgsl` - simple bindless shader
- ✅ `MaterialAsset.texture_indices` - stores bindless indices
- ✅ `Material.texture_indices` - application-layer texture indices
- ✅ Render loop binds bindless once per frame
- ✅ Default textures at reserved slots 0-4 (white, normal, MR, AO, emission)
- ✅ Fallback to legacy mode when bindless not initialized

**Remaining (optional):**
- Update GLTF loader to use bindless shaders
- Remove deprecated `TextureDescriptorSet` and `PbrTextureDescriptorSet` code

### GPU Culling: 🔲 Not Started

See plan below.

---

## Overview

Two major performance optimizations to tackle:

1. **Bindless Textures** - Single descriptor set for all textures, eliminating per-material descriptor binding
2. **GPU Culling** - Compute-based frustum culling with indirect drawing

---

## Feature 1: Bindless Textures

### Why It Matters
- **Current**: Each material has its own descriptor set for textures. Binding overhead per draw call.
- **Bindless**: One global texture array bound once per frame. Materials reference textures by index.
- **Benefit**: Reduced descriptor bindings, better GPU cache utilization, scales to thousands of textures.

### Prerequisites
- [ ] Vulkan 1.2+ or `VK_EXT_descriptor_indexing` extension
- [ ] `shaderSampledImageArrayNonUniformIndexing` feature
- [ ] `runtimeDescriptorArray` feature

### Phase 1: Infrastructure (Foundation)

#### 1.1 Feature Detection
- [ ] Add device feature checks for descriptor indexing
- [ ] Update `VulkanContext` to enable required features
- [ ] Add fallback path for non-supporting hardware (rare)

**Files:**
- `katla_vulkan/src/vulkan/context.rs`

#### 1.2 Bindless Texture Manager
- [ ] Create `BindlessTextureManager` struct
- [ ] Track texture slots with allocation/deallocation
- [ ] Maintain single descriptor set with texture array
- [ ] Handle texture updates (when loading new textures)

**New Files:**
- `katla_vulkan/src/vulkan/bindless_texture.rs`

**Key Design:**
```rust
pub struct BindlessTextureManager {
    descriptor_set: VkDescriptorSet,
    descriptor_pool: VkDescriptorPool,
    layout: VkDescriptorSetLayout,
    textures: Vec<Option<VkImageView>>,
    free_slots: Vec<u32>,
    sampler: VkSampler, // Shared sampler
}

impl BindlessTextureManager {
    pub fn allocate_slot(&mut self, view: VkImageView) -> u32;
    pub fn free_slot(&mut self, index: u32);
    pub fn update_descriptor_set(&mut self, device: &Device);
}
```

#### 1.3 Update Descriptor Layout Builder
- [ ] Add `add_binding_with_count()` method for array bindings
- [ ] Support `descriptorCount > 1` in layout creation

**Files:**
- `katla_vulkan/src/vulkan/descriptor.rs`

### Phase 2: Shader Migration

#### 2.1 Create Bindless Shader Variants
- [ ] `model_pbr_bindless.wgsl` - Full PBR with texture indices
- [ ] `model_simple_bindless.wgsl` - Simple material variant
- [ ] Update push constants to include texture indices

**Shader Pattern:**
```wgsl
struct TextureIndices {
    albedo: u32,
    normal: u32,
    metallic_roughness: u32,
    ao: u32,
    emission: u32,
}

@group(1) @binding(0)
var bindless_textures: binding_array<texture_2d<f32>, 4096>;
@group(1) @binding(1)
var shared_sampler: sampler;

// Usage:
let indices = texture_indices[draw_id]; // From storage buffer
let albedo = textureSample(
    bindless_textures[indices.albedo],
    shared_sampler,
    uv
);
```

**NOTE:** WGSL doesn't support push constants natively. Texture indices will be passed via:
- **Per-object uniform/storage buffer** - Pack `TextureIndices` into existing `ObjectUniforms` or separate storage buffer
- **Material uniform buffer** - Small per-material uniform with texture indices

**New Files:**
- `resources/shaders/model_pbr_bindless.wgsl`
- `resources/shaders/model_simple_bindless.wgsl`

### Phase 3: Material System Integration

#### 3.1 Material Asset Changes
- [ ] Add `texture_indices: Vec<u32>` to `MaterialAsset`
- [ ] Remove `texture_descriptor: Option<TextureDescriptorSet>` for bindless materials
- [ ] Add material flag for bindless vs legacy

**Files:**
- `katla_vulkan/src/vulkan/material/mod.rs`
- `katla_vulkan/src/rendering/registry.rs`

#### 3.2 Material Builder Updates
- [ ] Add `build_bindless()` method to `MaterialBuilder`
- [ ] Create bindless pipeline variants
- [ ] Register textures with `BindlessTextureManager` during build

**Files:**
- `katla_vulkan/src/vulkan/material/materialbuilder.rs`

### Phase 4: Renderer Integration

#### 4.1 Draw Call Changes
- [ ] Bind bindless descriptor set once per frame (set 1)
- [ ] Pass texture indices via per-object uniform or storage buffer (NOT push constants - WGSL limitation)
- [ ] Remove per-material texture binding for bindless materials

**Files:**
- `katla_vulkan/src/lib.rs` (VulkanRenderer)
- `katla_vulkan/src/rendering/drawlist.rs`

#### 4.2 Migration Support
- [ ] Support both bindless and legacy materials simultaneously
- [ ] Gradual migration path
- [ ] Material TOML format update for texture index specification (optional)

### Phase 5: Cleanup

- [ ] Remove legacy `TextureDescriptorSet` and `PbrTextureDescriptorSet`
- [ ] Remove legacy shader variants
- [ ] Update all materials to bindless

### Estimated Scope
- **New Files**: 3-4
- **Modified Files**: 8-10
- **Complexity**: Medium-High
- **Dependencies**: None (standalone feature)

---

## Feature 2: GPU Culling

### Why It Matters
- **Current**: CPU-based frustum culling in `CullingSystem`. Scales poorly with object count.
- **GPU Culling**: Compute shader culls objects in parallel. Scales to tens of thousands of objects.
- **Benefit**: Massive performance improvement for large scenes, frees CPU for game logic.

### Prerequisites
- [ ] Compute pipeline infrastructure (✅ already exists)
- [ ] Indirect drawing support (❌ needs implementation)
- [ ] Storage buffer infrastructure (✅ already exists via `DeviceAddressBuffer`)

### Phase 1: Indirect Drawing Infrastructure

#### 1.1 Add Indirect Draw Commands
- [ ] Add `draw_indexed_indirect()` to `CommandBuffer`
- [ ] Add `DrawIndexedIndirectCommand` struct (or use ash's)
- [ ] Add indirect buffer type

**Files:**
- `katla_vulkan/src/vulkan/commandbuffer.rs`

**New Types:**
```rust
#[repr(C)]
pub struct DrawIndexedIndirectCommand {
    pub index_count: u32,
    pub instance_count: u32,
    pub first_index: u32,
    pub vertex_offset: i32,
    pub first_instance: u32,
}
```

#### 1.2 Indirect Command Buffer
- [ ] Create `IndirectCommandBuffer` wrapper
- [ ] Manage GPU buffer for indirect commands
- [ ] Support atomic counter for visible count

**New Files:**
- `katla_vulkan/src/vulkan/indirect_buffer.rs`

### Phase 2: GPU Culling Shader

#### 2.1 Culling Compute Shader
- [ ] Create `frustum_cull.wgsl` compute shader
- [ ] Input: bounding spheres, model matrices, frustum planes
- [ ] Output: filtered draw commands, visible object data

**New Files:**
- `resources/shaders/culling/frustum_cull.wgsl`

**Shader Design:**
```wgsl
struct BoundingSphere {
    center: vec3f,
    radius: f32,
}

struct DrawCommand {
    index_count: u32,
    instance_count: u32,
    first_index: u32,
    vertex_offset: i32,
    first_instance: u32,
}

struct FrustumPlane {
    normal: vec3f,
    distance: f32,
}

@group(0) @binding(0)
var<storage, read> bounding_spheres: array<BoundingSphere>;
@group(0) @binding(1)
var<storage, read> model_matrices: array<mat4x4f>;
@group(0) @binding(2)
var<storage, read> frustum: array<FrustumPlane, 6>;
@group(0) @binding(3)
var<storage, read_write> draw_commands: array<DrawCommand>;
@group(0) @binding(4)
var<storage, read_write> visible_indices: array<u32>;
@group(0) @binding(5)
var<storage, read_write> visible_count: atomic<u32>;

@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) global_id: vec3u) {
    let object_idx = global_id.x;
    if (object_idx >= arrayLength(&bounding_spheres)) { return; }

    let bounds = bounding_spheres[object_idx];
    let model = model_matrices[object_idx];

    // Transform bounding sphere to world space
    let world_center = model * vec4f(bounds.center, 1.0);

    // Test against all 6 frustum planes
    var visible = true;
    for (var i = 0u; i < 6u; i++) {
        let plane = frustum[i];
        let dist = dot(world_center.xyz, plane.normal) + plane.distance;
        if (dist < -bounds.radius) {
            visible = false;
            break;
        }
    }

    if (visible) {
        let slot = atomicAdd(&visible_count, 1u);
        visible_indices[slot] = object_idx;
        draw_commands[slot] = original_commands[object_idx];
    }
}
```

### Phase 3: Culling Buffer Management

#### 3.1 Culling Data Structures
- [ ] Create `CullingBuffer` manager
- [ ] Manage bounding volume buffer (GPU)
- [ ] Manage draw command buffer (GPU, read-write)
- [ ] Manage visible count buffer (atomic)

**New Files:**
- `katla_vulkan/src/vulkan/culling_buffer.rs`

**Key Design:**
```rust
pub struct CullingBuffer {
    bounding_spheres: DeviceAddressBuffer<BoundingSphere>,
    model_matrices: DeviceAddressBuffer<Mat4>,
    frustum_planes: DeviceAddressBuffer<FrustumPlane>,
    draw_commands: DeviceAddressBuffer<DrawIndexedIndirectCommand>,
    visible_indices: DeviceAddressBuffer<u32>,
    visible_count: DeviceAddressBuffer<AtomicU32>,
}

impl CullingBuffer {
    pub fn update_bounds(&mut self, objects: &[(BoundingSphere, Mat4)]);
    pub fn update_frustum(&mut self, frustum: &Frustum);
    pub fn reset_visible_count(&mut self);
    pub fn visible_draw_commands(&self) -> &[DrawIndexedIndirectCommand];
}
```

### Phase 4: Render Graph Integration

#### 4.1 Culling Pass
- [ ] Add culling compute pass before geometry pass
- [ ] Set up proper barriers (culling -> indirect draw)
- [ ] Integrate with existing render graph

**Files:**
- `katla_vulkan/src/render_graph/pass.rs`
- `katla_app/src/application/mod.rs` (or wherever render graph is built)

#### 4.2 Geometry Pass Update
- [ ] Switch to `draw_indexed_indirect()` for geometry rendering
- [ ] Use visible draw commands from culling pass
- [ ] Handle case of zero visible objects

### Phase 5: ECS Integration

#### 5.1 Hybrid Culling System
- [ ] Update `CullingSystem` to upload bounds to GPU
- [ ] Remove CPU-side frustum culling (or keep as fallback)
- [ ] Update `BoundingVolume` component for GPU format

**Files:**
- `katla_app/src/systems/culling_system.rs`
- `katla_app/src/components/scene/bounding.rs`

### Phase 6: Optimization (Optional)

- [ ] **Occlusion culling** - Add depth buffer-based occlusion
- [ ] **Hierarchical culling** - BVH or octree for massive scenes
- [ ] **GPU-driven LOD** - Select LOD level in culling shader

### Estimated Scope
- **New Files**: 4-5
- **Modified Files**: 6-8
- **Complexity**: High
- **Dependencies**: Indirect drawing infrastructure

---

## Recommended Implementation Order

### Option A: Bindless First (Recommended)
1. **Bindless Textures** - Lower risk, self-contained, immediate draw call improvement
2. **GPU Culling** - Builds on improved rendering pipeline

### Option B: GPU Culling First
1. **GPU Culling** - Bigger perf win for large scenes
2. **Bindless Textures** - Further reduces overhead

### Option C: Parallel (Ambitious)
1. Work on both simultaneously (different files, minimal overlap)

---

## Success Metrics

### Bindless Textures
- [ ] All materials use single bindless descriptor set
- [ ] No per-material descriptor bindings in render loop
- [ ] Support for 4096+ textures
- [ ] No visible texture sampling errors

### GPU Culling
- [ ] 10,000+ objects culled in <1ms GPU time
- [ ] Indirect drawing works correctly
- [ ] No visual artifacts (flickering, missing objects)
- [ ] CPU culling system removed or deprecated

---

## Questions to Resolve

1. **Bindless sampler strategy**: Single shared sampler vs. array of samplers for different filter modes?
2. **Texture index storage**: Per-object uniform buffer (pack into existing ObjectUniforms) vs. separate storage buffer? (Push constants NOT an option - WGSL doesn't support them)
3. **Culling granularity**: Per-mesh vs. per-object culling?
4. **LOD support**: Include LOD selection in culling shader from day one?

---

## References

- [Vulkan Descriptor Indexing](https://www.khronos.org/blog/vulkan-descriptor-indexing)
- [NVIDIA Bindless Textures](https://developer.nvidia.com/content/binding-textures-one-bindless-texture)
- [GPU-Driven Rendering Pipelines](https://developer.nvidia.com/gpugems/gpugems3/part-ii-light-and-shadows/chapter-14-advanced-ambient-occlusion)
- [Our Machinery Culling](https://ourmachinery.com/post/gpu-driven-rendering/)
