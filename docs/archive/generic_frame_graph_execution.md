# Generic Frame Graph Execution

## Problem

The frame graph execution layer (`render_graph/frame/`) is hardcoded to Vulkan:

- `Frame<'a>` holds `&'a mut VulkanRenderer`, `vk::Buffer`, `gpu_allocator::vulkan::Allocation`
- All 14 files in `render_graph/frame/` use `ash::vk::*` types directly
- `PassDesc::compute_fn` is `#[cfg(feature = "vulkan")]` gated
- `Application::frame_graph` is `#[cfg(feature = "vulkan")]` gated
- Metal has a parallel `MetalFrameGraph` that is completely disconnected

The `GpuBackend` trait (`backend/traits.rs`) already provides a backend-agnostic
abstraction for command buffers, encoders, pipelines, buffers, images, etc. Both
Metal and Vulkan implement it. The frame graph execution should go through these
traits, not raw `ash::vk` types.

## Goal

One frame graph execution path shared by all backends. The application code
(build_frame_graph, submit draw lists, render_frame) is identical regardless of
whether Vulkan or Metal is the active backend.

## Architecture

### Current State

```
Application (vulkan only: frame_graph, pass_ids, build_frame_graph)
  └─ VulkanRenderer::render(&mut frame_graph, |frame| { ... })
       └─ FrameGraph<VulkanRenderer>::execute()
            └─ Frame<'a>  ← hardcoded to VulkanRenderer, vk::* types
                 └─ render_graph/frame/*.rs  ← 14 files, all use ash::vk

Application (metal only: no frame_graph at all)
  └─ MetalRenderer::render_frame()  ← hardcoded 3-pass method
       └─ MetalFrameGraph  ← separate, unused in production
```

### Target State

```
Application (shared: frame_graph, pass_ids, build_frame_graph)
  └─ GpuRenderer::render(&mut frame_graph, |frame| { ... })
       └─ FrameGraph<B>::execute()
            └─ Frame<'a, B: RenderGraphBackend>
                 └─ render_graph/frame/*.rs  ← use GpuBackend traits
```

## Steps

### Step 1: Make Frame Generic Over Backend

Replace `Frame<'a>` with `Frame<'a, B: RenderGraphBackend>`.

Key changes:
- `renderer: &'a mut VulkanRenderer` becomes `renderer: &'a mut B`
- `temporary_buffers: Vec<(vk::Buffer, Allocation)>` becomes backend-agnostic
  (or removed — the Frame shouldn't manage GPU memory directly)
- `resolve_color_attachment` returns `ColorAttachmentInfo<B>` instead of
  `vk::RenderingAttachmentInfo`
- `resolve_depth_attachment` returns `DepthAttachmentInfo<B>` instead of
  `vk::RenderingAttachmentInfo`
- `image_index: u32` stays (both backends have a concept of swapchain image index)

The `ColorAttachmentInfo<B>`, `DepthAttachmentInfo<B>`, and `RenderPassInfo<B>`
types already exist in `backend/command.rs` and are parameterized over `GpuBackend`.

### Step 2: Backend-Agnostic Pass Execution

Replace all Vulkan command buffer calls with `GpuBackend` trait methods:

| Vulkan (current)                        | GpuBackend (target)                          |
|-----------------------------------------|----------------------------------------------|
| `cmd.begin_rendering(&[vk_att], ...)`   | `encoder = cmd.begin_render_pass(info)`      |
| `cmd.end_rendering()`                   | `encoder.end_encoding()`                     |
| `vk::cmd_bind_pipeline(cmd, GRAPHICS)`  | `encoder.bind_graphics_pipeline(&pipeline)`  |
| `cmd.bind_descriptor_sets(layout, ...)` | `encoder.bind_storage_buffer(buf, idx, ...)` |
| `cmd.draw_array(3, 1, 0, 0)`           | `encoder.draw(3, 1, 0, 0)`                   |
| `cmd.bind_vertex_buffer(buf, offset)`   | `encoder.bind_vertex_buffer(buf, offset, 0)` |
| `cmd.bind_index_buffer(buf, offset)`    | `encoder.bind_index_buffer(buf, offset, U32)`|
| `cmd.set_viewport(&[vp])`              | `encoder.set_viewport(x, y, w, h, near, far)`|
| `cmd.set_scissor(&[rect])`             | `encoder.set_scissor(x, y, w, h)`            |

Each pass executor (shadow, geometry, fullscreen, etc.) restructured to:
1. Resolve attachments into `RenderPassInfo<B>`
2. Begin render pass → get encoder
3. Record draw calls via `GpuRenderEncoder<B>` methods
4. End encoding

### Step 3: Make PassDesc::compute_fn Backend-Agnostic

Currently:
```rust
#[cfg(feature = "vulkan")]
pub type ComputeFn = Box<dyn Fn(
    &mut Frame,
    &crate::vulkan::commandbuffer::CommandBuffer,
    crate::handle::PipelineHandle,
) -> Result<(), RenderGraphError>>;
```

Replace with a generic callback that uses GpuBackend types:
```rust
pub type ComputeFn<B> = Box<dyn Fn(
    &mut Frame<'_, B>,
    &<B as GpuBackend>::CommandBuffer,
    crate::handle::PipelineHandle,
) -> Result<(), RenderGraphError>>;
```

Or alternatively, use a trait-based approach if boxing becomes problematic.

### Step 4: Barriers as Backend Hook

Vulkan needs explicit image layout transitions between passes. Metal doesn't.

Add optional methods to `RenderGraphBackend`:
```rust
fn pre_pass_barrier(
    cmd: &mut Self::CommandBuffer,
    texture: &Self::TransientTexture,
    from: ResourceState,
    to: ResourceState,
) {}

fn post_pass_barrier(
    cmd: &mut Self::CommandBuffer,
    texture: &Self::TransientTexture,
    from: ResourceState,
    to: ResourceState,
) {}
```

Vulkan implements these with `ImageBarrier` helpers. Metal uses no-op defaults.

### Step 5: Ungate Application

Remove `#[cfg(feature = "vulkan")]` from:
- `Application::frame_graph` field
- `Application::pass_ids` field
- `Application::gltf_cache` field (or gate independently)
- `build_frame_graph()` method
- `render_frame()` Vulkan-specific path merges with Metal path
- `init_vulkan()` → `init_rendering()` (backend-agnostic)
- `frame_loop.rs` particle/animation/frame_graph code
- `picking.rs`

### Step 6: Implement MetalRenderer::render()

Add a `render(&mut self, frame_graph, f)` method to `MetalRenderer` (via `GpuRenderer`
trait) that mirrors Vulkan's flow:
1. Acquire next drawable (begin_frame)
2. Create command buffer
3. Call `frame_graph.execute(self, drawable_index, |frame| { f(frame) })`
4. Present drawable (end_frame)

The existing `MetalRenderer::render_frame()` hardcoded method is replaced entirely.

### Step 7: Remove MetalFrameGraph

The separate `MetalFrameGraph` in `metal/metal_frame_graph.rs` becomes redundant.
Remove it once Metal flows through the generic `FrameGraph<MetalRenderer>`.

## File Impact

### Modified (render_graph/frame/ — all become generic)
- `mod.rs` — Frame struct becomes `Frame<'a, B: RenderGraphBackend>`
- `barriers.rs` — Backend hook delegation
- `graphics_pass.rs` — Use GpuRenderEncoder
- `shadow_pass.rs` — Use GpuRenderEncoder
- `depth_prepass.rs` — Use GpuRenderEncoder
- `outline_pass.rs` — Use GpuRenderEncoder
- `particle_rendering.rs` — Use GpuRenderEncoder
- `compositing.rs` — Use GpuRenderEncoder
- `ui_rendering.rs` — Use GpuRenderEncoder
- `draw_calls.rs` — Generic draw list execution
- `draw_helpers.rs` — Generic draw helpers
- `object_id_pass.rs` — Use GpuRenderEncoder

### Modified (render_graph/)
- `pass.rs` — Generic ComputeFn
- `frame_graph.rs` — Generic execute(), ungate Frame pub use
- `mod.rs` — Ungate frame module

### Modified (backend/)
- `command.rs` — May need minor additions (e.g., stencil ops on GpuRenderEncoder)
- `traits.rs` — May need barrier hook on GpuBackend or RenderGraphBackend

### Modified (renderer/)
- `gpu_renderer.rs` — Add render() method to GpuRenderer trait
- `mod.rs` — VulkanRenderer::render() via GpuRenderer

### Modified (katla_app/)
- `mod.rs` — Ungate frame_graph, pass_ids
- `builder.rs` — Ungate build_frame_graph, make backend-agnostic
- `init.rs` — Ungate init_vulkan → init_rendering
- `renderer.rs` — Merge Vulkan/Metal render_frame paths
- `frame_loop.rs` — Ungate frame_graph usage
- `editor_methods.rs` — Ungate frame_graph usage
- `picking.rs` — Ungate or provide Metal fallback

### Removed
- `metal/metal_frame_graph.rs` — Replaced by generic FrameGraph

## Implementation Order

1. Frame generic + resolve_attachment refactor (steps 1-2)
2. PassDesc compute_fn generic (step 3)
3. Barrier hook (step 4)
4. Ungate Application (step 5)
5. MetalRenderer::render (step 6)
6. Remove MetalFrameGraph (step 7)
7. Build, test, run `cargo run -- -s`

Each step should compile and pass tests independently before moving to the next.
