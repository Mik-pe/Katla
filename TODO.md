# TODO

## ECS

### P0: Soundness

- [ ] **Fix `query_ref` soundness hole** — `world.rs:234` allows `world.query_ref::<&mut T>()` from `&self`, creating `&mut` without `&mut World` (UB). Add a sealed `ImmutableQuery` marker trait implemented only for immutable patterns (`&T`, `(&T, &U)`, etc.) and bound `query_ref` on it.
- [ ] **Centralize unsafe borrow pattern in queries** — 16+ raw-pointer casts across `iter2..iter8.rs` replicate the same `storage as *mut ComponentStorageManager` / `(*ptr).get_storage_mut::<T>()` pattern. Extract into a single unsafe helper on `ComponentStorageManager` (e.g., `get_two_storage_mut::<T1, T2>() -> (&mut Storage<T1>, &mut Storage<T2>)`) with one consolidated SAFETY comment, then call it from each query impl.
  - [ ] Add `get_two_storage_mut::<T1, T2>()` unsafe helper on `ComponentStorageManager` with consolidated SAFETY comment
  - [ ] Update `iter2.rs` through `iter8.rs` to use the new helper, removing raw-pointer casts

### P1: Architecture

- [ ] **Move `input` module out of `katla_ecs`** — `input/mod.rs`, `input/mouse.rs`, `input/actions.rs` contain `InputState`, `MouseButton`, `Action` (hardcoded game-specific actions like `MoveForward`, `Jump`). None are ECS concepts. Move to `katla_app` where they're consumed, or into a `katla_input` crate. Keep `World::get_input()` working via a generic or trait.
- [ ] **Optimize change detection from O(all_entities * type_ids)** — `storage.rs:collect_changed_entity_ids()` iterates every entity with any component and checks per-type generation lookups. Maintain a per-type `SparseSet<EntityId, ()>` of dirty entity IDs that gets populated on `insert`/`get_mut` and drained on `clear_changed()`. Eliminates the full-scan entirely.
- [ ] **Replace `HashMap` sparse mapping with array-based sparse set** — `SparseSet<K, V>` uses `HashMap<K, usize>` for the sparse array, hashing `EntityId` on every lookup. Since `EntityId` has a dense `u32` index, use `Vec<Option<usize>>` indexed by `EntityId::index()` for true O(1) with zero hashing. Update `SparseSet` to take a key-to-index converter or require `EntityId` keys directly.

### P2: Maintenance

- [ ] **Macro-ify query iterator generation** — `iter1..iter8.rs` total ~2000 lines of nearly identical code with 2^N mutability permutations per arity. Create a declarative macro that generates all permutations for a given arity from a template. Adding a 9th component should be a one-line macro invocation.
- [x] **Fix `cleanup_empty_entities` missing events** — `world.rs:396` deallocates entities from the allocator but does NOT emit `EntityEvent::Destroyed` or `ComponentEvent::Removed`, breaking the event contract that all other destroy paths follow. Add event emission. Also remove the unnecessary `unsafe` block since the method takes `&mut self`.
- [x] **Add panic safety to `World::update`** — `world.rs:316` uses `std::mem::take(&mut self.systems)` before the system loop. If any system panics, `self.systems` is left permanently empty. Use a scope guard (or `Drop` impl) that restores systems on panic, or use `std::panic::catch_unwind` per system.

### P3: Polish

- [ ] **Fix doctests** — 10 of 13 doctests are `ignore`d. Convert key examples (World::query, World::spawn, Spawnable) to runnable doctests using `use katla_ecs::*` so they're validated by CI.
- [x] **Remove redundant `Clone` bound on `SparseSet`** — `sparse_set.rs` requires `K: Hash + Eq + Copy + Clone` but `Copy` implies `Clone`. Drop the redundant `Clone`.
- [x] **Document `Action::COUNT = 16` padding** — `input/actions.rs` has 14 variants but `COUNT = 16`. Add a brief comment explaining the 2-slot padding.

## Gizmo

### UX

- [ ] Add plane-drag support (e.g., XY, XZ, YZ planes) for translate and scale modes
- [ ] Calibrate scale sensitivity to screen-space movement (magic 0.01 constant is not zoom-aware)

## Outline + Overlay

All outline code quality and refactoring items completed.

## Game Maker API

Bite-sized tasks to make the engine usable by game makers. Ordered by impact and independence.

### P0: Discoverability

- [x] Add `katla_app::prelude` module re-exporting `ApplicationBuilder`, all components, systems, animation types, `FrameContext`, `AppError`, `AppResult`
- [x] Make `animation` module types fully public: verify `AnimationPlayer`, `AnimationEvent`, `AnimatedModel`, `Skin`, `Skeleton`, `AnimationClip` are re-exported from `katla_app::animation` (not just `pub(crate)`)

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

- [x] Reduce `game/src/main.rs` boilerplate: add `Application::run()` or `ApplicationBuilder::run()` that handles `build()`, `init()`, and `event_loop.run_app()` in one call
- [x] Add `Default` impl for `ApplicationBuilder` so game makers can write `ApplicationBuilder::default().with_name("My Game").run()`
- [ ] Audit all `pub(crate)` items in `katla_app/src/components/` — promote to `pub` anything a game maker would need to query or mutate from systems

## katla_gfx

### P0: Visibility Tightening

- [x] **Change `renderer/` submodules to `pub(crate) mod`** — All 22 submodules (`animation_init`, `bindless_queries`, `compositing`, `depth_prepass`, `destroy_api`, `font_atlas`, `frame_lifecycle`, `fullscreen_shader`, `light_culling`, `material_api`, `mesh_manager`, `outline`, `particle_init`, `picking`, `readback`, `registry`, `shadow`, `skeleton_api`, `texture_api`, `types`, `ui_renderer`, `viewport_manager`) are `pub mod` but are implementation details. Change all to `pub(crate) mod`. Re-export only `AssetRegistry`, `DrawList`, `DrawCall`, `UIDrawList`, `UiDrawCommand`, `FrameUniforms`, `InstanceData`, `VulkanRenderer` from `renderer::types` via `lib.rs`.
- [ ] **Change `sync` module to `pub(crate) mod`** — `sync.rs` exposes raw Vulkan wrapper types (`VkSemaphore`, `VkFence`, `VkPipeline`, etc.) to the entire workspace. Make it `pub(crate) mod` and re-export only `ShaderStages` (already re-exported from `lib.rs` via `pipeline_state`). Note: kept `pub` for now because validation examples use `katla_gfx::sync::*` via the external crate path. Gate examples behind a Cargo feature first.
- [ ] **Change `animation` module to `pub(crate) mod`** — Exposes `PoseComputePipeline` and `PoseComputeBuffers` (GPU compute internals). Keep `pub(crate) mod animation` and re-export only the data types: `AnimChannelInfo`, `AnimClipHeader`, `JointInfo`, `SkeletonAnimParams`. Note: kept `pub` for now because validation examples and katla_app use `katla_gfx::animation::*` via the external crate path.
- [ ] **Change `shadow` module to `pub(crate) mod`** — `ShadowBuffers` and `CascadeShadowMapper` are internal GPU subsystems. Make `pub(crate)` and re-export nothing (app layer interacts through `VulkanRenderer` methods and `FrameUniforms`). Note: kept `pub` for now because validation examples use `katla_gfx::shadow::*`.
- [ ] **Change `lighting` module to `pub(crate) mod`** — `PointLightGPU`, `LightCullFrameData`, `LightCullingBuffers` are internal. Make `pub(crate)` and re-export nothing. Note: kept `pub` for now because validation examples and katla_app use `katla_gfx::lighting::*`.
- [x] **Change `vulkan/` submodules from `pub mod` to `pub(crate) mod`** — `vulkan/mod.rs` declares all 17 submodules as `pub mod`. Since `vulkan` itself is `pub(crate)`, this is technically contained but misleading. Change inner modules to `pub(crate) mod` or plain `mod` to match actual intent.
- [x] **Change `particles/` submodules from `pub mod` to `pub(crate) mod`** — `buffer`, `debug_readback`, `descriptors`, `dispatch`, `pipeline`, etc. are all `pub mod` within a public module. Only `EmitterConfig` and `GlobalParticleSystem` need to be public. Make submodules `pub(crate) mod`.
- [x] **Make `OutputRenderTarget` `pub(crate)`** — Currently `pub` but takes `Rc<VulkanContext>` and raw `vk` types. Only used internally by `VulkanRenderer::init_output_target()`.

### P0: VulkanRenderer Decomposition

- [ ] **Extract `ShadowState` fields into an owned `ShadowSubsystem` struct** — `VulkanRenderer` has ~10 shadow-related fields (`shadow: ShadowState`, plus cascade descriptor pools/layouts/sets/buffers/allocations/mapped ptrs spread across `shadow.rs`). Move into a `ShadowSubsystem` with `init()` and `destroy()` methods.
- [ ] **Extract light culling into an owned `LightSubsystem` struct** — `light_culling: LightCullingState` plus `resize_light_culling()` logic. Move into a `LightSubsystem` with `init()`, `destroy()`, and `resize()` methods.
- [ ] **Extract outline state into an owned `OutlineSubsystem` struct** — `outline: OutlineState` plus outline initialization and destroy logic. Already partially extracted as `OutlineState` but lifecycle is still on `VulkanRenderer`.
- [ ] **Extract depth prepass into an owned `DepthPrepassSubsystem`** — `depth_prepass: DepthPrepassState` with its own init/destroy.
- [ ] **Extract picking readback into an owned `PickingSubsystem`** — `pending_picking_readback` field plus `picking.rs` impl block.
- [ ] **Simplify `VulkanRenderer::init()` after subsystem extraction** — With owned subsystems, `init()` should drop from ~150 lines to ~50 lines of subsystem construction.

### P1: Error Handling

- [ ] **Return `Result` from `VulkanContext::init()`** — Currently panics on any Vulkan failure (no device, driver missing, allocation failure). Return `Result<Self, RendererError>` instead.
- [ ] **Replace `.unwrap()` in `vulkan/context/memory.rs`** — `create_buffer()`, `create_image()`, `map_buffer()`, `free_buffer()`, `free_image()` all unwrap. Return `Result` and propagate.
- [ ] **Replace `.unwrap()` in `vulkan/context/device.rs`** — `create_device()`, `pick_physical_device()`, `enumerate_required_extensions()` all unwrap. Return `Result` and propagate.
- [ ] **Replace `.unwrap()` in `vulkan/swapdata.rs`** — Semaphore and fence creation, `wait_for_fence()` all unwrap. Return `Result` and propagate.
- [ ] **Replace `.unwrap()` in `vulkan/swapchain.rs`** — `create_swapchain()`, `get_swapchain_images()`, surface format selection, physical device queries all unwrap. Return `Result` and propagate.

### P2: Code Duplication / Cleanup

- [x] **Remove `DrawList::sort_optimal()`** — Identical to `sort()`. Both do `sort_by_key(|d| d.sort_key.unwrap_or(u64::MAX))`. Keep `sort()` only. Already removed.
- [ ] **Unify `DrawCall` single-instance and instanced paths** — `model_matrix`, `color`, `metallic`, `roughness`, `ao` duplicate fields in `InstanceData`. Eliminate the flat fields and always use a single `InstanceData` (or `instances: Vec<InstanceData>` with a guaranteed first element).
- [ ] **Fix duplicate step comment in `VulkanRenderer::render()`** — Steps 10 (Present) and 11 (Advance) are already correctly numbered. No action needed.

### P3: Polish

- [ ] **Consider a minimal `Mat4` type within katla_gfx** — `FrameUniforms`, `InstanceData`, `DrawCall` all use `[f32; 16]` with scattered helpers (`compute_distance_from_camera`, `katla_math_proj_reverse_z` in tests). A small `Mat4` newtype with `transpose()` and `translation()` would centralize this without depending on katla_math.
- [x] **Add `#[inline]` to hot-path `ResourceStorage` methods** — `get()`, `get_mut()`, `insert()` are called per-draw-call but lack `#[inline]`.
- [ ] **Gate validation examples behind a Cargo feature** — 5 validation examples in `Cargo.toml` (particle, animation, light_shadow, picking, outline) pull in test infrastructure. Consider an `examples` or `validation` feature.

## katla_ui

### P0: Hybrid Elimination

- [x] **Remove `text_label()` alias** — `helpers.rs` has `text_label()` as a pure alias for `label()`. One way to add a label, not two.
- [ ] **Pick one layout style as primary** — Crate has both closure-based (`horizontal(|ui| { ... })`) and begin/end (`begin_row()` / `end_row()`). Document which is preferred; consider removing one style.
- [ ] **Merge `spacer()` / `spacing()` / `advance_cursor()`** — Three methods that all move the cursor with overlapping behavior. `spacer()` and `spacing()` behave identically when no layout is active. Pick one user-facing API, make the others internal or remove.
- [ ] **Reconcile `tooltip()` with `PopupStyle::Tooltip`** — `utility.rs::tooltip()` draws immediately at mouse position. `PopupStyle::Tooltip` exists in the popup system but is unused. Either make `tooltip()` use the popup system for z-ordering, or remove `PopupStyle::Tooltip`.

### P0: Correctness

- [ ] **Fix Slider/TextInput default IDs** — Both fall back to constant strings (`"slider"`, `"text_input"`), meaning two instances on the same frame share an ID and break interaction. Derive unique IDs from label+counter or require explicit IDs.
- [x] **Make `toggle_button()` `pub(crate)`** — `selectable.rs` exposes `toggle_button` as fully `pub` while all other widget internals are `pub(crate)`. Either make it `pub(crate)` or add a proper builder in `widgets/mod.rs`.
- [ ] **Set clip once per `draw_text()` call** — `drawing.rs` calls `set_clip()` inside the per-glyph loop. Clip doesn't change between glyphs in the same text call. Move `set_clip()` before the loop.

### P1: Performance

- [ ] **Eliminate per-frame `String` allocations in `Popup`** — `Popup.id` is `String` but every call site passes `&str`. The `id: impl Into<String>` forces allocation. Use `&'a str` with a lifetime or `Cow<'a, str>`.
- [x] **Remove `format!()` in `ScrollArea` scrollbar ID** — `scroll_area.rs` allocates `format!("{}_scrollbar", id)` every frame. Use a compound label approach in `generate_id` that doesn't allocate.
- [x] **Remove `format!()` in `DraggablePanel` close button** — `draggable_panel.rs` allocates `format!("close_{}", id)` every frame. Same fix as ScrollArea.
- [x] **Reuse `commands` Vec in `DrawList::finalize()`** — `draw_list.rs` replaces `self.commands` with a new Vec via `.collect()` every frame. Use `clear()` + `extend()` to reuse the allocation. Already uses `clear()` + `extend()`.
- [ ] **Store font bytes without `Box::leak`** — `font_loading.rs` leaks font data with `Box::leak(bytes.to_vec().into_boxed_slice())` for `'static` lifetime. Store bytes in `FontSystem` as `Vec<Vec<u8>>` and reference by index.

### P2: Ergonomics

- [ ] **Add `at_cursor()` to all builder widgets** — Button has `at_cursor()` but Checkbox, Slider, TextInput, Label don't. Inconsistent. Add to all or provide a unified auto-layout method.
- [ ] **Make cursor advancement consistent** — `button_auto_wide` and `label` advance the cursor. `Collapsible` and `Badge` don't. All widgets drawing at the cursor should advance it, or none should (with clear docs).
- [x] **Fix `draw_icon_centered` to use glyph metrics** — `drawing.rs` measures text advance width to center icons. For icon glyphs, advance width differs from visual bounds. Use `get_or_rasterize` and the glyph's actual size. Already uses `get_or_rasterize` and glyph size for centering.

### P3: Polish

- [x] **Remove dead `id` parameter in `graph()`** — `graph.rs` silences it with `let _ = id;`. Either use it or remove it.
- [x] **Fix `Button.width()` / `Button.height()` losing position** — `width()` and `height()` builders use `Rect2D::from_size()` which resets position to origin. Preserve the existing position: `Rect2D::from_origin_size(self.bounds.min, ...)`.
- [ ] **Separate `border_color` concern in Button** — Builder stores `border_color`, drawn in `Widget::ui()` impl rather than `button_with_colors()`. Move border drawing into `button_with_colors()` or a dedicated method.
