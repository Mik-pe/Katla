# System Patterns

Architecture and conventions for the Katla codebase. This is the single source of truth — if it's wrong, fix it here.

## Workspace Structure

```
katla_gfx    — Vulkan/Metal wrapper, render graph, materials, shaders (WGSL via naga)
katla_ecs    — Custom ECS: sparse set storage, query system, parallel scheduler
katla_math   — SIMD math: Vec2/3/4, Mat2/3/4, Quat, Transform, AABB, Frustum
katla_ui     — Declarative retained-mode UI on top of immediate-mode core (Taffy layout)
katla_app    — Application framework, editor, systems bridging all crates
katla_physics — Rapier3D wrapper with ECS components
katla_audio  — Standalone audio pipeline (cpal + hound + lewton)
katla_script — Luau scripting via mlua, entity lifecycle hooks, sandboxed
katla_derive — Proc-macro crate: #[derive(Component)] with #[inspect(...)] attributes
katla_icons  — ForkAwesome icon constants for the UI
```

## Dependency Boundaries (Enforced)

```
katla_math    → (nothing — zero internal deps)
katla_ecs     → katla_derive
katla_derive  → (nothing — proc-macro only)
katla_ui      → katla_math, katla_gfx, katla_icons
katla_physics → katla_ecs, katla_gfx, katla_math, rapier3d
katla_audio   → (nothing — zero internal deps)
katla_script  → katla_ecs, katla_math, katla_derive, mlua
katla_gfx     → katla_math, katla_icons
katla_app     → katla_gfx, katla_ecs, katla_math, katla_ui, katla_physics, katla_audio, katla_script
```

Violating these will cause compile errors. Do not work around them.

## GPU Backend Architecture (katla_gfx)

Dual backend selected at runtime:

- `GpuRenderer` trait — backend-agnostic API
- `VulkanRenderer` — via `ash` (all platforms, MoltenVK on macOS)
- `MetalRenderer` — via `objc2-metal` (macOS only, cfg-gated)
- `AnyRenderer` — enum dispatch

Backend code lives in `vulkan/` and `metal/`. When adding features:
1. Add method to `GpuRenderer` trait (default no-op)
2. Implement for both backends
3. Add dispatch to `AnyRenderer`

`katla_gfx` does NOT depend on `katla_math`. Use `crate::Size2D`, `crate::Rect`, etc. for native types.

### Frame-Scoped Rendering API

One frame means one token. `GpuRenderer::acquire_frame()` returns `FrameAcquisition`:

- `Ready(FrameToken)` — the token owns one reusable frame slot (and, when windowed, one surface image). Acquisition **waits for that slot's previous submission** before handing out the token, so frame-local CPU writes can never race a slot still in flight.
- `Unavailable` — the surface produced no drawable this tick (minimized/occluded); nothing was touched, retry later.
- `OutOfDate` — stale surface; recreate it and acquire again.

Every frame-local operation takes the token: `set_frame_uniforms`, `execute_draw_calls`, `draw`, `upload_lights`, `upload_shadow_cascades`, and graph execution via `render`. Backends validate the token against their open frame and reject stale/superseded/never-acquired tokens with a typed error — callers cannot write into another frame's slot. `present(token)` consumes it with one documented semantic on both backends (submit + present; returns after enqueue/commit, not GPU retirement). `abort(token)` abandons the frame: nothing submitted or presented, slot left reusable. A failed `render` poisons the frame so `present` refuses to submit half-encoded work, and `abort` clears the poison.

Two invariants that are easy to get wrong:

- **A slot advances on `present`, not on acquire.** Acquiring again while a frame is still open abandons that frame *on the same slot*; ownership is per acquisition (a generation counter), not per slot index. Only `present` rotates the slot.
- **`FrameToken` is `Copy`.** There is no destructor, so "dropping a token" is not an event — abandonment is the next acquisition (which calls `frame_clear` first). Never document or test a drop-token path.

There is no second, implicit call-ordering path: `begin_frame`/`end_frame`/`wait_for_frame` and the untokened write methods are deleted, not wrapped. `GpuRenderer` requires every frame-local method to be implemented explicitly (a default no-op would let a backend silently skip frame work).

### Prepared Frame Draws

Draw submissions are prepared once per frame and consumed by reference. `Frame::submit`/`AnyFrame::submit` take **`Rc<DrawList>`**, so a list moves into frame-owned storage once and sharing it across several passes costs a refcount bump instead of a deep clone. `PassExecutionData::prepared()` exposes `PreparedDraws<'_>`, which borrows those submissions: `iter()` walks every draw in submit order (the order the old per-pass merge produced) and object slots stay exactly as `DrawList::push` assigned them. `PreparedDrawCounts` supplies per-pass draw/instance totals for traces.

Every pass consumer reads the borrowed view — Vulkan geometry/draw calls, parallel geometry, outline scissor; Metal geometry, depth-prepass, outline, shadow, object-id. Backends must NOT rebuild a merged `DrawList` per pass or per frame: that was the cost this replaced (64 lists × 8 draws went from 395 µs/430 allocations to 9.7 µs/21 per frame). Metal's frame-level upload walk uploads each unique submission once, identified by `Rc::as_ptr` — shared lists must not be re-uploaded, but every slot the frame encodes must still be initialized.

### Resource Handles (generational)

`Handle<T>` identifies `(slot, generation)`; markers (`MeshMarker`, `MaterialMarker`, `TextureMarker`, `SkeletonMarker`, `EmitterMarker`, …) make handles type-safe. `ResourceStorage<T, M>` recycles slots but bumps the slot's generation on every removal, so a stale handle can never alias the resource later occupying the same slot — lookups validate both parts and `iter_enumerated` yields only live handles. `Handle::from_raw(index, generation)` is the only raw constructor (tests/diagnostics); handles are created exclusively by resource registration. Bindless descriptor/table slots are a GPU-side addressing scheme, never CPU resource identity: handle→slot maps are keyed by the full handle. Destroy invalidates handles permanently; double-destroy is harmless.

### Viewport Panel Containment

The 3D scene is composed for the editor's viewport panel's aspect ratio (the camera uses `viewport_size()` = panel dims). To match, the 3D-scene render targets (HDR, depth, tonemap-output, picking) and the Forward+ light-culling grid are **panel-sized**, not swapchain-sized. `Application::recreate_panel_rt_resources()` (renderer.rs) recreates them at the panel size (logical bounds × scale_factor) each frame when it changes, calling `GpuRenderer::recreate_scene_render_targets()` + `frame_graph.recreate_transient_textures()`.

`set_viewport_panel_rect()` (called each frame before render) passes the panel rect in physical pixels. In the editor preset, Metal tonemaps panel-sized HDR into the graph-owned `viewport_0` texture using texture-local coordinates, then the UI composites that texture into the drawable. When no UI composition pass is declared, the fullscreen output falls back to the drawable and converts the top-down panel origin to Metal's bottom-up viewport coordinates.

When `viewport_panel_rect` is `None`, scene passes use the full drawable extent. A UI-only or empty graph does not require scene depth, HDR targets, or a tonemap fence.

3-set layout: Set 0 (per-frame uniforms + storage buffer), Set 1 (bindless texture array up to 4096), Set 2 (skeletal animation joints). Never use push constants.

### Render Graph

Generic over `GpuRenderer`. `FrameGraphBuilder` provides fluent API. Passes live in `render_graph/passes/`. Automatic barrier insertion. Attachment load/store/clear behavior comes ONLY from pass declarations (`AttachmentOps` per color target, per-aspect `DepthStencilAttachmentOps`); graph compile normalizes the reverse-Z depth default and validates contracts (missing/stray ops, aspect mismatches, loads need an in-graph producer or an imported image whose `ImportedImageContract.initial` is not `Undefined`) before encoding; backends translate declarations exactly with no renderer-local heuristics.

Every built-in pass template hand-declares its typed image accesses (`NamedImageAccess` via the `named_image_access` helper in `passes/mod.rs`); the coarse→typed refinement remains only for the low-level `PassDesc`/`SimplePass` API. Generic sampled reads declare the whole-resource range (ALL aspects): sampling reads whichever aspect the target image carries and the graph cannot narrow it without the format — aspect-precise reads use `.with_range`. Never narrow a generic read to COLOR aspects: it silently unhazards it against depth-aspect writes on the same image (this culled the shadow pass and black-screened the editor for a day).

Imported images (including the built-in backbuffer) carry `ImportedImageContract { initial, required_final }`: `import_resource(name, handle, contract)` at the builder, `backbuffer_contract(...)` to override the default (backbuffer default = observable contents, i.e. the previous load-exemption semantics). A required final state differing from the initial one requires a live pass to access the image. Contracts are validated at compile and exposed in diagnostics (schema v6); backend sync-plan consumption is #33.

Synchronization is compiled, not inferred: the same typed accesses that build the DAG also build a `SyncPlan` inside `ExecutionPlan` — ordered per-pass image operations carrying subresource ranges, before/after states (usage + pipeline stage + access mode), and hazard reasons, plus imported-contract initial seeding and frame-end final-contract operations. Transients follow a frame-periodic steady state (the state one frame ends in is the state the next frame starts from); freshly created textures get discard-safe undefined→target bootstrap operations that backends coalesce once the tracked layout matches, and same-state cross-pass hazards encode same-layout ordering barriers. Vulkan realizes operations directly as synchronization2 barriers with explicit masks from the typed states (no layout-pair mask inference, no TOP_OF_PIPE fallback on the graph path); Metal realizes them through driver-tracked resources (documented classification, no explicit image barriers). The backbuffer's acquire/present barriers remain renderer-side until contract consumption, and the backend-owned scene depth keeps its explicit render-pass-instance barrier until it becomes a graph resource. Undefined-source bootstrap transitions are always encoded with conservative `ALL_COMMANDS`/`MEMORY_WRITE` source masks: the layout transition itself is a write, and under aliasing the image's physical memory carries the previous slot member's stores.

Transient allocation is compiled, not per-descriptor: `initialize_transient_textures` builds the `TransientAllocationPlan` (compatibility keys + non-overlapping lifetimes from the same compiled plan) and routes slot groups through `RenderGraphBackend::create_transient_slot(&[GraphResourceDesc])` — there is no per-descriptor backend creation entry point. Vulkan lowers multi-member slots to `ALIAS` images bound at offset 0 of one `VkDeviceMemory` per frame slot (`Rc<VkSlotMemory>` shared by members; lazily allocated memory only for attachment-only slots); Metal lowers to standalone allocations over the same grouping until heap aliasing lands. Single-member slots route to the backend's standalone path — never drop them from grouping. Aliasing is discard-by-design: entering a member's live interval finds whatever the previous member wrote, which matches the fresh-allocation semantics the bootstrap ops already model. `set_transient_aliasing(false)` forces standalone allocations for debugging without changing graph semantics. Slot assignments, bytes, and live spans are exposed in diagnostics (`transient_slots`, schema v10), which also render each slot's compatibility class, member order (alias predecessor → successor), summed logical bytes, and estimated bytes saved; DOT draws slots as dashed `cylinder` nodes wired to member resources so physical allocations are visually distinct from logical graph resources.

Pass liveness is rooted only in explicitly exported resources and explicitly side-effecting passes. Liveness walks true read-after-write producer edges backwards, so overwritten or unrelated branches are culled without keeping false WAW/WAR dependencies alive. A pass that preserves or blends an attachment must declare that target as both read and written. Submissions to culled passes are rejected as structured graph errors rather than silently dropped. Dependency analysis is subresource-range aware: typed `ImageAccess` ranges drive RAW/WAR/WAW edges (overlapping ranges hazard, disjoint mips/layers/aspects independent), and a writer replaces only the subresources it covers.

A backend must not encode work for an absent or culled pass. Scene-only resources such as depth/HDR targets and synchronization objects are required only when the selected plan needs them. Submitted command buffers are checked after completion and terminal GPU failures are returned as structured `RendererError` values.

Frame-graph topology is application policy. `ApplicationBuilder::with_frame_graph` receives the initialized backend and resource paths exactly once and returns an `ApplicationFrameGraph`; construction errors propagate without fallback. `ApplicationFrameGraph::new` selects `GraphOnly`, which executes the graph without Katla injecting scene, shadow, post-processing, particle, animation, picking, or editor work. The existing scene/editor pipeline is selected through the explicit `KatlaEditorFrameGraphPreset`.

Katla's optional built-in runtime resolves pass and transient-resource roles from `FrameGraphBindings`. Absence is represented by `Option`, never `PassId(0)` or another valid-ID sentinel. Bindings are validated at construction, re-resolved after graph mutation, and all submission, resize, bindless, picking, and per-frame subsystem work must check the corresponding capability.

## Headless Rendering

Vulkan and Metal share the application's scene/editor preparation and graph submission paths. Linux headless mode owns two offscreen color targets instead of a presentation swapchain; it needs a Vulkan driver but no window system. Captures use 2560×1440 physical pixels and a 1280×720 logical UI. Readback observes completed GPU work and saves PNGs.

Windowed Vulkan initialization and resize receive explicit pixel dimensions; variable-extent surfaces use these dimensions clamped to surface limits. Swapchain replacement also replaces its synchronization objects. Shutdown releases the Vulkan surface before the native window/display teardown.

Vulkan frame fences are waited without resetting and reset immediately before submission. Scene attachment dimensions are independent of output dimensions, so panel-sized depth, lighting tiles, and graphics viewports agree. Vulkan cascaded shadows share one atlas pass, with independent cascade parameters for each frame slot. UI vertex and instanced pipelines are rebuilt together when material layouts change; texture selection belongs to draw data.

## ECS Architecture (katla_ecs)

### EntityId

64-bit: `[32-bit generation | 32-bit index]`. Generation detects stale references on slot reuse. Created via `World::create_entity()` or `world.spawn()`.

### Storage

Per-type `ComponentStorage<T>` wrapping a **paged sparse set** (`SparseSet<EntityId, T>`):
- O(1) insert, lookup, remove
- Pages of 1024 entries allocated on demand
- Per-type dirty tracking: `insert()` and `get_mut()` mark dirty, `clear_changed()` resets

### Query System

`world.query::<(&A, &mut B, &C)>()` — iterator over entities with all components. Up to arity 8.

- `ImmutableQuery` sealed trait — prevents `&mut T` from `&World`
- Filters: `With<T>`, `Without<T>`, combinable as tuples
- Change detection: `query_changed::<&A>()` yields only dirty entities

### Systems

Implement `System` trait. Must override `component_access()` and `resource_access()` for parallel safety. Default "no declared access" is dangerous — parallel scheduler assumes no conflicts.

Execution order: First, Early, Normal, Late, Last. Sequential via `world.update(dt)`, parallel via `world.update_parallel(dt)` (rayon).

### Events

`EntityEvent::Spawned/Destroyed` and `ComponentEvent::Added/Removed`. Emitted each frame, drained via `world.entity_events()` / `world.component_events()`.

### Editor Features (behind `editor` feature flag)

- `Inspect` trait — runtime field metadata for inspector (auto-generated by `#[derive(Component)]`)
- `Agent` trait — observe→decide→act loop for AI scene manipulation
- `SceneTool` — structured operations (spawn, destroy, add/remove component, set field, duplicate, undo groups)
- `#[inspect(...)]` attributes: `skip`, `color`, `range(min, max)`, `speed(f32)`, `display_name`, `enum`, `struct`, `vec`, `entity_ref`

## UI Architecture (katla_ui)

Two layers:
1. **Declarative** (`declarative/`) — Primary API. Implement `Build` trait → `ViewTree::frame()` does build/diff/layout/input/draw. Drain actions from `ViewTree::actions_mut()`.
2. **Immediate-mode** (`context/`) — Low-level primitives. Avoid unless building custom widgets.

Rendering: `DrawList` of `InstanceData` (GPU-instanced quads, 56 bytes each) + `Vertex`/`DrawCmd` (complex geometry). `TextureId` is opaque — katla_app maps to GPU handles. Taffy does Flexbox layout.

Editor dock panels are all built in a stable order because their current state hooks are positional within the root `BuildContext`. `EditorOverlayView` only mounts the active tab from each `DockTree` leaf into the ZStack; building an inactive tab preserves its state slots, while leaving it unmounted prevents stale environment data from drawing or receiving input.

`DockSpace` is the sole owner of tab and splitter interaction. It remains non-interactive in normal hit testing so panel content is not blocked, and receives chrome/drag events through the declarative global-input pass. Dock actions are applied by the editor after `ViewTree::frame()`. Splitter ratios are local to each split node's bounds, and tab move actions retain the exact dragged tab identity.

## Physics (katla_physics)

Rapier3D wrapper. `PhysicsWorld` owns all Rapier state. ECS components: `RigidBody`, `ColliderShape` (Sphere/Box/Capsule/Trimesh/ConvexHull/Heightfield), `CollisionFilter` (layers+mask bitfields). `PhysicsActive(bool)` resource gates simulation behind play mode.

## Scripting (katla_script)

mlua with Luau. Per-entity instances with lifecycle hooks: `on_spawn`, `on_update`, `on_destroy`. Sandbox strips dangerous stdlib functions. 10M instruction limit, 5s timeout. Script↔engine communication via pending-command resources (one frame delay for safety).

## Matrix/Math Conventions

- Column-major only. `Mat4(pub [Vec4; 4])`. `m[col][row]`. `m[0]` = column 0.
- Vec2/3 scalar (not worth SSE). Vec4/Mat4/Quat use SSE on x86.
- Colors in spawning functions are sRGB, converted to linear internally.

## Code Hygiene (project rules)

- Never allow `dead_code`: no dead code, no `#[allow(dead_code)]`, no
  `#[expect(dead_code)]` suppressions. Delete instead of suppressing; git
  history preserves anything needed later. (Rule set 2026-09-09.)
