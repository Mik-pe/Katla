# Backend-Agnostic Render Graph

## Current State

The render graph lives in `katla_gfx/src/render_graph/` and is tightly coupled to Vulkan:

| Coupling Point | Location | What It Does |
|---|---|---|
| `FrameGraph::execute()` takes `&mut VulkanRenderer` | `frame_graph.rs` | Drives transient texture creation, material compilation, storage buffer updates, pass execution |
| `Frame` holds `&mut VulkanRenderer` and `vk::CommandBuffer` | `frame/mod.rs` | Submits draw calls, inserts barriers, manages temp allocations |
| `TransientTexture` wraps `vk::Image`, `vk::ImageView`, `VkImageView`, `VulkanContext` | `transient_texture.rs` | Owns Vulkan image memory lifecycle |
| `GraphCompiler` / `ExecutionPlan` | `compiler.rs` | **Already backend-agnostic** — pure dependency analysis |
| `PassDesc`, `PassBuilder`, `FrameGraphBuilder` | `pass.rs`, `builder.rs` | Mostly backend-agnostic, but `ComputeFn` closure captures `CommandBuffer` (Vulkan type) |
| `ResourceState::to_vk_*()` methods | `resource.rs` | Convert to `vk::PipelineStageFlags` / `vk::AccessFlags` |
| Barriers (`frame/barriers.rs`) | Calls `ImageBarrier` helpers with `vk::` types directly | |
| Backend abstraction traits | `backend/traits.rs`, `backend/command.rs` | Already define `GpuBackend`, `GpuCommandBuffer`, `GpuRenderEncoder`, etc. but the render graph ignores them |

The `MetalFrameGraph` in `metal/metal_frame_graph.rs` is a separate, standalone implementation that is never used. It has a much simpler model (add passes by name, create transient textures, execute one named pass at a time) and no dependency analysis.

The `backend/` module already defines backend-agnostic traits (`GpuBackend`, `GpuCommandBuffer`, `GpuRenderEncoder`, etc.) that both Vulkan and Metal implement. The render graph simply doesn't use them.

## Strategy

The compiler (`GraphCompiler`) and the declarative graph structure (`FrameGraphBuilder`, `PassBuilder`, `PassDesc`, `GraphResourceDesc`) are already or nearly backend-agnostic. The work is in replacing the Vulkan-specific execution layer with code that goes through the existing backend abstraction traits.

### Three-Layer Model

```
 +-----------------------------------+
 |  Layer 1: Graph Structure         |
 |  (backend-agnostic, no GPU types) |
 |                                   |
 |  FrameGraphBuilder                |
 |  PassBuilder / InternalPassBuilder|
 |  PassDesc                         |
 |  GraphResourceDesc                |
 |  GraphCompiler / ExecutionPlan    |
 +-----------------------------------+
            |
            v
 +-----------------------------------+
 |  Layer 2: Backend Interface       |
 |  (trait-based, per-backend impl)  |
 |                                   |
 |  trait RenderGraphBackend         |
 |  trait TransientTextureBackend    |
 |  trait BarrierOps                 |
 +-----------------------------------+
            |
            v
 +-----------------------------------+
 |  Layer 3: Backend Implementation  |
 |  (Vulkan, Metal, future)          |
 |                                   |
 |  VulkanRenderGraphBackend         |
 |  MetalRenderGraphBackend          |
 +-----------------------------------+
```

**Layer 1** is pure data and dependency analysis — no GPU types, no allocations. Already ~90% there.

**Layer 2** is a thin trait layer that the render graph calls into for GPU-specific work (create textures, insert barriers, begin render passes). This extends the existing `GpuBackend` family of traits.

**Layer 3** implements Layer 2 for each backend. The Vulkan implementation wraps the existing `VulkanRenderer` logic. The Metal implementation maps to Metal's command encoding model.

---

## Plan

### Phase 1: Decouple PassDesc and ComputeFn

**Problem**: `ComputeFn` in `pass.rs` captures a Vulkan `CommandBuffer` type:

```rust
pub type ComputeFn = Box<
    dyn Fn(
        &mut Frame,
        &crate::vulkan::commandbuffer::CommandBuffer,  // <-- Vulkan type
        crate::handle::PipelineHandle,
    ) -> Result<(), RenderGraphError>,
>;
```

**Change**: Generic over the backend's command buffer type, or replace the typed closure with a backend-agnostic callback. Two options:

- **Option A (trait object)**: Define a `PassExecute` trait that the `Frame` calls, with backend-specific implementations providing the concrete command buffer. The closure becomes `Box<dyn PassExecute>` instead of a concrete `Fn`.
- **Option B (enum dispatch)**: Since the backend is known at compile time (feature-gated), use a `#[cfg]`-gated type alias:

```rust
#[cfg(feature = "vulkan")]
pub type ComputeFn = Box<dyn Fn(&mut Frame, &crate::vulkan::commandbuffer::CommandBuffer, PipelineHandle) -> Result<(), RenderGraphError>>;
#[cfg(feature = "metal")]
pub type ComputeFn = Box<dyn Fn(&mut Frame, &crate::metal::command_buffer::MetalCommandBuffer, PipelineHandle) -> Result<(), RenderGraphError>>;
```

Option A is cleaner for long-term maintainability. Option B is less invasive. Recommend **Option A**.

### Phase 2: Abstract TransientTexture

**Problem**: `TransientTexture` directly wraps `vk::Image`, `VkImageView`, `Rc<VulkanContext>`, `gpu_allocator::Allocation`.

**Change**: Introduce a `TransientTextureBackend` trait (or a `GpuBackend`-associated type):

```rust
/// Backend-specific transient texture storage.
pub trait TransientTextureOps: Sized {
    /// Current tracked resource state (semantic usage).
    fn state(&self) -> ResourceState;
    /// Update tracked state after transition.
    fn set_state(&self, state: ResourceState);
}

/// Stored in FrameGraph, one per transient resource per frame.
pub struct TransientTexture<B: GpuBackend> {
    inner: B::TransientTexture,
    bindless_slot: Option<u32>,
}
```

Vulkan implements this with its current `TransientTexture` (renamed to `VulkanTransientTexture`). Metal implements it with `MetalTransientTexture` (already exists in `metal/metal_frame_graph.rs`).

`ResourceState` becomes a backend-agnostic enum (already is — just remove the `to_vk_*()` methods and move those to the Vulkan backend layer).

### Phase 3: Abstract Barrier Insertion

**Problem**: `frame/barriers.rs` calls Vulkan's `ImageBarrier::transition()` directly with `vk::ImageLayout`, `vk::PipelineStageFlags`, etc.

**Change**: Define a `BarrierOps` trait on the backend:

```rust
pub trait BarrierOps {
    /// Transition a transient texture to a new resource state.
    fn transition_texture(
        &self,
        cmd: &mut impl GpuCommandBuffer<Self>,
        texture: &mut impl TransientTextureOps,
        from: ResourceState,
        to: ResourceState,
    );

    /// Insert a depth-render-pass sync barrier.
    fn depth_render_pass_sync(
        &self,
        cmd: &mut impl GpuCommandBuffer<Self>,
        depth_image: &Self::Image,
    );
}
```

Vulkan maps `ResourceState` -> `vk::ImageLayout` + stage/access flags internally (move the current `to_vk_*()` logic there). Metal does the same with its resource state model (MTLBarrier / MTLFence / explicit `ResourceState` tracking).

The barrier logic in `frame/barriers.rs` becomes parameterized over `BarrierOps` instead of calling `ImageBarrier` directly:

```rust
fn insert_barriers(
    &mut self,
    backend: &impl BarrierOps,
    cmd: &mut impl GpuCommandBuffer<B>,
    pass_index: usize,
) -> Result<(), RenderGraphError> { ... }
```

### Phase 4: Abstract Frame Execution

**Problem**: `Frame` holds `&mut VulkanRenderer` and dispatches pass execution through Vulkan-specific code paths (`execute_graphics_pass`, `execute_fullscreen_pass`, `execute_shadow_pass`, etc.).

**Change**: Introduce a `RenderGraphBackend` trait:

```rust
pub trait RenderGraphBackend: GpuBackend {
    type Renderer;

    /// Create a transient texture for a given resource descriptor.
    fn create_transient_texture(
        renderer: &mut Self::Renderer,
        desc: &GraphResourceDesc,
    ) -> Result<Self::TransientTexture, RenderGraphError>;

    /// Destroy a transient texture.
    fn destroy_transient_texture(renderer: &mut Self::Renderer, texture: Self::TransientTexture);

    /// Execute a single pass.
    fn execute_pass(
        renderer: &mut Self::Renderer,
        cmd: &mut Self::CommandBuffer,
        pass: &PassDesc,
        pass_data: &PassExecutionData,
        resources: &ResourceLookup<Self>,
    ) -> Result<(), RenderGraphError>;

    /// Insert pre-pass barriers.
    fn insert_barriers(
        renderer: &mut Self::Renderer,
        cmd: &mut Self::CommandBuffer,
        pass: &PassDesc,
        resources: &ResourceLookup<Self>,
    ) -> Result<(), RenderGraphError>;

    /// Insert post-pass barriers.
    fn insert_post_barriers(
        renderer: &mut Self::Renderer,
        cmd: &mut Self::CommandBuffer,
        pass: &PassDesc,
        resources: &ResourceLookup<Self>,
    ) -> Result<(), RenderGraphError>;

    /// Current frame index (for per-frame resource double-buffering).
    fn current_frame(renderer: &Self::Renderer) -> usize;

    /// Register a texture view with the bindless system, return slot.
    fn register_bindless_texture(
        renderer: &mut Self::Renderer,
        view: &Self::ImageView,
    ) -> Result<u32, RenderGraphError>;

    /// Update an existing bindless texture slot with a new view.
    fn update_bindless_texture(
        renderer: &mut Self::Renderer,
        slot: u32,
        view: &Self::ImageView,
    ) -> Result<(), RenderGraphError>;
}
```

The `Frame` struct becomes generic:

```rust
pub struct Frame<'a, B: RenderGraphBackend> {
    graph: &'a FrameGraph<B>,
    renderer: &'a mut B::Renderer,
    pending: HashMap<usize, PassExecutionData>,
    backbuffer_written: bool,
    depth_buffer_written: bool,
}
```

### Phase 5: Make FrameGraph Generic

**Problem**: `FrameGraph` stores `Vec<HashMap<ResourceId, TransientTexture>>` (Vulkan-specific type) and references `VulkanRenderer` in `execute()` and `initialize_transient_textures()`.

**Change**:

```rust
pub struct FrameGraph<B: RenderGraphBackend> {
    // Layer 1 (unchanged):
    passes: Vec<PassDesc>,
    resources: Vec<GraphResourceDesc>,
    resource_by_name: HashMap<String, ResourceId>,
    pass_names: HashMap<String, usize>,
    execution_plan: Option<ExecutionPlan>,
    compiled: bool,
    transient_resources: Vec<GraphResourceDesc>,

    // Layer 2 (generic):
    transient_textures: Vec<HashMap<ResourceId, B::TransientTexture>>,
    params: FrameParams,
    ldr_texture_base_index: Option<u32>,
}
```

`execute()` signature changes to:

```rust
pub(crate) fn execute(
    &mut self,
    renderer: &mut B::Renderer,
    image_index: u32,
    f: impl FnOnce(&mut Frame<'_, B>),
) -> Result<(), RenderGraphError>
```

All the per-pass dispatch logic (shadow, geometry, fullscreen, compositing, etc.) moves into `B::execute_pass()`. The render graph core only knows about pass ordering, resource tracking, and barrier scheduling.

### Phase 6: Backend-Specific Pass Execution

The Vulkan `execute_pass()` implementation encapsulates the current `execute_*_pass` methods from `frame/`:

```
frame/graphics_pass.rs   -> VulkanRenderGraphBackend::execute_pass(PassKind::Geometry, ...)
frame/shadow_pass.rs     -> VulkanRenderGraphBackend::execute_pass(PassKind::Shadow, ...)
frame/fullscreen_pass.rs -> VulkanRenderGraphBackend::execute_pass(PassKind::Fullscreen, ...)
frame/compositing.rs     -> VulkanRenderGraphBackend::execute_pass(PassKind::Compositing, ...)
frame/particle_rendering.rs -> VulkanRenderGraphBackend::execute_pass(PassKind::Particles, ...)
frame/outline_pass.rs    -> VulkanRenderGraphBackend::execute_pass(PassKind::Outline, ...)
...
```

The Metal backend provides its own `execute_pass()` mapping. Each pass kind is dispatched identically — the render graph just calls `backend.execute_pass(...)` — but the backend interprets it through its own pipeline model.

### Phase 7: Remove MetalFrameGraph

Once `FrameGraph<MetalBackend>` works, `metal/metal_frame_graph.rs` becomes redundant and should be deleted entirely per the project convention (no hybrid implementations).

---

## Dependency Graph

```
Phase 1 (ComputeFn decoupling)
    |
    v
Phase 2 (TransientTexture abstraction)
    |
    v
Phase 3 (Barrier abstraction)  ----> Phase 5 (FrameGraph<B> generic)
    |                                       |
    v                                       v
Phase 4 (RenderGraphBackend trait) --> Phase 6 (Vulkan impl)
                                            |
                                            v
                                    Phase 7 (Delete MetalFrameGraph)
```

Phases 1-3 can be done incrementally without breaking the Vulkan path. Each phase should compile and pass tests before moving to the next. Phase 4 is the inflection point where `Frame` becomes generic. Phases 5-6 follow naturally.

---

## Files Changed (Per Phase)

| Phase | Files Modified/Created | Risk |
|---|---|---|
| 1 | `render_graph/pass.rs`, `render_graph/builder.rs` | Low — change one type alias |
| 2 | New `render_graph/transient_ops.rs`, `render_graph/transient_texture.rs`, `metal/transient_texture.rs` | Medium — core data structure |
| 3 | New `render_graph/barrier_ops.rs`, `render_graph/frame/barriers.rs`, `render_graph/resource.rs` | Medium — barrier logic is safety-critical |
| 4 | New `render_graph/backend_trait.rs`, `render_graph/frame/mod.rs` | High — `Frame` is the hot path |
| 5 | `render_graph/frame_graph.rs` | High — core struct becomes generic |
| 6 | New `render_graph/vulkan_backend.rs`, move `frame/*.rs` logic into it | Medium — refactoring, not new logic |
| 6 | New `render_graph/metal_backend.rs` | High — new Metal execution path |
| 7 | Delete `metal/metal_frame_graph.rs` | Low — removing dead code |

---

## What Stays Backend-Agnostic

These modules already work without GPU types and need no changes:

- `render_graph/compiler.rs` — dependency analysis, topological sort, cycle detection, DAG construction
- `render_graph/handles.rs` — `PassId`, `ResourceId`
- `render_graph/builder.rs` — `PassBuilder` trait, `InternalPassBuilder` (except `ComputeFn` in Phase 1)
- `render_graph/resource.rs` — `GraphResourceDesc`, `GraphResourceHandle` (move `to_vk_*()` to Vulkan layer)
- `render_graph/error.rs` — error types
- `render_graph/passes/` — pass templates (`GeometryPass`, `FullscreenPass`, etc.) — they produce `PassDesc` and `PassData`, not GPU commands

## What Becomes Backend-Specific

These contain or will contain Vulkan-specific logic and need per-backend implementations:

- Transient texture lifecycle (create, destroy, layout tracking)
- Barrier insertion (image layout transitions, memory synchronization)
- Pass execution (render pass setup, draw calls, compute dispatch)
- Bindless texture registration
- Storage buffer management (tonemap params, overlay params, per-object data)
- Swapchain/backbuffer management

---

## Scaling to DirectX 12

The 7-phase plan was designed around Vulkan and Metal. Adding DX12 reveals three friction points that need addressing. The good news is the fix is straightforward — it's about making the abstraction slightly less Vulkan-shaped at the trait boundary.

### Friction Point 1: Barrier Semantics

Vulkan and Metal have similar barrier models: transition a resource between named states. DX12 is fundamentally different.

| Concept | Vulkan | Metal | DX12 |
|---|---|---|---|
| Barrier unit | `vk::ImageMemoryBarrier` (per-image) | Implicit (encoder boundaries) | `D3D12_RESOURCE_BARRIER` (transition, aliasing, UAV) |
| Layout tracking | Explicit `vk::ImageLayout` enum | Not needed (GPU manages) | `D3D12_RESOURCE_STATES` bitmask (flags are combinable) |
| Split barriers | Optional `vk::DependencyInfo` | N/A | First-class (`BEGIN`/`END` split barriers) |
| Command buffer scope | Inside cmd buffer recording | Encoder scope | Inside command list recording |

**Problem with Phase 3 as proposed**: The `BarrierOps` trait implicitly assumes "transition a single texture between two states." This matches Vulkan one-to-one. It also works for Metal (Metal can just no-op or insert an MTLBarrier). But DX12 needs richer barrier batch submission and split barrier support.

**Fix**: Replace the per-texture `transition_texture()` with a batched barrier API:

```rust
pub trait BarrierOps<B: GpuBackend> {
    /// Begin accumulating barriers for a pass transition.
    fn begin_barrier_batch(&self) -> BarrierBatch<B>;

    /// Transition a texture. Batched — not flushed until submit.
    fn transition_texture(
        batch: &mut BarrierBatch<B>,
        texture: &B::Image,
        from: ResourceState,
        to: ResourceState,
    );

    /// Submit all accumulated barriers.
    fn submit_barriers(batch: BarrierBatch<B>, cmd: &mut B::CommandBuffer);
}
```

This lets Vulkan batch into a single `vkCmdPipelineBarrier2`, DX12 batch into a single `ResourceBarrier()` call with an array, and Metal either no-op or insert a single barrier at the encoder boundary. The `BarrierBatch` type is backend-specific (it's an associated type on the trait or on `GpuBackend`):

```rust
pub trait GpuBackend {
    // ... existing types ...
    type BarrierBatch: BarrierBatch<Self>;
}
```

Vulkan: `BarrierBatch` accumulates `vk::ImageMemoryBarrier2` entries. Metal: `BarrierBatch` is `()` (no-op). DX12: `BarrierBatch` accumulates `D3D12_RESOURCE_BARRIER` entries.

### Friction Point 2: Descriptor Management

Vulkan uses explicit descriptor sets (Set 0/1/2 layout, bindless texture array). Metal uses argument buffers. DX12 uses root signatures with descriptor tables, UAV counters, and static samplers.

**Problem**: The `RenderGraphBackend` trait in Phase 4 proposes `register_bindless_texture()` and `update_bindless_texture()`, which directly model Vulkan's bindless descriptor model. DX12 descriptor management is structurally different (heap-based, with visibility masks and static sampler slots).

**Fix**: Generalize the descriptor interface to a resource binding model:

```rust
pub trait RenderGraphBackend: GpuBackend {
    // ... other methods ...

    /// Register a texture for shader access across all frames.
    /// Returns an opaque handle; backend decides representation.
    fn register_shader_texture(
        renderer: &mut Self::Renderer,
        view: &Self::ImageView,
    ) -> Result<TextureSlot, RenderGraphError>;

    /// Update a previously registered texture.
    fn update_shader_texture(
        renderer: &mut Self::Renderer,
        slot: TextureSlot,
        view: &Self::ImageView,
    ) -> Result<(), RenderGraphError>;
}

/// Opaque texture slot. Backend decides the representation:
/// - Vulkan: bindless descriptor index (u32)
/// - Metal: argument buffer index (u32)
/// - DX12: descriptor heap offset (u64)
#[derive(Clone, Copy, Debug)]
pub struct TextureSlot {
    /// Backend-specific slot data.
    data: u64,
}
```

The key insight is to not expose what the slot *is* — just that it's a thing you can update. Each pass template reads the slot and passes it to the shader in a backend-specific way (push constants on Vulkan, argument buffer on Metal, root descriptor table on DX12).

### Friction Point 3: ShaderStages Needs DX12-Aware Extensions

The current `ShaderStages` struct has `vertex`, `fragment`, `compute`. DX12 distinguishes between visibility in the root signature (`VS`, `PS`, `GS`, `HS`, `DS`, `CS`, `ALL`). The naming is close but not identical (Metal calls it "pixel," not "fragment").

**Fix**: Rename `fragment` to `pixel` or use a bitmask with backend-agnostic names. Since `ShaderStages` is already in `backend/command.rs` (the backend abstraction layer), this is a natural place:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ShaderStages {
    pub vertex: bool,
    pub pixel: bool,    // "fragment" in Vulkan/GLSL, "pixel" in Metal/HLSL
    pub compute: bool,
    pub geometry: bool, // DX12 geometry shader
    pub hull: bool,     // DX12 tessellation control
    pub domain: bool,   // DX12 tessellation evaluation
}
```

Or, simpler: keep the current struct and let each backend map the fields it understands. A DX12 backend would just ignore `fragment` (use `pixel` internally) or we alias:

```rust
impl ShaderStages {
    pub const FRAGMENT: Self = Self::PIXEL; // alias for Vulkan/GLSL naming
}
```

This is a minor naming concern, not an architectural one.

### Updated Architecture: Revised Three-Layer Model

```
 +-----------------------------------------+
 |  Layer 1: Graph Structure               |
 |  (no GPU types, no backend awareness)   |
 |                                         |
 |  FrameGraphBuilder, PassBuilder, etc.   |
 |  GraphCompiler / ExecutionPlan          |
 |  GraphResourceDesc, ResourceState       |
 +-----------------------------------------+
              |
              v
 +-----------------------------------------+
 |  Layer 2: Backend Interface             |
 |  (trait-based, batched barriers,        |
 |   opaque descriptor slots)              |
 |                                         |
 |  GpuBackend (existing, extended with    |
 |    BarrierBatch, TextureSlot)           |
 |  BarrierOps (batched)                   |
 |  RenderGraphBackend (pass execution,    |
 |    transient texture lifecycle)         |
 +-----------------------------------------+
              |
              v
 +-----------------------------------------+
 |  Layer 3: Backend Implementations       |
 |                                         |
 |  VulkanBackend  | MetalBackend | DX12   |
 +-----------------------------------------+
```

The key change from the original plan: **Layer 2 uses batched barriers and opaque resource slots instead of per-call transitions and explicit bindless indices.** This costs nothing on Vulkan/Metal (just accumulate a single entry and flush) while making DX12 natural.

### Updated Phase Impact

The original Phases 1-2 (ComputeFn, TransientTexture) are unchanged. The changes affect:

| Phase | What Changes |
|---|---|
| Phase 3 (Barriers) | `BarrierOps` becomes batched, not per-texture. `BarrierBatch` added to `GpuBackend`. |
| Phase 4 (RenderGraphBackend trait) | `register_bindless_texture` -> `register_shader_texture` returning opaque `TextureSlot`. Add `ShaderStages` DX12 fields. |
| Phase 5 (FrameGraph generic) | No change — `FrameGraph<B>` is the same either way. |
| Phase 6 (Backend impls) | Three impls instead of two. Vulkan wraps existing logic. Metal maps through encoder model. DX12 maps through command lists + resource barriers. |
| New: Phase 6.5 | DX12 backend crate (`katla_dx12` or feature gate) implementing `GpuBackend`, `BarrierOps`, `RenderGraphBackend`. |

### What DX12 Backend Would Need to Implement

1. **`GpuBackend` associated types**: `Image = ComPtr<ID3D12Resource>`, `ImageView = D3D12_CPU_DESCRIPTOR_HANDLE`, `CommandBuffer = ComPtr<ID3D12GraphicsCommandList>`, etc.
2. **`BarrierOps`**: Accumulate `D3D12_RESOURCE_BARRIER` entries, flush with `ResourceBarrier()`. Split barriers via `D3D12_RESOURCE_BARRIER_FLAG_BEGIN_ONLY` / `END_ONLY`.
3. **`RenderGraphBackend::execute_pass`**: Map `PassKind` to `OMSetRenderTargets`, `DrawInstanced`, `Dispatch`, etc.
4. **`TransientTextureOps`**: Track `D3D12_RESOURCE_STATES` bitmask. Create committed resources with `CreateCommittedResource`.
5. **`register_shader_texture`**: Create SRV descriptor in shader-visible heap, return heap offset as `TextureSlot`.

None of this requires changes to Layer 1. The graph structure and compiler remain completely backend-agnostic.

### Verdict

The original plan scales to DX12 with three targeted adjustments:

1. **Batched barriers** instead of per-texture transitions (Phase 3)
2. **Opaque `TextureSlot`** instead of bare `u32` bindless index (Phase 4)
3. **`ShaderStages` extended** with DX12 shader stages (minor naming)

These changes are small, backwards-compatible with the Vulkan+Metal path, and make the architecture cleanly support N backends. The graph structure (Layer 1) and compiler require zero changes for any future backend.
