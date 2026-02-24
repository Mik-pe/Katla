# Katla RHI Design Document

Research-based recommendations for improving the Render Hardware Interface layer.

## Current State Assessment

### Strengths
- Modern Vulkan 1.3 with dynamic rendering
- Synchronization2 for barriers
- Storage buffers with instance indexing
- Type-safe wrapper types
- Template-based materials with hot reload
- Bindless texture system

### Pain Points
- No descriptor set caching
- No pipeline caching
- Multiple descriptor wrapper types (StorageDescriptorSet, SkeletonDescriptorSet, etc.)
- Materials expose internal types (Rc<RefCell<MaterialPipeline>>)
- No buffer pooling

---

## Priority Recommendations

### 1. Descriptor Set Cache (High Priority)

**Problem:** Creating descriptor sets every frame is expensive.

**Solution:** LRU cache keyed by layout + resource signature.

```rust
pub struct DescriptorSetCache {
    cache: HashMap<DescriptorKey, VkDescriptorSet>,
    pool: VkDescriptorPool,
}

impl DescriptorSetCache {
    pub fn get_or_create(
        &mut self,
        layout: VkDescriptorSetLayout,
        resources: &[ResourceBinding]
    ) -> VkDescriptorSet;
}
```

**Impact:** Eliminates per-frame descriptor allocations.

---

### 2. Pipeline Cache (High Priority)

**Problem:** No caching of compiled pipelines.

**Solution:** Hash-based lookup by shader + render state.

```rust
pub struct PipelineCache {
    cache: HashMap<PipelineKey, VkPipeline>,
}

#[derive(Hash, Eq, PartialEq)]
struct PipelineKey {
    vertex_shader: ShaderId,
    fragment_shader: ShaderId,
    render_state: RenderState,
    vertex_format: VertexFormat,
}
```

**Impact:** Faster pipeline lookup, better hot reload performance.

---

### 3. Unified Descriptor Builder (High Priority)

**Problem:** Multiple descriptor types: StorageDescriptorSet, SkeletonDescriptorSet, BufferDescriptorSet.

**Solution:** Single unified builder.

```rust
let descriptor = DescriptorSetBuilder::new()
    .add_storage_buffer(0, &frame_buffer)
    .add_storage_buffer(1, &object_buffer)
    .add_combined_sampler(2, &texture, &sampler)
    .build(layout)?;
```

**Impact:** Reduces code duplication, easier to use.

---

### 4. Opaque Resource Handles (Medium Priority)

**Problem:** Materials expose `Rc<RefCell<MaterialPipeline>>`.

**Solution:** Opaque handles with central registry.

```rust
pub struct MaterialHandle(u32);
pub struct PipelineHandle(u32);
pub struct TextureHandle(u32);
```

**Impact:** Cleaner API, better encapsulation.

---

### 5. Per-Frame Command Pools (Medium Priority)

**Problem:** Command buffers not pooled per-frame.

**Solution:** One pool per frame-in-flight, reset entire pool.

```rust
pub struct FrameResources {
    command_pool: vk::CommandPool,
    // ... other per-frame resources
}

impl FrameResources {
    pub fn reset(&mut self) {
        unsafe {
            self.device.reset_command_pool(
                self.command_pool,
                vk::CommandPoolResetFlags::empty()
            );
        }
    }
}
```

**Impact:** Better memory management, no fragmentation.

---

### 6. Buffer Pool (Medium Priority)

**Problem:** No reuse of temporary buffers.

**Solution:** Pool with acquire/release pattern.

```rust
pub struct BufferPool<T> {
    available: Vec<T>,
    in_use: Vec<T>,
}

impl<T> BufferPool<T> {
    pub fn acquire(&mut self) -> Option<T>;
    pub fn release(&mut self, buffer: T);
}
```

**Impact:** Reduced allocations for staging/temporary buffers.

---

### 7. Sampler Cache (Low Priority)

**Problem:** Samplers created repeatedly.

**Solution:** Cache by configuration.

```rust
pub struct SamplerCache {
    cache: HashMap<SamplerConfig, vk::Sampler>,
}

#[derive(Hash, Eq, PartialEq)]
struct SamplerConfig {
    filter: FilterMode,
    wrap_u: WrapMode,
    wrap_v: WrapMode,
    mip_lod_bias: f32,
    max_anisotropy: f32,
}
```

**Impact:** Fewer sampler objects, consistent configuration.

---

## Industry Patterns Reference

| Pattern | wgpu | bgfx | Godot 4 | Bevy |
|---------|------|------|---------|------|
| Descriptor caching | Yes | Yes | Yes | Via bind groups |
| Pipeline caching | Yes | Yes | Yes | Yes |
| Opaque handles | Yes | Yes | Yes (RID) | Yes (Handle) |
| Per-frame pools | Yes | Yes | Yes | Yes |
| Buffer pooling | Partial | Yes | Yes | Yes |

---

## Implementation Order

1. **Phase 1**: DescriptorSetCache + unified DescriptorSetBuilder
2. **Phase 2**: PipelineCache + opaque handles
3. **Phase 3**: Per-frame command pools + buffer pool
4. **Phase 4**: Sampler cache + texture/view separation

---

## Key Principles

1. **Cache aggressively** - Descriptors, pipelines, samplers
2. **Pool resources** - Buffers, command pools per frame
3. **Hide internals** - Opaque handles, no Rc<RefCell<>>
4. **Unify abstractions** - One builder pattern, not many
5. **Batch operations** - Group submissions, minimize state changes
