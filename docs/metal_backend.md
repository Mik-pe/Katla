# Metal Backend Architecture

The native Metal backend of `katla_gfx` as it exists today. This document replaced the pre-implementation plan (`archive/metal_backend_implementation.md`) in August 2026; every module and type named here is verified against the tree.

---

## 1. Backend Landscape

`katla_gfx` is a cross-backend rendering library:

- **Vulkan** — via `ash` + `ash-window`, all platforms.
- **Metal** — via `objc2-metal`, compiled on macOS only (`cfg(target_os = "macos")` in `katla_gfx/src/lib.rs`).

Selection is explicit, never implicit:

- `AnyRenderer::new_metal(...)` / `AnyRenderer::new_vulkan(...)` for runtime selection through the `GpuRenderer` trait.
- `MetalRenderer` / `VulkanRenderer` directly for compile-time commitment.

Backend-neutral contracts live in `katla_gfx/src/backend/traits.rs` (`GpuBackend`, `GpuContext<B>`) and `katla_gfx/src/renderer.rs` (`GpuRenderer`: pipelines, materials, frame submission).

**macOS 26 policy** (AGENTS.md, binding): CI uses exactly one explicit Apple Silicon runner, `macos-26`. No `macos-latest`, no older-generation compatibility jobs. When Katla adopts a newer macOS generation, `macos-26` is replaced directly and `docs/ci.md` updates in the same change.

## 2. Module Ownership

All Metal code is `pub(crate)` under `katla_gfx/src/metal/` (declared in `mod.rs`), behind `cfg(target_os = "macos")`. Grouped by responsibility:

**Device and context**
- `context.rs` — `MetalContext`: default device via `MTLCreateSystemDefaultDevice`, command queue, depth/stencil state factory, headless initialization (`init_headless_with_size`, offscreen `CAMetalLayer`).

**Command path** (pre-Metal-4 `MTLCommandQueue`/encoder model; see [#54](https://github.com/Mik-pe/Katla/issues/54) for the clean-cut migration)
- `command_buffer.rs` — `MetalCommandBuffer`, submit with capture-free static completion handler.
- `render_encoder.rs`, `compute_encoder.rs`, `blit_encoder.rs` — encoders; deliberately `!Send`/`!Sync` (single-threaded encoding, enforced by const assertions, [#57](https://github.com/Mik-pe/Katla/issues/57)).

**Frame pipeline**
- `frame_lifecycle.rs` — `wait_for_frame` / `begin_frame` / `end_frame`.
- `execution_plan.rs` — `MetalExecutionPlan`: ordered executable pass records compiled from the backend-neutral render graph. Metal consumes these records directly; it does not rebuild topology.
- `frame_render.rs` — record-stream execution; `validate_frame_submissions` runs pure plan/data contract checks **before any encoder is created** (unknown pass submissions, multi-draw-list UI passes, missing depth target → typed `RendererError`, drawable dropped so no partial frame can present).
- `render_targets.rs`, `depth_prepass.rs`, `draw_helpers.rs` — target management and shared draw encoding.

**Resources**
- `buffer.rs`, `texture.rs`, `sampler.rs`, `format.rs` — native resource wrappers and format conversion.
- `metal_transient_texture.rs` — render-graph transient textures.

**Services**
- `texture_upload.rs` — `TextureUploadQueue`: staged uploads; initial data never lives in a Shared texture (pooled staging → blit → Private, retired after the consuming submission). See [#58](https://github.com/Mik-pe/Katla/issues/58).
- `pipeline_archive.rs` — `MetalPipelineArchive`: persistent `MTLBinaryArchive` + metadata sidecar at `~/Library/Caches/dev.ravboet.katla/pipelines/` with keyed invalidation (`ArchiveRejection`: Absent / MetadataMismatch / Corrupt) and `PipelineCacheStats` observability. See [#53](https://github.com/Mik-pe/Katla/issues/53).
- `argument_buffer.rs` — bindless argument buffer; passes declare explicit residency. See [#55](https://github.com/Mik-pe/Katla/issues/55).
- `timestamp_queries.rs`, `diagnostics.rs` — GPU timing and `GpuDiagnosticsMode` (validation/release command-buffer diagnostics, deterministic encoder labels, `GpuExecutionFailure.encoders` attached to renderer errors).

**Materials and shaders**
- `shader.rs` — WGSL → naga front-end → validation → naga MSL back-end with per-profile binding maps (`katla_msl_options`, `ShaderProfile` variants such as `ShadowSkinned`). **No SPIR-V, no SPIRV-Cross, no MoltenVK anywhere on the Metal path.**
- `material_api.rs`, `pipeline.rs`, `init_pipelines.rs` — material compilation and pipeline state creation.
- `mesh_api.rs`, `skeleton_api.rs`, `texture_api.rs`, `viewport_api.rs` — resource upload/management APIs.

**Render passes**
- `shadow.rs` — cascaded shadow maps (shared cascade data with Vulkan).
- `light_culling.rs` — Forward+ light culling.
- `outline.rs`, `picking.rs` (object-ID pass), `ui_renderer.rs`, `font_atlas.rs`, `animation.rs`.
- `particle.rs` — WIP, `#[cfg(test)]`-gated; not yet wired into the Metal render graph.

**Application and surface**
- `surface.rs` — `MetalSurface`: window drawable ownership. **Thread-affinity model**: the surface, its current drawable, and all layer mutations (acquire, present, resize, attachment) are confined to the main thread that owns the backing `NSView`; `MetalSurface` is `!Send`/`!Sync` by const assertion. Device/queue and immutable pipeline state keep audited `unsafe impl Send`/`Sync` with SAFETY comments citing Apple guarantees. Headless rendering never constructs a surface (offscreen layer owned by the context).
- `metal_renderer.rs` — `MetalRenderer`: the `GpuRenderer` implementation tying it all together.
- `sync.rs` — fence/semaphore equivalents for the current single-slot frame model.

## 3. Frame Lifecycle

1. `begin_frame` — acquire drawable (windowed: `MetalSurface`; headless: offscreen texture set as current drawable).
2. Application builds/updates the frame graph (application-owned topology; the editor pipeline is one preset among possible graphs — empty, UI-only, custom pass graphs are all valid).
3. Graph compiler produces the deterministic `MetalExecutionPlan` (dead passes culled; liveness roots are exported resources and side-effect passes).
4. `render_frame` validates submissions against the plan (typed errors before encoding), then encodes one encoder per pass record — attachments, load/store/clear from graph declarations where defined; some semantic handlers still resolve backend-owned textures (see §6).
5. Submit with completion handling; `end_frame` presents via the surface.

**Known limitation:** frames are single-slot serialized — one frame in flight, no per-slot resource ownership yet. [#36](https://github.com/Mik-pe/Katla/issues/36) defines frame-slot lifetimes; [#54](https://github.com/Mik-pe/Katla/issues/54) builds the Metal 4 command model on top.

## 4. Resource Model

- Textures intended for GPU use are Private storage; CPU bytes enter exclusively through `TextureUploadQueue` staging. One documented CPU-readback exception exists in `create_texture_shared`.
- Bindless textures bind through one argument buffer; the geometry pass declares its residency explicitly rather than scanning the registry.
- Pipelines are created through descriptors that consult `MetalPipelineArchive` and register into it; corrupt/stale archives rebuild atomically. Pipeline cache keys cover shader source hashes, entry points, layouts, formats, and blend/depth/stencil/raster state.
- Transient render-graph textures are realized per frame; live-range aliasing is not yet implemented ([#35](https://github.com/Mik-pe/Katla/issues/35)).

## 5. Render-Graph Integration

The render graph (`katla_gfx/src/render_graph/`) has three layers: pure structure/compilation (no GPU types), the `RenderGraphBackend` trait, and per-backend implementations (`vulkan_backend.rs`, `metal_backend.rs`). The graph owns topology, ordering, liveness, and diagnostics (text/JSON/DOT). Metal owns native resource realization and command encoding only — it may reject unsupported executable payloads before command-buffer creation but must not invent passes or silently add editor behavior.

Graph-side work still open: buffer resources with range-aware dependencies ([#31](https://github.com/Mik-pe/Katla/issues/31)), typed image accesses/subresource ranges ([#30](https://github.com/Mik-pe/Katla/issues/30)), one compiled synchronization plan for both backends ([#33](https://github.com/Mik-pe/Katla/issues/33)), backend-neutral compute commands ([#32](https://github.com/Mik-pe/Katla/issues/32)), full executable-payload generification ([#56](https://github.com/Mik-pe/Katla/issues/56)).

## 6. Honest Gaps

- **Direct-to-drawable legacy path:** removed. Validation gates every frame; missing state is a typed `RendererError`, never a silent fallback (closed via [#51](https://github.com/Mik-pe/Katla/issues/51)).
- **Single-slot serialization:** no real frames in flight until [#36](https://github.com/Mik-pe/Katla/issues/36) + [#54](https://github.com/Mik-pe/Katla/issues/54).
- **Texture upload:** the staged queue is live, but mip-chain policy, subresource partial updates, and per-frame byte bounds remain on [#58](https://github.com/Mik-pe/Katla/issues/58), which is blocked on a documented shared→private storage anomaly (Xcode GPU capture pending — do not flip storage modes).
- **Pipeline cache:** archive + observability are live; async warmup, off-thread hot reload, and the latency benchmark remain on [#53](https://github.com/Mik-pe/Katla/issues/53).
- **Metal 4:** command path, argument tables, and residency sets are clean-cut migrations on [#54](https://github.com/Mik-pe/Katla/issues/54) and [#55](https://github.com/Mik-pe/Katla/issues/55) — no dual-runtime transition is planned.

## 7. Related Documents

- `vulkan_to_metal_mapping.md` — Vulkan-side API equivalents (historical migration aid from the `ash`-era; the native backend does not follow it).
- `backend_agnostic_render_graph.md` — render-graph design background (superseded in part by the implemented graph; see `render_graph/mod.rs`).
- `archive/metal_backend_implementation.md` — the superseded implementation plan (historical only).
- `memory-bank/systemPatterns.md` — whole-engine architecture.
