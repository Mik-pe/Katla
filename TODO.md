# TODO

## ECS

### P2: Features

- [x] **Add entity/component removal events** — ECS emits `EntityEvent::Destroyed` and `ComponentEvent::Removed` via `destroy_entity()`, `remove_component()`, and `cleanup_empty_entities()`. Public accessors: `entity_events()`, `component_events()`, `component_events_for<T>()`.

### P3: Polish

- [ ] **Fix doctests** — 150+ doc examples use ` ```ignore ` blocks across the workspace. Convert key examples to runnable doctests.
- [x] **Tighten public API surface** — `ComponentStorage`, `ComponentStorageManager`, `ImmutableQuery`, `QueryData`, `OrderedSystem` are `pub use`'d but never used by external crates. Should be `pub(crate)`.
- [ ] **Narrow `World::storage_mut()` exposure** — Exposes internal `ComponentStorageManager`. Used by `katla_app` camera systems; could be replaced with a narrower API.

## Gizmo

### UX

- [ ] Add plane-drag support (e.g., XY, XZ, YZ planes) for translate and scale modes
- [ ] Calibrate scale sensitivity to screen-space movement (magic 0.01 constant is not zoom-aware)

## katla_gfx

### P2: Features

- [x] **Add billboard rendering support** — Render billboard quads for geometry-less entities (particle emitters, point lights, etc.) so they are visible in the editor viewport. All 9 subtasks complete:
  1. ~~**Extract `ForkAwesome` icon codepoints into `katla_icons` crate**~~ — New zero-dep workspace crate with 100+ icon constants. `katla_ui` re-exports via `pub use katla_icons::ForkAwesome;`
  2. ~~**Create billboard icon texture generation utility**~~ — `billboard_icons` module rasterizes ForkAwesome glyphs via skrifa+vello_cpu into 64x64 RGBA buffers
  3. ~~**Create billboard WGSL shader**~~ — `resources/shaders/billboard.wgsl`: camera-facing quad using view matrix right/up, alpha discard, bindless texture via material_params.w
  4. ~~**Create billboard mesh + material initialization**~~ — Unit plane mesh + alpha-blended material in `BillboardResources` on `Application`, initialized during app init
  5. ~~**Create `BillboardComponent` ECS component**~~ — `BillboardIcon` enum (Lightbulb, Fire) + `BillboardComponent` with icon, color, size
  6. ~~**Create billboard draw call generation**~~ — `collect_billboard_draw_calls()` queries Billboard+Transform entities, uses `compute_gizmo_scale` for constant screen-space size
  7. ~~**Attach `BillboardComponent` on entity spawn**~~ — Lightbulb for PointLight, Fire for ParticleEmitter in scene loading
  8. ~~**Editor-only gating**~~ — All billboard code gated `#[cfg(feature = "editor")]`, skips `EditorHidden` entities
  9. ~~**Upload icon textures and track bindless indices**~~ — Icons rasterized during init, uploaded via `create_texture()`, bindless slot passed via emission param
- [ ] **Make billboard entities pickable via GPU picking** — Billboards are excluded from the depth prepass (which writes object IDs for picking) to avoid rendering a flat gray plane. To make them pickable, either create a billboard-aware depth prepass shader (with camera-facing vertex transform + object ID output) or add billboard draw calls to the depth prepass using a separate pipeline that applies the billboard vertex transform.
  1. ~~**Investigate depth prepass pipeline**~~ — The depth prepass uses a single hardcoded pipeline (`depth_prepass.wgsl`, position-only vertex input, `obj.model * position` transform). Outputs `instance_index + 1` as R32Uint for picking. No per-material pipeline — all draws share one pipeline. Skinned meshes get a second pipeline variant (same shader, different vertex layout + Set 2 for joints). Precedent for multi-pipeline passes exists in `draw_meshes_with_skinning`.
  2. ~~**Investigate per-draw-call pipeline override**~~ — The depth prepass does NOT use per-material pipelines. `execute_draw_list()` (geometry pass) does per-material lookup, but depth/shadow/outline passes use `draw_meshes_with_skinning()` which binds a single pass-level pipeline for all draws. To support billboards in depth prepass, a third pipeline variant must be added to `DepthPrepassSubsystem` and the `draw_meshes_with_skinning` loop must detect billboard draws and bind the billboard pipeline.
  3. **Create `billboard_depth.wgsl`** — Depth + object ID shader with camera-facing vertex transform (same as `billboard.wgsl`: extract right/up from view matrix rows, offset unit quad vertices from `model[3].xyz`). Samples the icon texture from bindless (using `material_params.w` as texture index, same as `billboard.wgsl`) and discards transparent pixels (`alpha < 0.5`) so clicks pass through to geometry behind the icon. Fragment outputs `instance_index + 1` as `vec4u` only where the icon is opaque. PBR vertex layout to match billboard mesh. Requires Set 0 (frame + object uniforms) and Set 1 (bindless textures + sampler) descriptor sets.
  4. **Create billboard depth prepass pipeline** — Add `init_depth_prepass_billboard_pipeline()` to `DepthPrepassSubsystem` (follows existing static/skinned pattern). Uses `billboard_depth.wgsl` shader, PBR vertex layout (to match billboard mesh), back-face culled, reverse-Z, R32Uint + D32SfloatS8Uint output. Unlike static/skinned variants, this pipeline must also include Set 1 descriptor layout (bindless textures) so the shader can sample the icon texture for alpha discard. Double-sided to match the billboard material.
  5. **Integrate billboard draws into depth prepass** — Update `draw_meshes_with_skinning` in `draw_helpers.rs` to detect billboard draw calls (by comparing against billboard material handle) and bind the billboard depth pipeline for those draws. Update `scene_draw_list` split in `renderer.rs` so billboards reach the depth prepass but still skip the shadow pass. Billboards must be excluded from shadow maps to avoid flat plane artifacts.
  6. **Populate `entity_instance_map` for billboard entities** — `collect_billboard_draw_calls()` currently does not insert into `entity_instance_map`. Add `self.entity_instance_map.insert(idx, entity_id)` so `process_picking()` can resolve `instance_index → EntityId` for billboard clicks.

### P1: Stubs / Missing Implementations

- ~~**Implement `create_transient_texture()`**~~ — False positive. No such function exists. Transient textures are managed by `FrameGraph::recreate_transient_textures()` which is fully implemented.

### P2: Robustness

- [ ] **Remove hardcoded compositing viewport layout** — `render_graph/frame/compositing.rs` hardcodes split-screen rects. Should pass viewport rectangles via uniform buffer.
- [x] **Integrate GPU particle timing** — `particles/timing.rs` has a full `TimestampQuery` struct (`#[allow(dead_code)]`) that is implemented but never used.

### P0: Visibility Tightening

- ~~**Change `animation` module to `pub(crate) mod`**~~ — Module visibility done (`pub(crate)` by default, `pub` with `validation` feature). Re-exports (`AnimChannelInfo`, `AnimClipHeader`, `JointInfo`, `SkeletonAnimParams`, `PoseComputeBuffers`, `PoseComputePipeline`) are unconditional because `katla_app` consumes them directly.
- ~~**Change `shadow` module to `pub(crate) mod`**~~ — Module visibility done. `CascadeParams` re-export is unconditional because `katla_app::builder` consumes it.
- ~~**Change `lighting` module to `pub(crate) mod`**~~ — Module visibility done. `PointLightGPU` re-export is unconditional because `katla_app::renderer` consumes it.
- [x] **Make `VkImageView` field private** — `sync.rs:176` exposes `pub struct VkImageView(pub vk::ImageView)`. The inner field should be private with an accessor method to prevent constructing invalid views. (Note: feature-gated, low priority.)
- ~~**Tighten `pub` on vulkan/ submodules**~~ — False positive. The `vulkan` module is `pub(crate) mod vulkan` in `lib.rs`, so internal `pub` items are already crate-scoped. No action needed.
- ~~**Make `PipelineHandle::new()` pub(crate)**~~ — Verified that `Handle::new()` has legitimate external callers (e.g., `katla_app/src/ui/renderer.rs:158` creates `TextureHandle::new(bindless_index)`). Keep as-is; audit test-only callers instead.
- ~~**Audit feature-flag-gated module visibility**~~ — Intentional design for validation examples/tests. No action needed.

### P1: Robustness

- [ ] **Replace `.expect()` calls in per-frame hot paths** — 65 `.expect()` calls across 27 files (verified count, not 80+). `.expect()` in init/setup code is acceptable in Rust graphics engines. Focus on per-frame rendering paths (`renderer/mod.rs`, `render_graph/frame/`) and cleanup code where `.expect()` → `Result` propagation would prevent crashes.
- [x] **Decompose `GlobalParticleSystem` struct (30 fields)** — Grouped into `ParticlePipelines` (4), `ParticleDescriptors` (11), `ParticleBuffers` (2), `ParticleEmitterPool` (4) sub-structs.
- [ ] **Refactor `VulkanContext.allocator` away from `ManuallyDrop<RefCell<Allocator>>`** — `vulkan/context/mod.rs:99`. The `try_borrow_mut()` pattern silently leaks memory on failure (e.g., `particles/mod.rs:246`). Consider `Mutex<Allocator>` or restructuring for single-threaded access by design.

### P2: Code Reusability

- [ ] **Consolidate image creation into shared helper** — `OutputRenderTarget::new()`, `Texture::create_image()`, `TransientTexture`, swapchain all construct `vk::ImageCreateInfo` then delegate to `context.create_image()`. Differences in usage flags are legitimate, but an `ImageCreateInfoBuilder` could reduce boilerplate. Low priority.
- [x] **Extract repeated pipeline lookup into `AssetRegistry` method** — 21 occurrences (verified) across `render_graph/frame/` repeat: `material.pipeline.ok_or(...)` then `registry.get_pipeline_vk_handles(...).ok_or(...)`. A `get_pipeline_or_err(handle) -> Result<_, RenderGraphError>` method would eliminate significant boilerplate.
- [x] **Extract `DrawCall` builder helper** — `renderer/types.rs:204-250` — 6 builder methods all repeat the same `if let Some(inst) = self.instances.first_mut()` guard. Extract a private `with_first_instance_mut()` helper. Minor refactor, low risk.

### P3: Polish

- [ ] **Consider a minimal `Mat4` type within katla_gfx** — `FrameUniforms` in `renderer/types.rs:18-20` uses raw `[f32; 16]` arrays. Low priority: the raw arrays match GPU memory layout and are constructed from `katla_math` in `katla_app`, so a local Mat4 would create yet another conversion boundary.
- ~~**Extract viewport/UI from renderer module**~~ — Viewport and UI logic extracted into `viewport_manager.rs` and `ui_renderer.rs` submodules. Only struct composition remains in `mod.rs`, which is appropriate.
- ~~**Clean up dead code**~~ — Stale. `ShadowBuffers::len()/is_empty()` already removed. `CascadeParams::cascades()` has a test caller. `MaterialBuilder::with_push_constant_range()` (actually `PipelineBuilder`) has an example caller. No dead code to remove.
- [x] **Chain errors in `RendererError::source()`** — `error.rs` — `source()` returns `None` for `VulkanError`/`IoError` variants because they convert to `String`, losing the original error. Change `IoError(String)` to `IoError(io::Error)` etc. to preserve error chains for debugging.

## katla_app

### P0: Structural Debt

- [ ] **Split `application/mod.rs` monolith (1928 lines)** — Partially decomposed already (`builder.rs` 846 lines, `spawning.rs` 608 lines, `renderer.rs` 530 lines, etc. exist). The remaining `mod.rs` holds the core `Application` struct, `ApplicationHandler` impl (event loop), window events, frame orchestration, input routing, cleanup, hot reload, and timing. Extract event handling, input routing, and frame orchestration into focused submodules.
- [ ] **Decompose `Application` god struct (39 fields)** — Verified count is 39 fields. Many are cfg-gated editor fields. Group the 7 editor-specific cfg-gated fields (gizmo_state, gizmo_resources, prev_mouse_screen, entity_instance_map, entity_to_instance_indices, pending_pick, stencil_indicator_bindless_index) into an `EditorState` sub-struct behind a single `#[cfg(feature = "editor")]`. This also reduces cfg sprinkling. Particle readback flags could similarly be grouped.

### P1: Stubs / Missing Implementations

- [ ] **Implement `EditorAction::DuplicateEntity`** — Currently logs "not yet implemented" (`editor/mod.rs`). Entity duplication with all components is a stub.
- [ ] **Implement `EditorAction::ResetParticleSystem`** — Currently logs "not yet implemented" (`editor/mod.rs`). Particle system reset is a stub.

### P2: Robustness

- [ ] **Reduce `unwrap()` in physics systems** — `physics_system.rs` and `velocity_system.rs` have 17 `unwrap()` calls on ECS component queries that will panic if components are missing.
- [ ] **Guard `TransformOptimization` resource access** — `transform_hierarchy_system.rs` calls `unwrap()` on `get_resource_mut::<TransformOptimization>()` which panics if not inserted.
- ~~**Guard GLTF bone mapping**~~ — False positive. The only `unwrap()` on `transforms.get()` is in test code (line 306). Production code at line 228 already uses `if let Some()` guard.
- [ ] **Guard asset browser edge cases** — `asset_browser/mod.rs` has `unwrap()` on `drag_asset`, `parent()`, `selected_index` that could panic on edge cases.

### P2: Code Reusability

- [x] **Extract shared transform-from-position pattern** — 44 occurrences total but 24 are in tests. ~15 in production code across 5 files (`spawner.rs`, `scene/mod.rs`, `spawning.rs`, `physics_system.rs`, `velocity_system.rs`). Low priority: `Transform::new_from_position()` already exists; the repetition is the `TransformComponent { transform: ... }` wrapper. A `TransformComponent::from_position()` convenience method would help.
- ~~**Consolidate `Spawner` and `Application` entity creation**~~ — The two paths serve intentionally different access patterns. `Spawner` is a `World` extension trait for system-level access; `Application` spawning handles GPU resource tracking at the editor level. Consolidation is not desirable.
- [x] **Replace hand-rolled TOML parsers with serde** — `Preferences` and `GuiState` now use serde derive with `toml::from_str`/`to_string_pretty`. Added `#[serde(default)]` for forward-compatible partial configs.

### P2: Architecture

- [ ] **Reduce `#[cfg(feature = "editor")]` sprinkling** — 84+ occurrences across 7 files (`application/mod.rs` ~40, `builder.rs` ~17, `renderer.rs` ~11, `ui/mod.rs` 8). For struct fields, per-field cfg is idiomatic. For function bodies and methods, extract editor-specific code into the existing `application/editor/` module. Grouping editor fields into `EditorState` sub-struct (P0-2) would also help.

### P3: Polish

- ~~**Remove dead `DragToViewport.path` field**~~ — False positive. `DragToViewport.path` is actively used: set in `asset_browser/mod.rs:650-654` and read in `editor_ui.rs:852-861` for `EditorAction::SpawnModelAtPath`. Not dead code.
- [ ] **Split `scene/mod.rs` (3998 lines)** — Largest file in katla_app with no submodules. Scene serialization/deserialization, entity instantiation, GLTF loading, and scene management all in one file. High priority for maintainability — decompose into `scene/serialization.rs`, `scene/loader.rs`, `scene/instantiation.rs`, etc.
- [ ] **Split `editor_ui.rs` (1334 lines)** — Decomposition already in progress: `editor_ui/` subdirectory exists with `hierarchy.rs` (383 lines), `inspector.rs` (353 lines), `preferences.rs` (559 lines), `status_bar.rs` (117 lines), `toolbar.rs` (173 lines), `viewport_grid.rs` (262 lines), `asset_browser/`. Remaining code is core panel layout/orchestration. Low priority.
- [x] **Remove unused `Selection` resource** — 320 lines of dead code in `resources/selection.rs` with full tests and API, but never imported, registered, or used outside its own file. Either integrate into editor flow or remove.
- ~~**Audit stateless service structs**~~ — False positive. Stateless system structs (`ParticleSystem {}`, `VelocitySystem`, `PhysicsSystem`, `OrbitCameraSystem`, etc.) are idiomatic Rust ECS pattern. The struct is a type token for trait dispatch; state lives in `World`. No action needed.

## katla_ui

### P1: Correctness

- [ ] **Unify click-handling logic across widgets** — `button_with_colors()` intentionally deviates from `click_behavior()` to bypass popup blocking (documented in code). `menu_bar_dropdown()` uses its own inline check. Consider a `click_behavior_popup_aware()` variant for button.
- [x] **Fix `label_auto_colored()` cursor corruption** — `context/widgets.rs:136` manually advances `self.cursor` instead of using `advance_cursor()`, bypassing layout stack awareness. Verified: if called inside a row/column layout, it corrupts cursor state. Should use `advance_cursor()` like `label()` in `helpers.rs` does.
- [x] **Fix `button_auto_wide` layout bypass** — `context/widgets.rs:115-133` manually sets cursor. Verified: has a double-advance bug since `self.add()` already calls `advance_cursor()` internally, causing cursor to skip ahead in layouts.

### P2: Robustness

- ~~**Guard `DrawList::finalize()` on empty lists**~~ — False positive. `finalize()` uses `unwrap_or(0)` and safe iterators — already handles empty lists correctly. The 8 `unwrap()` calls are in test assertions only.
- [x] **Fix `Vertex.texture_index` always being 0** — `types.rs:62-88` — every vertex is created with `texture_index: 0` and comments say "Will be set during batch conversion" but `finalize()` never resolves it. Either remove the field from the public struct or resolve within `finalize()`.

### P2: Visibility

- [ ] **Make `UiContext` fields `pub(crate)`** — `context/mod.rs:63-67` exposes `pub input`, `pub style`, `pub fonts`. These fields need external access from `katla_app` (20+ access sites). Tightening would require accessor methods or moving the access patterns.
- [x] **Make `LayoutState` `pub(crate)`** — `context/layout.rs:22-36`. Verified: internal layout detail. However, some may be re-exported — check before changing.
- [x] **Make `FontId` inner field `pub(crate)`** — `text/mod.rs:50`. Low priority: `FontId(pub u32)` allows arbitrary construction but this is standard for simple ID types.

### P2: Code Reusability

- [ ] **Fix hierarchy view to use a reusable list view** — Each list element in the hierarchy panel is fixed to different pixel sizes, causing elements to jump around when scrolling. Should build a reusable `ListView` widget in `katla_ui` with uniform row heights and virtualized scrolling.
- [x] **Extract shared text/icon centering utility** — Three separate implementations in `context/drawing.rs:240-265`, `context/drawing.rs:270-305`, `context/widgets/basic.rs:66-72` each compute centering slightly differently. Should be a shared helper.
- [x] **Reduce theme method repetition** — Extracted `pub struct ColorScheme` with all 42 color fields. `dark()`/`light()`/`classic()` now one-liners via `UiStyle::with_colors(ColorScheme::dark())`.
- [x] **Introduce `DraggablePanel::show()` config struct** — `widgets/draggable_panel.rs:99-105` takes 9 parameters with `#[allow(clippy::too_many_arguments)]`. Use a builder or config struct consistent with the crate's widget patterns.

### P3: Polish

- [x] **Rename `context/widgets/selectable.rs` to better reflect contents** — File contains `toggle_button()` which is actively called by `ToggleButton` widget in `widgets/mod.rs:369`. The filename `selectable.rs` is misleading — there is no `selectable()` method. Consider renaming to `toggle_button.rs` or merging into another widget file. The `selectable_selected`/`selectable_hovered` style fields are used.
- [x] **Remove or deprecate `spacer()` in favor of `spacing()`** — `context/layout.rs:92-106` — `spacer()` always advances horizontally regardless of layout direction, while `spacing()` is direction-aware. The doc says "prefer spacing()" but both are `pub`.
- [x] **Document `end_column()` vs `end_row()` spacing asymmetry** — `context/layout.rs:228-231` — `end_column()` adds trailing spacing but `end_row()` doesn't. Undocumented and surprising.
- [x] **Use `KeyCode::Backspace` instead of `\x08` character check** — `context/widgets/basic.rs:256-258` checks `c == '\x08'` for backspace but `KeyCode` enum has a `Backspace` variant. Conflates character input with key events.
- [ ] **Remove hardcoded widget default sizes** — `Button` 100x30, `Checkbox` 150x24, `Slider` 150x20, `TextInput` 200x24, etc. don't relate to `UiStyle` dimensions. `at_cursor()` methods use style values but defaults ignore them.
- [x] **Fix `Separator` hardcoded 200.0 width** — `widgets/mod.rs:673-704` — no way to make it span full container width without caller computing manually.
- [x] **Replace per-frame `Vec<Vec2>` allocation in `graph()`** — `context/widgets/graph.rs:53-65` allocates every frame. Use a scratch buffer approach like `DrawList` uses for circles.
- [x] **Make `property_row()` label width configurable** — `context/helpers.rs:20-42` hardcodes `60.0` for label column width. Doesn't scale with font size or content length.

### P3: Font Library Migration (ab_glyph → skrifa + vello_cpu) — DONE

Follow egui's approach: use `skrifa` for font parsing/outlining and `vello_cpu` for rasterization. Only 4 files needed changes; atlas and drawing code were unaffected.

- [x] **Add `skrifa` + `vello_cpu` dependencies** to `katla_ui/Cargo.toml` (also `kurbo` for Bézier paths)
- [x] **Replace `FontArc` storage with skrifa font types** — `text/mod.rs` stores `Arc<Vec<u8>>` in `HashMap<FontId, Arc<Vec<u8>>>`; constructs `skrifa::FontRef` on demand
- [x] **Migrate font loading** — `text/font_loading.rs` uses `FontRef::new(&data)` for validation
- [x] **Migrate text measurement** — `text/measurement.rs` uses `skrifa::MetadataProvider` equivalents
- [x] **Migrate glyph ID lookup** — `font.charmap().map(c)` in `rasterization.rs` and `measurement.rs`
- [x] **Migrate kerning** — stubbed to return 0.0; `// TODO: Add GPOS kerning support via skrifa's GPOS table access`
- [x] **Rewrite glyph rasterization** — `text/rasterization.rs` uses vello_cpu scene rendering (outline → `kurbo::BezPath` via `KurboPen` → `vello_cpu::RenderContext` → pixel buffer)
- ~~**Adopt egui's subpixel quantization**~~ — 4-bin Latin subpixel quantization already implemented (`SubpixelBin` enum in `text/mod.rs`, integrated into glyph cache key and rasterization). CJK 1-bin optimization not yet done — could be a separate lower-priority item.
- [x] **Verify atlas integration** — `RasterizedGlyph` output and `place_in_atlas` work unchanged
- [x] **Remove `ab_glyph` dependency** — from `katla_ui/Cargo.toml` and workspace `Cargo.toml`

## katla_math

### P3: Polish

- [x] **Decide fate of scalar quaternion module** — `scalar::quat` module is `#[allow(dead_code)]` on x86/x86_64 (primary targets). Either gate behind cfg or remove.

## Cross-Cutting

### P3: Cleanup

- [ ] **Audit `#[allow(clippy::too_many_arguments)]`** — 17 functions suppress this lint (down from ~20). Consider introducing parameter structs for functions with many arguments.

### P3: Dependency Hygiene

- [x] **Upgrade skrifa to latest and deduplicate** — Upgraded to `skrifa 0.40`, replaced custom `BoundsPen` with `ControlBoundsPen`. Single version in lockfile, no duplicates.
- [ ] **Pool `vello_cpu::RenderContext` for CJK workloads** — Currently a fresh `RenderContext` + `Pixmap` is allocated per glyph cache miss. Acceptable for pre-cached ASCII but wasteful for runtime CJK input. Reuse a shared context or pool buffers.
