# Architecture

## Workspace Structure

```
katla/
├── katla_math/     — Custom math library (vectors, matrices, quaternions) with SIMD
├── katla_gfx/      — High-level graphics API (Vulkan backend, render graph, shaders)
├── katla_ecs/      — Custom Entity Component System framework
├── katla_derive/   — Derive macros for ECS (Component trait)
├── katla_ui/       — Immediate mode UI system for debug overlays and in-game HUDs
├── katla_app/      — Application framework, components, systems, editor
├── katla_icons/    — ForkAwesome icon codepoints (zero-dep)
└── resources/      — Shaders (WGSL), fonts, assets
```

## Dependency Graph (Enforced)

```
katla_math  ← (no deps)
katla_ecs   ← (no deps)
katla_derive ← (no deps)
katla_icons ← (no deps)
katla_gfx   ← (no workspace deps)
katla_ui    ← katla_math, katla_gfx
katla_app   ← katla_gfx, katla_ecs, katla_math, katla_ui
```

## Key Architectural Patterns

### Rendering Pipeline
- **Frame Graph**: Declarative render pass scheduling with automatic resource barriers
- **Bindless Resources**: Textures accessed via index arrays, not descriptor sets
- **Depth Prepass**: Writes object IDs (R32Uint) for GPU picking + depth buffer for early-Z
- **GPU Particles**: Compute shader-based particle system with emitter pool
- **Single-frame mode**: `cargo run -- -s` runs 25 frames for headless validation

### ECS Architecture
- `World` owns `ComponentStorageManager` which manages typed `ComponentStorage<T>`
- Systems are stateless structs implementing a trait; state lives in `World`
- `Query<Q>` supports multi-component iteration with mutable/immutable access
- Component derive macro: `#[derive(Component)]`

### UI System
- Immediate mode: `UiContext` is passed by `&mut self` to widgets each frame
- Draw lists batch rendering commands; finalized into vertex buffers
- Text rendering: skrifa for font parsing, vello_cpu for glyph rasterization
- Layout stack: row/column nesting with cursor advancement

### Editor (feature-gated)
- All editor code behind `#[cfg(feature = "editor")]`
- Gizmo system: translate/rotate/scale manipulation with axis hit testing
- GPU picking: depth prepass → object ID → entity_instance_map → EntityId
- Billboard rendering: camera-facing quads for point lights, particle emitters

## Data Flows

### Frame Render Loop
1. `ApplicationHandler::window_event(RedrawRequested)`
2. ECS systems update transforms, physics, animations
3. `render_frame()` collects draw calls from ECS components
4. Frame graph executes: depth prepass → shadow pass → geometry pass → compositing → UI
5. Present swapchain image

### GPU Picking Flow
1. Click coordinates queued via `queue_picking_readback()`
2. Depth prepass renders object IDs (instance_index + 1) to R32Uint texture
3. Readback copies pixel at click coords to staging buffer
4. `process_picking()` maps instance_index → EntityId via entity_instance_map

### Scene Serialization
1. `save_scene()` iterates ECS entities, collects components into `Scene` descriptor
2. Serializes to RON (Rusty Object Notation) file
3. `load_scene()` deserializes, spawns entities with `spawn_entity()`
4. GPU resources tracked via `GpuResourceTracker` (ref-counted handles)
