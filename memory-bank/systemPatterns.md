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

### Viewport Panel Containment

The 3D scene is composed for the editor's viewport panel's aspect ratio (the camera uses `viewport_size()` = panel dims). To match, the 3D-scene render targets (HDR, depth, tonemap-output, picking) and the Forward+ light-culling grid are **panel-sized**, not swapchain-sized. `Application::recreate_panel_rt_resources()` (renderer.rs) recreates them at the panel size (logical bounds × scale_factor) each frame when it changes, calling `GpuRenderer::recreate_scene_render_targets()` + `frame_graph.recreate_transient_textures()`.

`set_viewport_panel_rect()` (called each frame before render) passes the panel rect in physical pixels. In the editor preset, Metal tonemaps panel-sized HDR into the graph-owned `viewport_0` texture using texture-local coordinates, then the UI composites that texture into the drawable. When no UI composition pass is declared, the fullscreen output falls back to the drawable and converts the top-down panel origin to Metal's bottom-up viewport coordinates.

When `viewport_panel_rect` is `None`, scene passes use the full drawable extent. A UI-only or empty graph does not require scene depth, HDR targets, or a tonemap fence.

3-set layout: Set 0 (per-frame uniforms + storage buffer), Set 1 (bindless texture array up to 4096), Set 2 (skeletal animation joints). Never use push constants.

### Render Graph

Generic over `GpuRenderer`. `FrameGraphBuilder` provides fluent API. Passes live in `render_graph/passes/`. Automatic barrier insertion.

The compiled dependency DAG and execution order are authoritative. Backends consume compiled pass identity and access metadata rather than maintaining a second name-based graph. Metal currently maps semantic `PassKind` values to fixed encoder implementations, but pass presence is optional: empty, UI-only, geometry-only, post-processed scene, and scene-without-UI topologies are valid. Any remaining singleton/order/unsupported-kind restriction must identify itself as a fixed-encoder limitation, not a core graph rule.

A backend must not encode work for an absent pass. Scene-only resources such as depth/HDR targets and synchronization objects are required only when the selected schedule needs them. Submitted command buffers are checked after completion and terminal GPU failures are returned as structured `RendererError` values.

Frame-graph topology is application policy. `ApplicationBuilder::with_frame_graph` receives the initialized backend and resource paths exactly once and returns an `ApplicationFrameGraph`; construction errors propagate without fallback. `ApplicationFrameGraph::new` selects `GraphOnly`, which executes the graph without Katla injecting scene, shadow, post-processing, particle, animation, picking, or editor work. The existing scene/editor pipeline is selected through the explicit `KatlaEditorFrameGraphPreset`.

Katla's optional built-in runtime resolves pass and transient-resource roles from `FrameGraphBindings`. Absence is represented by `Option`, never `PassId(0)` or another valid-ID sentinel. Bindings are validated at construction, re-resolved after graph mutation, and all submission, resize, bindless, picking, and per-frame subsystem work must check the corresponding capability.

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
