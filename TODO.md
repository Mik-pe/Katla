# TODO

## ECS

### P0: Soundness

- [ ] **Fix `query_ref` soundness hole** — `world.rs:234` allows `world.query_ref::<&mut T>()` from `&self`, creating `&mut` without `&mut World` (UB). Add a sealed `ImmutableQuery` marker trait implemented only for immutable patterns (`&T`, `(&T, &U)`, etc.) and bound `query_ref` on it.
- [ ] **Centralize unsafe borrow pattern in queries** — 16+ raw-pointer casts across `iter2..iter8.rs` replicate the same `storage as *mut ComponentStorageManager` / `(*ptr).get_storage_mut::<T>()` pattern. Extract into a single unsafe helper on `ComponentStorageManager` (e.g., `get_two_storage_mut::<T1, T2>() -> (&mut Storage<T1>, &mut Storage<T2>)`) with one consolidated SAFETY comment, then call it from each query impl.
  - [ ] Add `get_two_storage_mut::<T1, T2>()` unsafe helper on `ComponentStorageManager` with consolidated SAFETY comment
  - [ ] Update `iter2.rs` through `iter8.rs` to use the new helper, removing raw-pointer casts

### P1: Architecture

- [ ] **Move `input` module out of `katla_ecs`** — `input/mod.rs`, `input/mouse.rs`, `input/actions.rs` contain `InputState`, `MouseButton`, `Action` (hardcoded game-specific actions like `MoveForward`, `Jump`). None are ECS concepts. Move to `katla_app` where they're consumed, or into a `katla_input` crate. Keep `World::get_input()` working via a generic or trait.
  - [ ] Create `katla_app::input` module with moved types
  - [ ] Update `World::get_input()` to use a trait-based approach or resource pattern
  - [ ] Remove `input/` from `katla_ecs` and update all consumers
- [ ] **Optimize change detection from O(all_entities * type_ids)** — `storage.rs:collect_changed_entity_ids()` iterates every entity with any component and checks per-type generation lookups. Maintain a per-type `SparseSet<EntityId, ()>` of dirty entity IDs that gets populated on `insert`/`get_mut` and drained on `clear_changed()`. Eliminates the full-scan entirely.
  - [ ] Add per-type `SparseSet<EntityId, ()>` dirty tracking field to storage
  - [ ] Populate dirty set on `insert` and `get_mut`
  - [ ] Rewrite `collect_changed_entity_ids()` to drain dirty sets instead of full scan
  - [ ] Update `clear_changed()` to clear dirty sets
- [ ] **Replace `HashMap` sparse mapping with array-based sparse set** — `SparseSet<K, V>` uses `HashMap<K, usize>` for the sparse array, hashing `EntityId` on every lookup. Since `EntityId` has a dense `u32` index, use `Vec<Option<usize>>` indexed by `EntityId::index()` for true O(1) with zero hashing. Update `SparseSet` to take a key-to-index converter or require `EntityId` keys directly.
  - [ ] Refactor `SparseSet` to use `Vec<Option<usize>>` instead of `HashMap<K, usize>`
  - [ ] Update all `SparseSet` consumers to work with the new API

### P2: Maintenance

- [ ] **Macro-ify query iterator generation** — `iter1..iter8.rs` total ~2000 lines of nearly identical code with 2^N mutability permutations per arity. Create a declarative macro that generates all permutations for a given arity from a template. Adding a 9th component should be a one-line macro invocation.
  - [ ] Design declarative macro template for a single arity's mutability permutations
  - [ ] Generate all mutability permutations for arities 1-8 using the macro
  - [ ] Replace existing `iter1..iter8.rs` files with macro invocations
  - [ ] Verify all permutations compile and existing tests pass

### P3: Polish

- [ ] **Fix doctests** — 10 of 13 doctests are `ignore`d. Convert key examples (World::query, World::spawn, Spawnable) to runnable doctests using `use katla_ecs::*` so they're validated by CI.

## Gizmo

### UX

- [ ] Add plane-drag support (e.g., XY, XZ, YZ planes) for translate and scale modes
- [ ] Calibrate scale sensitivity to screen-space movement (magic 0.01 constant is not zoom-aware)

## Game Maker API

Bite-sized tasks to make the engine usable by game makers. Ordered by impact and independence.

### P1: Lifecycle Hooks

- [ ] Add `ApplicationBuilder::on_init(FnOnce(&mut Application))` — runs after `build()` returns, before event loop, letting game makers spawn initial entities
- [ ] Add `ApplicationBuilder::on_update(FnMut(&mut World, f32))` — called each frame inside `RedrawRequested` between `world.update(dt)` and rendering, for custom game logic
- [ ] Add `ApplicationBuilder::on_shutdown(FnOnce(&mut Application))` — called during `cleanup_on_exit()` for game-side cleanup
- [ ] Wire lifecycle hooks into `Application` fields (store as `Option<Box<dyn FnMut...>>`) and call sites in `mod.rs`

### P2: Spawning Decoupling

- [ ] Extract mesh creation helpers from `Application` spawners into standalone functions that only need `&mut VulkanRenderer` (e.g., `create_cube_mesh`, `create_sphere_mesh` are already on renderer — verify game makers can reach them)
- [ ] Add a `Spawner` newtype or extension trait on `World` that wraps the spawn-with-mesh + component bundle pattern, so basic entity creation doesn't require `&mut Application`
- [ ] Make `spawn_gltf_model` return a `Result` instead of `Option`, with descriptive error variants (file not found, parse error, GPU upload failure)

### P3: Editor Decoupling

- [ ] Gate editor UI behind a Cargo feature flag (`editor`) in `katla_app/Cargo.toml` — default on, game makers can disable with `default-features = false`
- [ ] Move `EditorUI`, `EditorAction`, `FocusedPanel` fields behind `#[cfg(feature = "editor")]` in `Application` struct
- [ ] Move editor-specific frame logic (UI draw list generation, editor action processing, gizmo rendering) behind `#[cfg(feature = "editor")]` guards in `RedrawRequested`
- [ ] Provide a no-editor codepath: when the feature is off, render the viewport fullscreen with no panels

### P4: Resource Loading API

- [ ] Add public `Application::load_texture(path) -> AppResult<TextureHandle>` that wraps renderer texture creation
- [ ] Add public `Application::load_mesh(path) -> AppResult<MeshHandle>` that wraps GLTF mesh loading (without spawning an entity)
- [ ] Add public `Application::load_animation(path, clip_name) -> AppResult<AnimationClip>` for loading animation clips independently
- [ ] Document the handle-based asset workflow in a code example or doc comment on `ApplicationBuilder`

### P5: Polish

- [ ] Audit all `pub(crate)` items in `katla_app/src/components/` — promote to `pub` anything a game maker would need to query or mutate from systems

## katla_gfx

### P0: Visibility Tightening

- [ ] **Change `sync` module to `pub(crate) mod`** — `sync.rs` exposes raw Vulkan wrapper types (`VkSemaphore`, `VkFence`, `VkPipeline`, etc.) to the entire workspace. Make it `pub(crate) mod` and re-export only `ShaderStages` (already re-exported from `lib.rs` via `pipeline_state`). Note: kept `pub` for now because validation examples use `katla_gfx::sync::*` via the external crate path. Gate examples behind a Cargo feature first.
- [ ] **Change `animation` module to `pub(crate) mod`** — Exposes `PoseComputePipeline` and `PoseComputeBuffers` (GPU compute internals). Keep `pub(crate) mod animation` and re-export only the data types: `AnimChannelInfo`, `AnimClipHeader`, `JointInfo`, `SkeletonAnimParams`. Note: kept `pub` for now because validation examples and katla_app use `katla_gfx::animation::*` via the external crate path.
- [ ] **Change `shadow` module to `pub(crate) mod`** — `ShadowBuffers` and `CascadeShadowMapper` are internal GPU subsystems. Make `pub(crate)` and re-export nothing (app layer interacts through `VulkanRenderer` methods and `FrameUniforms`). Note: kept `pub` for now because validation examples use `katla_gfx::shadow::*`.
- [ ] **Change `lighting` module to `pub(crate) mod`** — `PointLightGPU`, `LightCullFrameData`, `LightCullingBuffers` are internal. Make `pub(crate)` and re-export nothing. Note: kept `pub` for now because validation examples and katla_app use `katla_gfx::lighting::*`.

### P0: VulkanRenderer Decomposition

- [ ] **Extract `ShadowState` fields into an owned `ShadowSubsystem` struct** — `VulkanRenderer` has ~10 shadow-related fields (`shadow: ShadowState`, plus cascade descriptor pools/layouts/sets/buffers/allocations/mapped ptrs spread across `shadow.rs`). Move into a `ShadowSubsystem` with `init()` and `destroy()` methods.
- [ ] **Extract light culling into an owned `LightSubsystem` struct** — `light_culling: LightCullingState` plus `resize_light_culling()` logic. Move into a `LightSubsystem` with `init()`, `destroy()`, and `resize()` methods.
- [ ] **Extract outline state into an owned `OutlineSubsystem` struct** — `outline: OutlineState` plus outline initialization and destroy logic. Already partially extracted as `OutlineState` but lifecycle is still on `VulkanRenderer`.
- [ ] **Extract depth prepass into an owned `DepthPrepassSubsystem`** — `depth_prepass: DepthPrepassState` with its own init/destroy.
- [ ] **Extract picking readback into an owned `PickingSubsystem`** — `pending_picking_readback` field plus `picking.rs` impl block.
- [ ] **Simplify `VulkanRenderer::init()` after subsystem extraction** — With owned subsystems, `init()` should drop from ~150 lines to ~50 lines of subsystem construction. Depends on all 5 subsystem extractions above.

### P1: Error Handling

- [ ] **Return `Result` from `VulkanContext::init()`** — Currently panics on any Vulkan failure (no device, driver missing, allocation failure). Return `Result<Self, RendererError>` instead.
- [ ] **Replace `.unwrap()` in `vulkan/context/memory.rs`** — `create_buffer()`, `create_image()`, `map_buffer()`, `free_buffer()`, `free_image()` all unwrap. Return `Result` and propagate.
- [ ] **Replace `.unwrap()` in `vulkan/context/device.rs`** — `create_device()`, `pick_physical_device()`, `enumerate_required_extensions()` all unwrap. Return `Result` and propagate.
- [ ] **Replace `.unwrap()` in `vulkan/swapdata.rs`** — Semaphore and fence creation, `wait_for_fence()` all unwrap. Return `Result` and propagate.
- [ ] **Replace `.unwrap()` in `vulkan/swapchain.rs`** — `create_swapchain()`, `get_swapchain_images()`, surface format selection, physical device queries all unwrap. Return `Result` and propagate.

### P2: Code Duplication / Cleanup

- [ ] **Unify `DrawCall` single-instance and instanced paths** — `model_matrix`, `color`, `metallic`, `roughness`, `ao` duplicate fields in `InstanceData`. Eliminate the flat fields and always use a single `InstanceData` (or `instances: Vec<InstanceData>` with a guaranteed first element).
  - [ ] Remove duplicate flat fields from `DrawCall`
  - [ ] Ensure all `DrawCall` construction sites populate `InstanceData` instead
  - [ ] Update render pass to always use instance path
  - [ ] Verify visual correctness

### P3: Polish

- [ ] **Consider a minimal `Mat4` type within katla_gfx** — `FrameUniforms`, `InstanceData`, `DrawCall` all use `[f32; 16]` with scattered helpers (`compute_distance_from_camera`, `katla_math_proj_reverse_z` in tests). A small `Mat4` newtype with `transpose()` and `translation()` would centralize this without depending on katla_math.
- [ ] **Gate validation examples behind a Cargo feature** — 5 validation examples in `Cargo.toml` (particle, animation, light_shadow, picking, outline) pull in test infrastructure. Consider an `examples` or `validation` feature.

## katla_ui

### P0: Layout Consistency

- [ ] **Pick one layout style as primary** — Crate has both closure-based (`horizontal(|ui| { ... })`) and begin/end (`begin_row()` / `end_row()`). Document which is preferred; consider removing one style.
  - [ ] Audit all call sites using both closure-based and begin/end layout styles
  - [ ] Decide and document which style to keep as primary
  - [ ] Remove the non-primary style and update all consumers

### P1: Performance

- [ ] **Store font bytes without `Box::leak`** — `text/font_loading.rs` leaks font data with `Box::leak(bytes.to_vec().into_boxed_slice())` for `'static` lifetime. Store bytes in `FontSystem` as `Vec<Vec<u8>>` and reference by index.
  - [ ] Add `Vec<Vec<u8>>` storage in `FontSystem` for font byte data
  - [ ] Change font references from `&'static [u8]` to index-based or lifetime-parameterized access
  - [ ] Remove `Box::leak` calls in font loading

### P2: Ergonomics

- [x] **Add `at_cursor()` to all builder widgets** — Button has `at_cursor()` but Checkbox, Slider, TextInput, Label don't. Inconsistent. Add to all or provide a unified auto-layout method.
- [x] **Make cursor advancement consistent** — `button_auto_wide` and `label` advance the cursor. `Collapsible` and `Badge` don't. All widgets drawing at the cursor should advance it, or none should (with clear docs).

### P3: Polish

- [x] **Separate `border_color` concern in Button** — Builder stores `border_color`, drawn in `Widget::ui()` impl rather than `button_with_colors()`. Move border drawing into `button_with_colors()` or a dedicated method.
