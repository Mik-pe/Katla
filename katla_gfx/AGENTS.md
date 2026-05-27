# katla_gfx

## Cross-Backend Architecture

Two rendering backends, selected at runtime via `AnyRenderer`:
- **Vulkan** — via `ash`, all platforms (macOS uses MoltenVK)
- **Metal** — via `objc2-metal`, native macOS only (cfg-gated behind `target_os = "macos"`)

`GpuRenderer` is the backend-agnostic trait. Both `VulkanRenderer` and `MetalRenderer` implement it. `AnyRenderer` is an enum that dispatches dynamically.

Backend-specific code lives in `vulkan/` and `metal/`. The public API uses `GpuRenderer` trait methods only.

### When Adding New Features

1. Add the method to `GpuRenderer` trait first (with default no-op impl)
2. Implement for both `VulkanRenderer` and `MetalRenderer`
3. Add dispatch to `AnyRenderer` enum
4. If it involves render graph resources, extend `RenderGraphBackend` trait

## Render Graph

The render graph is generic over `GpuRenderer`. `FrameGraphBuilder` provides a fluent API for declaring passes and resources. `AnyFrameGraph` / `AnyFrame` provide runtime dispatch. Pass types (GeometryPass, ShadowPass, etc.) live in `render_graph/passes/`.

## Descriptor Set Layout (Vulkan-only)

Vulkan uses a **3-set descriptor layout**. Metal uses argument buffers instead.

- **Set 0** — Per-frame uniforms + per-object storage buffer array (indexed by `instance_index`)
- **Set 1** — Bindless texture array (up to 4096) + shared sampler
- **Set 2** — Optional skeletal animation joint matrices

For shader authors: access textures via `bindless_textures[texture_indices.x]`. Never use push constants.

## Image Barriers (Vulkan-only)

Use `ImageBarrier` helpers — never manually construct `vk::ImageMemoryBarrier`. The API uses explicit source layouts. Automatic stage/access mask deduction, Vulkan 1.3 sync2.

Before using `vk::` types, check for existing wrappers/helpers first.

## Feature Gating

The `validation` feature promotes internal modules (`barrier`, `sync`, `lighting`, pipeline types) from `pub(crate)` to `pub` for use in validation examples and benchmarks. Run with `--features validation`.
