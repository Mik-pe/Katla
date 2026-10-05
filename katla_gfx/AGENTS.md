# katla_gfx

Read [graphics ownership](../docs/graphics_core.md) for core API changes.
For native Metal work, use [backend contracts](../docs/metal_backend.md) and
[frame-slot ownership](../docs/metal4_frame_slots.md); for graph hazards, use
[synchronization](../docs/render_graph_synchronization.md).

## Cross-Backend Architecture

Two rendering backends, selected at runtime via `AnyRenderer`:
- **Vulkan** — via `ash`, all platforms (macOS uses MoltenVK)
- **Metal** — via `objc2-metal`, native macOS only (cfg-gated behind `target_os = "macos"`)

`GpuRenderer` is the backend-agnostic trait. Both `VulkanRenderer` and `MetalRenderer` implement it. `AnyRenderer` is an enum that dispatches dynamically.

Backend-specific code lives in `vulkan/` and `metal/`. Public resource, frame, submission and graph operations use backend-neutral types.

### When Adding New Features

1. Keep `GpuRenderer` limited to resource creation, capabilities, frame ownership, submission, graph execution and generic readback.
2. Compose scene and editor features in the application with ordinary resources, explicit bindings and graph passes. Font atlases, viewport policy, picking selection, outlines, shadows and postprocessing must not become mandatory core methods.
3. Implement new core operations for both `VulkanRenderer` and `MetalRenderer`, with explicit `AnyRenderer` dispatch. Required operations must not have default no-op implementations.
4. Extend `RenderGraphBackend` only for generic graph allocation, synchronization or execution needs. Never introduce a second feature-forwarding trait.
5. A core/headless graph must initialize without installing scene or editor resources and pipelines.

## Render Graph

The render graph is generic over `GpuRenderer`. `FrameGraphBuilder` provides a fluent API for declaring passes and resources. `AnyFrameGraph` / `AnyFrame` provide runtime dispatch. Pass types (GeometryPass, ShadowPass, etc.) live in `render_graph/passes/`.

## Descriptor Set Layout (Vulkan-only)

Vulkan derives descriptor layouts from the selected shader entry points. Metal 4 uses reflected per-stage argument tables, immutable bindless resource-ID buffers, and submission-owned residency snapshots. Bindings declare the actual buffer usage, stages and ranges in the graph; resource groups are shader contracts rather than a fixed number of engine descriptor sets.

Katla scene shaders use groups for frame/object data, bindless textures, skeletal joints, tiled lights and shadows. Custom shaders declare their own bindings through ordinary pass packets. For scene shader authors, access textures via `bindless_textures[texture_indices.x]`. Never use push constants.

## Image Barriers (Vulkan-only)

Graph synchronization comes from the compiled synchronization plan. Vulkan consumes that plan with explicit subresource layouts and Vulkan 1.3 sync2. Use `ImageBarrier` helpers for non-graph operations; never manually construct `vk::ImageMemoryBarrier` or add a second graph barrier path.

Before using `vk::` types, check for existing wrappers/helpers first.

## Feature Gating

The `validation` feature exposes internal barrier, synchronization and pipeline types for validation examples and benchmarks. Run with `--features validation`.
