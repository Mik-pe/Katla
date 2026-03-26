# Architecture

Architectural decisions, patterns, and conventions for the Katla engine.

**What belongs here:** Architectural decisions, module organization, coding patterns.

## Workspace Structure

- `katla_math` - Math library (vectors, matrices, quaternions) - NO dependencies on other crates
- `katla_gfx` - Graphics API layer - NO dependencies on katla_math, katla_ecs, katla_app
- `katla_ecs` - Entity Component System - NO dependencies on other katla crates
- `katla_ui` - Immediate mode UI - CAN depend on katla_math, katla_gfx
- `katla_app` - Application framework - can depend on all other crates
- `katla_derive` - Proc macros for ECS

## Dependency Rules (CRITICAL)

```
katla_math  ← (nothing)
katla_gfx   ← (nothing)
katla_ecs   ← (nothing) — NOTE: previously violated by katla_math dep, fixed in cleanup mission
katla_ui    ← katla_math, katla_gfx
katla_app   ← katla_math, katla_gfx, katla_ecs, katla_ui
```

## Workspace Dependencies

All shared dependencies are centralized in root `Cargo.toml` under `[workspace.dependencies]`. Crates reference them with `{ workspace = true }`. No wildcard `"*"` versions allowed.

## katla_math Specifics

### SSE Backend Coverage

Only `Vec4`, `Quat`, and `Mat4` have SSE backends (`katla_math/src/sse/`). `Vec2` and `Vec3` are scalar-only. API standardization work only needs dual-backend changes for `Vec4`/`Quat`/`Mat4`.

### SSE Vec4 Const Limitation

`Vec4` on x86_64 uses SSE intrinsics (`__m128`) which cannot be used in `const` contexts. Therefore `Vec4::ZERO`/`ONE` etc exist only as const associated constants on the scalar implementation, while the SSE implementation provides `fn zero()`/`fn one()` methods. Tests targeting `Vec4` must use method calls since x86_64 uses the SSE path.

### AABB vs Sphere create_from_verts Input Types

`AABB::create_from_verts` accepts `&[Vec3]` while `Sphere::create_from_verts` accepts `&[f32; 3]`. This API inconsistency limits shared helper dedup without allocation (Sphere currently heap-allocates a `Vec<Vec3>` to call the shared `compute_bounds` helper).

## ECS Component Trait

The `Component` trait in `katla_ecs` is a marker trait bounded by `Any`:
```rust
pub trait Component: Any {}
```
The `#[derive(Component)]` macro generates an empty `impl<T> Component for T {}`. The derive macro's only value is ergonomic `#[derive(Component)]` syntax — the trait bound `T: Any` is what enables type-erased downcasting via `AnyComponentStorage`. The `as_any`/`as_any_mut` methods live on `AnyComponentStorage`, not on `Component`.

## Code Conventions

- Visibility: `pub(crate)` by default, `pub` only when necessary
- No backwards compatibility - remove old code when replacing
- No hybrid implementations - single way to do things
- Error handling: Use `Option<T>`, `Result<T, E>`, avoid `unwrap()` in production

## UI Architecture

- Immediate mode UI pattern
- `UiContext::begin()` → widget calls → `UiContext::end()` returns `DrawList`
- `DrawList` converted to `UIDrawList` for GPU rendering
- Font atlas with white pixel at (0,0) for solid color rendering

## Graphics Rendering Concepts

### Texture Color Space Formats

**SRGB vs UNORM Texture Formats:**
- **SRGB format** (`rgba8_srgb`): Texture data is in sRGB color space. GPU automatically converts to linear space during sampling. Required for UI/font textures that contain color data meant for display.
- **UNORM format** (`rgba8_unorm`): Texture data is in linear color space. No automatic conversion during sampling. Generally used for non-color data (normal maps, roughness maps, etc.).

**Critical for rendering correctness:** When a texture contains display colors (like font atlases), it must be created with SRGB format. If created with UNORM format, the color values will be interpreted incorrectly:

- White pixel [255,255,255,255] in SRGB space → samples as pure white (1.0, 1.0, 1.0, 1.0)
- Same pixel in UNORM (linear) space → interpreted as linear white, which appears semi-transparent when blended with sRGB render targets

**Code example:** Font atlas creation
```rust
// CORRECT - Font atlas with color data
let font_atlas = device.create_texture(TextureDescriptor {
    format: TextureFormat::rgba8_srgb(),  // sRGB for color data
    // ...
});

// WRONG - Would cause transparency issues
let font_atlas = device.create_texture(TextureDescriptor {
    format: TextureFormat::rgba8_unorm(),  // Linear space - wrong for fonts
    // ...
});
```

### Shader Texture Modulation Pattern

**Solid color rendering via white pixel sampling:**

UI shaders use a common pattern for efficient solid color rendering:
```
output_color = vertex_color * texture_sample(texture, uv)
```

**How it works:**
1. Solid color quads set UV coordinates to (0, 0)
2. Font atlas has a white pixel [255,255,255,255] at UV (0, 0)
3. Shader samples white pixel: (1.0, 1.0, 1.0, 1.0)
4. Shader multiplies by vertex color: `vertex_color * (1.0, 1.0, 1.0, 1.0) = vertex_color`
5. Result: Efficient solid color rendering without special cases

**Why this matters:** The white pixel must be in the correct color space (SRGB) for the multiplication to work correctly. If the texture format is UNORM, the white pixel samples as linear white which renders semi-transparent.

## Render Graph Synchronization

### Pass Dependencies

Every render pass must declare its resource dependencies explicitly:
- `.read("resource_name")` - Pass samples from this texture (requires `SHADER_READ_ONLY_OPTIMAL`)
- `.write("resource_name")` - Pass writes to this texture/color attachment

**Why this matters:** The render graph uses these declarations to automatically insert Vulkan pipeline barriers with correct stage and access masks. Missing dependencies can cause:
- Race conditions between passes
- Visual flickering when framerate varies
- Vulkan validation errors

### Example: UI Pass Sampling Tonemapped Scene

```rust
// CORRECT - Declares read dependency
.add_pass(UIPass::new("ui")
    .read("ldr_color")       // UI samples tonemapped scene
    .write("backbuffer")     // UI writes to swapchain
    .material(ui_material))

// WRONG - Missing read dependency causes sync issues
.add_pass(UIPass::new("ui")
    .write("backbuffer")     // No read declared - barrier not inserted!
    .material(ui_material))
```

When a pass samples a transient texture via bindless, the read dependency MUST be declared so the render graph inserts the correct barrier:
- `srcStage = COLOR_ATTACHMENT_OUTPUT` (previous pass writes)
- `dstStage = FRAGMENT_SHADER` (this pass samples)
- `srcAccess = COLOR_ATTACHMENT_WRITE`
- `dstAccess = SHADER_READ`

## ECS Event System

Entity and component events use a simple `Vec`-based queue on `World`:

- **`EntityEvent`** enum: `Spawned(EntityId)`, `Destroyed(EntityId)` — defined in `katla_ecs/src/events.rs`
- **`ComponentEvent`** enum: `Added(EntityId, TypeId)`, `Removed(EntityId, TypeId)` — uses `TypeId` for type-safe filtering
- Events emitted from `create_entity`/`destroy_entity`/`add_component`/`remove_component`
- Access: `world.entity_events() -> &[EntityEvent]`, `world.component_events() -> &[ComponentEvent]`, `world.component_events_for::<T>() -> Vec<&ComponentEvent>` (filtered by TypeId)
- **Flushed at end of `update()`** — events from the current frame are visible to systems during `update()` but cleared afterward
- `cleanup_empty_entities()` and `clear_entities()` do NOT emit events (bulk cleanup, not user-initiated)

`World::add_component` is generic `<T: Component + 'static>` (not `impl Trait`) to capture `TypeId::of::<T>()` for event emission.

## ECS Change Detection

Per-component change detection via generation counters:

- Each `ComponentStorage` has a `SparseSet<EntityId, u64>` tracking generation per entity
- Generation incremented on `insert()` (add_component) and `get_mut()` (get_component_mut)
- `ComponentStorageManager` maintains a `changed_generations` snapshot per component TypeId
- `world.query_changed::<Q>()` returns entities whose generation > snapshot (union semantics for multi-component tuples)
- `clear_changed()` called at end of `update()` resets the snapshot
- `collect_changed_entity_ids()` iterates all entities with any component, not just queried types — minor perf note for large worlds

## ECS World Internal: UnsafeCell

`World.storage` is wrapped in `UnsafeCell<ComponentStorageManager>` to support `query_ref(&self)` (immutable queries from shared references):

- All storage access goes through `self.storage.get()` (unsafe, `&self`) or `self.storage.get_mut()` (safe, `&mut self`)
- `query_ref()` is sound only for immutable `QueryData` types (`&T`, `(&T, &U)`, etc.) — mutable queries through `query_ref` would be UB but are not prevented at compile time
- `UnsafeCell` makes `World` `!Sync` (acceptable — World is never shared across threads)
- Old `pub(crate) storage` field removed; use `world.storage_mut(&mut self)` for direct access

## ECS QueryData Trait Maintenance

`QueryData` trait has methods that must be implemented across all arity files (`iter1.rs` through `iter8.rs`). When adding new trait methods (e.g., `type_ids_for_changed`, `entity_id_from_item`), ALL 8 files must be updated. This is a maintenance burden — be thorough when adding methods.

## GPU Resource Destroy APIs

Per-resource destroy methods on `VulkanRenderer` (in `katla_gfx/src/renderer/destroy_api.rs`):

- `destroy_mesh(handle)` — delegates to `AssetRegistry::remove_mesh`, GPU buffers dropped via `MeshAsset` Drop
- `destroy_material(handle)` — removes from `AssetRegistry`, also destroys descriptor set layout and associated pipeline
- `destroy_texture(handle)` — checks default texture guard, releases bindless slot via `BindlessTextureManager::release_texture_slot`, then `TextureManager::destroy`
- `destroy_skeleton(handle)` — removes from both `skeleton_descriptors` and `skeleton_buffers` ResourceStorages

**Safety guarantees:**
- Double-destroy is safe (no-op, `ResourceStorage::remove` returns `None` for already-removed slots)
- `NONE` handles (`index = u32::MAX`) and unowned handles are safe (returns `None` from storage)
- Default textures (slots 0-4 in TextureManager) are protected by `is_default_texture()` guard

**Key design notes:**
- `BindlessTextureManager::release_texture_slot()` protects default slots (0-4) from being freed
- `TextureManager::destroy()` also removes bindless slot tracking via `unregister_bindless_slot()`
- Material destroy cascades: removes material → destroys descriptor set layout → removes pipeline from registry
- `AssetRegistry::remove_mesh/remove_material` return `Option<T>` for caller cleanup if needed
- `AssetRegistry::remove_pipeline` is `pub(crate)` since only the renderer should directly remove pipelines
- `material_descriptor_set` on `MaterialAsset` is currently always `None` (never populated by the material compiler). `destroy_material` destroys the descriptor set layout but not the descriptor set. When descriptor sets are later populated, `destroy_material` must also free them.
- `destroy_texture` has an explicit `handle.is_none()` guard while `destroy_mesh`, `destroy_material`, and `destroy_skeleton` do not. All are safe due to `ResourceStorage::remove` returning `None` for invalid indices, but the pattern is inconsistent.

## GPU Resource Cleanup (katla_app)

`GpuResourceTracker` in `katla_app/src/gpu_resource_tracker.rs` provides reference-counted tracking for meshes, materials, textures, and skeletons. Used by scene load and entity destruction for GPU cleanup.

**Initialization:** Two-phase — created in `builder.rs` with `MaterialHandle::NONE` as protected material, then `set_protected_material()` called during `Application::init()` after the actual default PBR material is compiled.

**Scene load cleanup:** `load_scene` calls `gpu_resource_tracker.release_all()` before `clear_entities()`, destroying all tracked GPU resources in bulk.

**Entity deletion cleanup:** `EditorAction::DeleteEntity` calls `app.world.destroy_entity(id)` which fires `EntityEvent::Destroyed`. The `gpu_cleanup` module processes these events on the next frame after `world.update()`, releasing per-entity GPU resources. This is the standard pattern for entity deletion GPU cleanup.

**Shared resources:** Protected by reference counting — a mesh used by multiple entities is only destroyed when all references are released. `GpuResourceTracker::release(entity_id)` decrements ref counts and only destroys when count reaches zero.

**Known gap — GLTF textures not tracked:** `track_texture()` API exists on `GpuResourceTracker` but is never called from the spawn path (`spawn_gltf_model` in `spawning.rs`). GLTF model textures (albedo, normal, MR, AO, emission) are uploaded but not tracked, so they leak on scene load. Texture tracking would require either adding texture handles to `DrawableComponent` or a separate per-entity texture list.

**Known gap — ComponentEvent::Removed not integrated:** The `gpu_cleanup` module only handles `EntityEvent::Destroyed`, not `ComponentEvent::Removed`. If `DrawableComponent` is removed from a live entity without destroying it, GPU resources would leak.
