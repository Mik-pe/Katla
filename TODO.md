# TODO

## ECS

### P0: Soundness

- [ ] **Fix `query_ref` soundness hole** — `world.rs:234` allows `world.query_ref::<&mut T>()` from `&self`, creating `&mut` without `&mut World` (UB). Add a sealed `ImmutableQuery` marker trait implemented only for immutable patterns (`&T`, `(&T, &U)`, etc.) and bound `query_ref` on it.
- [ ] **Centralize unsafe borrow pattern in queries** — 16+ raw-pointer casts across `iter2..iter8.rs` replicate the same `storage as *mut ComponentStorageManager` / `(*ptr).get_storage_mut::<T>()` pattern. Extract into a single unsafe helper on `ComponentStorageManager` (e.g., `get_two_storage_mut::<T1, T2>() -> (&mut Storage<T1>, &mut Storage<T2>)`) with one consolidated SAFETY comment, then call it from each query impl.

### P1: Architecture

- [ ] **Move `input` module out of `katla_ecs`** — `input/mod.rs`, `input/mouse.rs`, `input/actions.rs` contain `InputState`, `MouseButton`, `Action` (hardcoded game-specific actions like `MoveForward`, `Jump`). None are ECS concepts. Move to `katla_app` where they're consumed, or into a `katla_input` crate. Keep `World::get_input()` working via a generic or trait.
- [ ] **Optimize change detection from O(all_entities * type_ids)** — `storage.rs:collect_changed_entity_ids()` iterates every entity with any component and checks per-type generation lookups. Maintain a per-type `SparseSet<EntityId, ()>` of dirty entity IDs that gets populated on `insert`/`get_mut` and drained on `clear_changed()`. Eliminates the full-scan entirely.
- [ ] **Replace `HashMap` sparse mapping with array-based sparse set** — `SparseSet<K, V>` uses `HashMap<K, usize>` for the sparse array, hashing `EntityId` on every lookup. Since `EntityId` has a dense `u32` index, use `Vec<Option<usize>>` indexed by `EntityId::index()` for true O(1) with zero hashing. Update `SparseSet` to take a key-to-index converter or require `EntityId` keys directly.

### P2: Maintenance

- [ ] **Macro-ify query iterator generation** — `iter1..iter8.rs` total ~2000 lines of nearly identical code with 2^N mutability permutations per arity. Create a declarative macro that generates all permutations for a given arity from a template. Adding a 9th component should be a one-line macro invocation.
- [ ] **Fix `cleanup_empty_entities` missing events** — `world.rs:396` deallocates entities from the allocator but does NOT emit `EntityEvent::Destroyed` or `ComponentEvent::Removed`, breaking the event contract that all other destroy paths follow. Add event emission. Also remove the unnecessary `unsafe` block since the method takes `&mut self`.
- [ ] **Add panic safety to `World::update`** — `world.rs:316` uses `std::mem::take(&mut self.systems)` before the system loop. If any system panics, `self.systems` is left permanently empty. Use a scope guard (or `Drop` impl) that restores systems on panic, or use `std::panic::catch_unwind` per system.

### P3: Polish

- [ ] **Fix doctests** — 10 of 13 doctests are `ignore`d. Convert key examples (World::query, World::spawn, Spawnable) to runnable doctests using `use katla_ecs::*` so they're validated by CI.
- [ ] **Remove redundant `Clone` bound on `SparseSet`** — `sparse_set.rs` requires `K: Hash + Eq + Copy + Clone` but `Copy` implies `Clone`. Drop the redundant `Clone`.
- [ ] **Document `Action::COUNT = 16` padding** — `input/actions.rs` has 14 variants but `COUNT = 16`. Add a brief comment explaining the 2-slot padding.

## Gizmo

### UX

- [ ] Add plane-drag support (e.g., XY, XZ, YZ planes) for translate and scale modes
- [ ] Calibrate scale sensitivity to screen-space movement (magic 0.01 constant is not zoom-aware)

## Outline + Overlay

All outline code quality and refactoring items completed.

## Game Maker API

Bite-sized tasks to make the engine usable by game makers. Ordered by impact and independence.

### P0: Discoverability

- [ ] Add `katla_app::prelude` module re-exporting `ApplicationBuilder`, all components, systems, animation types, `FrameContext`, `AppError`, `AppResult`
- [ ] Make `animation` module types fully public: verify `AnimationPlayer`, `AnimationEvent`, `AnimatedModel`, `Skin`, `Skeleton`, `AnimationClip` are re-exported from `katla_app::animation` (not just `pub(crate)`)

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

- [ ] Reduce `game/src/main.rs` boilerplate: add `Application::run()` or `ApplicationBuilder::run()` that handles `build()`, `init()`, and `event_loop.run_app()` in one call
- [ ] Add `Default` impl for `ApplicationBuilder` so game makers can write `ApplicationBuilder::default().with_name("My Game").run()`
- [ ] Audit all `pub(crate)` items in `katla_app/src/components/` — promote to `pub` anything a game maker would need to query or mutate from systems
