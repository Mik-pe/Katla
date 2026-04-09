# TODO

> **Note:** Investigation items (marked with "Investigate", "Consider", or "Design") are research tasks. Each investigation should produce concrete follow-up TODO items with clear scope and priority once the research is complete.

## ECS

### P2: Features

- [x] **Add entity/component removal events** — ECS emits `EntityEvent::Destroyed` and `ComponentEvent::Removed` via `destroy_entity()`, `remove_component()`, and `cleanup_empty_entities()`. Public accessors: `entity_events()`, `component_events()`, `component_events_for<T>()`.

### P3: Polish

- [x] **Fix doctests** — 150+ doc examples use ` ```ignore ` blocks across the workspace. Convert key examples to runnable doctests. (11 practically runnable doctests converted: 5 in katla_ecs, 6 in katla_app particle.rs)
- [x] **Tighten public API surface** — `ComponentStorage`, `ComponentStorageManager`, `ImmutableQuery`, `QueryData`, `OrderedSystem` are `pub use`'d but never used by external crates. Should be `pub(crate)`.
- [x] **Narrow `World::storage_mut()` exposure** — Exposes internal `ComponentStorageManager`. Used by `katla_app` camera systems; could be replaced with a narrower API. (Removed; callers replaced with direct query/get_component_mut)

## Gizmo

### UX

- [x] **Add plane-drag support (XY, XZ, YZ planes) for translate and scale modes** — GizmoHandle enum unifies axis/plane interaction. Plane handles rendered as semi-transparent quads at origin corners. Plane hit testing with axis-priority. Two-axis translate and scale via compute_translate_plane_delta/compute_scale_plane_delta.
- [x] **Calibrate scale sensitivity to screen-space movement** — Replaced hardcoded `0.01` fallback with zoom-aware `1.0 / (gizmo_scale * 5.0)` derived from camera distance, FOV, and viewport height.

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
- [x] **Make billboard entities pickable via GPU picking** — Billboard depth prepass shader + pipeline created, billboard draws integrated into depth prepass, entity_instance_map populated. Click-through on transparent pixels works.

### P2: Robustness

- [x] **Remove hardcoded compositing viewport layout** — `render_graph/frame/compositing.rs` hardcodes split-screen rects. Should pass viewport rectangles via uniform buffer. (Done: CompositingUniforms with ViewportRect now passed via uniform buffer)
- [x] **Integrate GPU particle timing** — `particles/timing.rs` has a full `TimestampQuery` struct (`#[allow(dead_code)]`) that is implemented but never used.

### P1: Robustness

- [x] **Replace `.expect()` calls in per-frame hot paths** — 65 `.expect()` calls across 27 files (verified count, not 80+). `.expect()` in init/setup code is acceptable in Rust graphics engines. Focus on per-frame rendering paths (`renderer/mod.rs`, `render_graph/frame/`) and cleanup code where `.expect()` → `Result` propagation would prevent crashes. (Done: ~21 per-frame .expect() calls replaced with Result propagation)
- [x] **Decompose `GlobalParticleSystem` struct (30 fields)** — Grouped into `ParticlePipelines` (4), `ParticleDescriptors` (11), `ParticleBuffers` (2), `ParticleEmitterPool` (4) sub-structs.
- [x] **Refactor `VulkanContext.allocator` away from `ManuallyDrop<RefCell<Allocator>>`** — `vulkan/context/mod.rs:99`. The `try_borrow_mut()` pattern silently leaks memory on failure (e.g., `particles/mod.rs:246`). Consider `Mutex<Allocator>` or restructuring for single-threaded access by design. (Done: GpuAllocator wrapper with warning on borrow conflict)

### P2: Code Reusability

- ~~**Consolidate image creation into shared helper**~~ — N/A: Already consolidated around `VulkanContext::create_image()`. Remaining differences in usage flags are legitimate.
- [x] **Extract repeated pipeline lookup into `AssetRegistry` method** — 21 occurrences (verified) across `render_graph/frame/` repeat: `material.pipeline.ok_or(...)` then `registry.get_pipeline_vk_handles(...).ok_or(...)`. A `get_pipeline_or_err(handle) -> Result<_, RenderGraphError>` method would eliminate significant boilerplate.
- [x] **Extract `DrawCall` builder helper** — `renderer/types.rs:204-250` — 6 builder methods all repeat the same `if let Some(inst) = self.instances.first_mut()` guard. Extract a private `with_first_instance_mut()` helper. Minor refactor, low risk.

### P3: Polish

- ~~**Consider a minimal `Mat4` type within katla_gfx**~~ — N/A: Would violate the katla_gfx dependency restriction (must NOT depend on katla_math). Raw `[f32; 16]` arrays match GPU memory layout.
- [x] **Chain errors in `RendererError::source()`** — `error.rs` — `source()` returns `None` for `VulkanError`/`IoError` variants because they convert to `String`, losing the original error. Change `IoError(String)` to `IoError(io::Error)` etc. to preserve error chains for debugging.

## katla_app

### P0: Structural Debt

- [x] **Split `application/mod.rs` monolith (1928 lines)** — Partially decomposed already (`builder.rs` 846 lines, `spawning.rs` 608 lines, `renderer.rs` 530 lines, etc. exist). The remaining `mod.rs` holds the core `Application` struct, `ApplicationHandler` impl (event loop), window events, frame orchestration, input routing, cleanup, hot reload, and timing. Extract event handling, input routing, and frame orchestration into focused submodules. (Done: events.rs, frame_loop.rs, picking.rs, init.rs extracted)
- [x] **Decompose `Application` god struct (39 fields)** — Verified count is 39 fields. Many are cfg-gated editor fields. Group the 7 editor-specific cfg-gated fields (gizmo_state, gizmo_resources, prev_mouse_screen, entity_instance_map, entity_to_instance_indices, pending_pick, stencil_indicator_bindless_index) into an `EditorState` sub-struct behind a single `#[cfg(feature = "editor")]`. This also reduces cfg sprinkling. Particle readback flags could similarly be grouped. (Done: 13 editor fields into EditorState, 2 debug fields into DebugState)

### P1: Stubs / Missing Implementations

- [x] **Implement `EditorAction::DuplicateEntity`** — Currently logs "not yet implemented" (`editor/mod.rs`). Entity duplication with all components is a stub. (Done: Full component duplication with transform offset, GPU resource tracking, selection update)
- [x] **Implement `EditorAction::ResetParticleSystem`** — Currently logs "not yet implemented" (`editor/mod.rs`). Particle system reset is a stub. (Done: reset_all() on GlobalParticleSystem, editor action destroys/re-creates emitters)

### P2: Robustness

- ~~**Reduce `unwrap()` in physics systems**~~ — N/A: All 17 `unwrap()` calls are in `#[cfg(test)]` blocks only, not production code.
- [x] **Guard `TransformOptimization` resource access** — `transform_hierarchy_system.rs` calls `unwrap()` on `get_resource_mut::<TransformOptimization>()` which panics if not inserted. (Done: replaced with get_or_insert_with pattern)
- [x] **Guard asset browser edge cases** — `asset_browser/mod.rs` has `unwrap()` on `drag_asset`, `parent()`, `selected_index` that could panic on edge cases. (Done: all 3 unwraps replaced with if-let/unwrap_or)

### P2: Code Reusability

- [x] **Extract shared transform-from-position pattern** — 44 occurrences total but 24 are in tests. ~15 in production code across 5 files (`spawner.rs`, `scene/mod.rs`, `spawning.rs`, `physics_system.rs`, `velocity_system.rs`). Low priority: `Transform::new_from_position()` already exists; the repetition is the `TransformComponent { transform: ... }` wrapper. A `TransformComponent::from_position()` convenience method would help.
- ~~**Consolidate `Spawner` and `Application` entity creation**~~ — The two paths serve intentionally different access patterns. `Spawner` is a `World` extension trait for system-level access; `Application` spawning handles GPU resource tracking at the editor level. Consolidation is not desirable.
- [x] **Replace hand-rolled TOML parsers with serde** — `Preferences` and `GuiState` now use serde derive with `toml::from_str`/`to_string_pretty`. Added `#[serde(default)]` for forward-compatible partial configs.

### P2: Architecture

- [x] **Reduce `#[cfg(feature = "editor")]` sprinkling** — 84+ occurrences across 7 files (`application/mod.rs` ~40, `builder.rs` ~17, `renderer.rs` ~11, `ui/mod.rs` 8). For struct fields, per-field cfg is idiomatic. For function bodies and methods, extract editor-specific code into the existing `application/editor/` module. Grouping editor fields into `EditorState` sub-struct (P0-2) would also help. (Done: reduced from ~83 to <=35 via EditorState + module-level extraction)

### P3: Polish

- [x] **Split `scene/mod.rs` (3998 lines)** — Largest file in katla_app with no submodules. Scene serialization/deserialization, entity instantiation, GLTF loading, and scene management all in one file. High priority for maintainability — decompose into `scene/serialization.rs`, `scene/loader.rs`, `scene/instantiation.rs`, etc. (Done: tests.rs, default_scene.rs, serialization.rs extracted; mod.rs <= 80 lines)
- [x] **Split `editor_ui.rs` (1334 lines)** — Decomposition already in progress: `editor_ui/` subdirectory exists with `hierarchy.rs` (383 lines), `inspector.rs` (353 lines), `preferences.rs` (559 lines), `status_bar.rs` (117 lines), `toolbar.rs` (173 lines), `viewport_grid.rs` (262 lines), `asset_browser/`. Remaining code is core panel layout/orchestration. Low priority. (Done: types.rs, layout.rs, tests.rs extracted)
- [x] **Remove unused `Selection` resource** — 320 lines of dead code in `resources/selection.rs` with full tests and API, but never imported, registered, or used outside its own file. Either integrate into editor flow or remove.

## katla_app

### P3: Polish

- [x] **Add visual section headers to preferences tabs** — `draw_section_header()` renders a tinted background bar with label text. Appearance: "COLOR THEME" / "VIEW OPTIONS" / "FONT SCALE". Editor: "SNAPPING" / "CAMERA" / "GRID". AI: "PROVIDER" / "CREDENTIALS" / "MODEL SETTINGS". Keybindings: section-grouped by "Viewport", "Scene", "Navigation", "Movement". (`preferences.rs`)
- [x] **Fix cramped vertical spacing in preferences panel** — Replaced ad-hoc magic numbers with named constants: `HEADER_TO_WIDGET` (12px), `WIDGET_GAP` (8px), `SECTION_GAP` (20px), `LABEL_GAP` (8px), `GRID_SPACING` (8px). All tabs use consistent spacing. (`preferences.rs`)
- [x] **Add horizontal padding and column margins to preferences content** — `HORIZONTAL_PADDING` (16px) applied symmetrically. `content_width = panel_width - 2 * HORIZONTAL_PADDING`. All grids and widgets respect the margin. (`preferences.rs`)
- [x] **Fix duplicate "Color Theme" label in Appearance tab** — Removed the duplicate `ui.label_auto_colored("Color Theme", ...)` call. Section header now replaces both labels. (`preferences.rs`)
- [x] **Unify widget row heights and label-to-widget spacing** — `ROW_HEIGHT` (28px) constant used consistently across all tabs. Removed all hardcoded 24.0/20.0 row heights. `LABEL_GAP` (8px) between labels and controls. (`preferences.rs`)
- [x] **Redesign Keybindings tab to show all actual shortcuts** — 17 shortcuts organized into 4 sections (Viewport, Scene, Navigation, Movement) with `draw_section_header()`. Wider badge (120px) for combo keys like "Ctrl+Shift+A". Alternating row backgrounds. Covers gizmo modes, camera controls, save, panels, entity management. (`preferences.rs`)
- [x] **Improve AI tab field layout with inline labels** — `inline_field_row()` helper for label-on-left (30% width) + input-on-right (70%) layout. Applied to Model, Base URL, Temperature, Max Tokens fields. API Key stays full-width. (`preferences.rs`)

## katla_ui

### P1: Correctness

- [x] **Unify click-handling logic across widgets** — `button_with_colors()` intentionally deviates from `click_behavior()` to bypass popup blocking (documented in code). `menu_bar_dropdown()` uses its own inline check. Consider a `click_behavior_popup_aware()` variant for button. (Done: click_interaction() with ClickConfig replaces all three patterns)
- [x] **Fix `label_auto_colored()` cursor corruption** — `context/widgets.rs:136` manually advances `self.cursor` instead of using `advance_cursor()`, bypassing layout stack awareness. Verified: if called inside a row/column layout, it corrupts cursor state. Should use `advance_cursor()` like `label()` in `helpers.rs` does.
- [x] **Fix `button_auto_wide` layout bypass** — `context/widgets.rs:115-133` manually sets cursor. Verified: has a double-advance bug since `self.add()` already calls `advance_cursor()` internally, causing cursor to skip ahead in layouts.

### P2: Robustness

- [x] **Fix `Vertex.texture_index` always being 0** — `types.rs:62-88` — every vertex is created with `texture_index: 0` and comments say "Will be set during batch conversion" but `finalize()` never resolves it. Either remove the field from the public struct or resolve within `finalize()`.

### P2: Visibility

- [x] **Make `UiContext` fields `pub(crate)`** — `context/mod.rs:63-67` exposes `pub input`, `pub style`, `pub fonts`. These fields need external access from `katla_app` (20+ access sites). Tightening would require accessor methods or moving the access patterns. (Done: fields changed to pub(crate), accessor methods added, all 20+ sites updated)
- [x] **Make `LayoutState` `pub(crate)`** — `context/layout.rs:22-36`. Verified: internal layout detail. However, some may be re-exported — check before changing.
- [x] **Make `FontId` inner field `pub(crate)`** — `text/mod.rs:50`. Low priority: `FontId(pub u32)` allows arbitrary construction but this is standard for simple ID types.

### P2: Code Reusability

- [x] **Fix hierarchy view to use a reusable list view** — Each list element in the hierarchy panel is fixed to different pixel sizes, causing elements to jump around when scrolling. Should build a reusable `ListView` widget in `katla_ui` with uniform row heights and virtualized scrolling. (Done: ListView widget created with virtualization, hierarchy refactored)
- [x] **Extract shared text/icon centering utility** — Three separate implementations in `context/drawing.rs:240-265`, `context/drawing.rs:270-305`, `context/widgets/basic.rs:66-72` each compute centering slightly differently. Should be a shared helper.
- [x] **Reduce theme method repetition** — Extracted `pub struct ColorScheme` with all 42 color fields. `dark()`/`light()`/`classic()` now one-liners via `UiStyle::with_colors(ColorScheme::dark())`.
- [x] **Introduce `DraggablePanel::show()` config struct** — `widgets/draggable_panel.rs:99-105` takes 9 parameters with `#[allow(clippy::too_many_arguments)]`. Use a builder or config struct consistent with the crate's widget patterns.

### P3: Polish

- [x] **Rename `context/widgets/selectable.rs` to better reflect contents** — File contains `toggle_button()` which is actively called by `ToggleButton` widget in `widgets/mod.rs:369`. The filename `selectable.rs` is misleading — there is no `selectable()` method. Consider renaming to `toggle_button.rs` or merging into another widget file. The `selectable_selected`/`selectable_hovered` style fields are used.
- [x] **Remove or deprecate `spacer()` in favor of `spacing()`** — `context/layout.rs:92-106` — `spacer()` always advances horizontally regardless of layout direction, while `spacing()` is direction-aware. The doc says "prefer spacing()" but both are `pub`.
- [x] **Document `end_column()` vs `end_row()` spacing asymmetry** — `context/layout.rs:228-231` — `end_column()` adds trailing spacing but `end_row()` doesn't. Undocumented and surprising.
- [x] **Use `KeyCode::Backspace` instead of `\x08` character check** — `context/widgets/basic.rs:256-258` checks `c == '\x08'` for backspace but `KeyCode` enum has a `Backspace` variant. Conflates character input with key events.
- [x] **Remove hardcoded widget default sizes** — `Button` 100x30, `Checkbox` 150x24, `Slider` 150x20, `TextInput` 200x24, etc. don't relate to `UiStyle` dimensions. `at_cursor()` methods use style values but defaults ignore them. (Done: all widget defaults now read from Style fields)
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

### AI Co-Creator — Content Generation with Glass Box Transparency

> **Vision:** An AI co-creator that helps you build worlds, tune gameplay, and iterate on game design — right inside the editor. Ask it to "place a forest of 50 trees around this clearing", "make this particle emitter look like campfire sparks", or "create an enemy patrol route between these points". Every change the AI makes is visible in the viewport, logged in a timeline, and undoable with a single click. You stay in control — the AI suggests, you decide.
>
> **Three pillars:**
> 1. **World Building** — Procedural entity placement, scene templates, environment composition. "Populate this area with ruins and overgrowth."
> 2. **Parameter Tuning** — Iterate on particle effects, lighting moods, physics feel. "Make the fire warmer and more flickery."
> 3. **Game Logic & Design** — Entity behaviors, gameplay rules, balance tuning. "Add enemies that chase the player within 10 units."
>
> **Architecture:** `katla_agent` crate provides the execution harness, scene tools, and LLM integration. All AI code is feature-gated behind `#[cfg(feature = "agent")]`. The AI's actions flow through the same undo stack as manual edits. Glass box UI shows every action, lets you scrub the timeline, and rollback anything. See `docs/agent-glass-box-research.md` for research findings.
>
> **Build order:** Component Reflection → Scene Tool API → Agent Harness → LLM Backend → Content Generation Tools → Glass Box UI.

#### Foundation: Component Reflection (unlocks generic scene tools and inspector)

- [x] **Design the `Inspect` trait** — `Inspect` trait with `fn fields() -> Vec<FieldInfo>` and `fn field_mut(&mut self, name: &str) -> Option<FieldMut<'_>>`. Uses `&dyn Any` for field values. (`katla_ecs/src/inspect.rs`)
- [x] **Define `FieldInfo` / `FieldConstraints` types** — Structs for field metadata: name, display_name, type_name, kind (Float/Int/Bool/String/Color/Struct/Enum/Vec/EntityRef), constraints (range, speed, skip). (`katla_ecs/src/inspect.rs`)
- [ ] **Extend `#[derive(Component)]` to generate `Inspect` impl** — Modify `katla_derive` to parse struct fields and generate an `Inspect` implementation behind `#[cfg(feature = "editor")]`. Parse `#[inspect(skip)]`, `#[inspect(range = 0.0..=1.0)]`, `#[inspect(color)]` field attributes. (~scope: medium)
- [ ] **Build `PropertyEditor` dispatch for generic widgets** — Function that takes `FieldInfo` + `FieldMut` and renders the appropriate katla_ui widget. Start with f32 (slider), bool (checkbox), String (text input). Dispatch on field kind. (~scope: medium)
- [ ] **Implement missing katla_ui widgets for inspection** — Build DragValue (numeric drag-to-edit), Dropdown (enum selector), ColorPicker. Needed for generic inspector and AI parameter editing. (~scope: large)
- [ ] **Rewrite inspector panel to use `Inspect` trait** — Replace hardcoded match arms in `inspector.rs` with generic `Inspect`-based rendering. (~scope: medium)

#### Scene Tool API (what the AI uses to build content)

- [x] **Design `SceneTool` trait and standard tool set** — `SceneTool` trait with JSON Schema definitions for LLM function calling. Tools: `spawn_entity`, `destroy_entity`, `set_component`, `query_entities`, `get_scene_hierarchy`, `duplicate_entity`. (`katla_agent/src/tools/`)
- [x] **Implement `Command` pattern for scene mutations** — `SceneOp` enum with execute/undo semantics. AI actions flow through the same undo stack as manual edits. (`katla_agent/src/tools/mod.rs`)
- [x] **Implement `UndoGroup` for atomic AI operations** — All operations from one AI request are grouped into one undo unit. Single Ctrl+Z undoes the entire group. (`katla_app` editor action integration)
- [ ] **Add scene tool validation layer** — Validate tool calls: reject out-of-range values, invalid entity IDs, destructive ops on protected entities. Clamp values to component constraints from `FieldConstraints`. (~scope: small)
- [ ] **Build entity query/filter language** — Allow the AI to query "all entities with PointLight within radius R of position P", "all ParticleEmitters", "entity named 'Player'". Structured filter that maps to ECS queries. (~scope: medium)
- [x] **Implement procedural placement helpers** — `scatter()`, `place_grid()`, `place_ring()`, `place_cluster()`, `place_along_path()` in `katla_agent/src/tools/placement.rs` with tests. Building blocks for "place 50 trees" type requests.
- [x] **Create scene templates / prefab system** — `SceneTemplate` struct with named templates: `campfire`, `street_lamp`, `village_square`, `forest_clearing`. AI instantiates templates instead of raw components. (`katla_agent/src/tools/templates.rs`)

#### Agent Harness (execution infrastructure)

- [x] **Design `Agent` trait and `AgentContext`** — `Agent` trait with observe/decide/on_result cycle. `AgentContext` provides read-only ECS world access and scene tool invocation. Background thread submits tool calls to main thread. (`katla_agent/src/lib.rs`)
- [x] **Build async execution bridge** — Dedicated tokio runtime on background thread with `mpsc` channels. Agent submits tool calls, main thread processes N pending actions per frame. (`katla_agent/src/runtime.rs`)
- [x] **Create `katla_agent` crate skeleton** — New workspace crate: agent trait, scene tools, execution context, LLM integration. Feature-gated behind `#[cfg(feature = "agent")]`. (`katla_agent/`)
- [x] **Write integration tests for agent + scene tools** — Tests for placement tools (scatter, grid, ring, cluster, path), templates, and tuning. (`katla_agent/src/tools/`)

#### LLM Backend (connects AI co-creator to the engine)

- [x] **Design `LlmProvider` async trait** — `LlmProvider` trait with `chat_completion(messages, tools) -> Response`. `OpenAiProvider` using `async-openai`. Gate behind `#[cfg(feature = "llm-assistant")]`. (`katla_agent/src/llm/`)
- [x] **Set up async runtime bridge for LLM calls** — Dedicated tokio runtime on background thread. Winit render loop dispatches requests and polls for responses. (`katla_agent/src/runtime.rs`)
- [x] **Implement scene context serialization** — `get_scene_context_json()` serializes current scene state as structured JSON: selected entity + components, nearby entities, hierarchy, entity counts by type. (`katla_app/src/application/editor/agent.rs`)
- [x] **Write game-design-aware system prompts** — System prompt that understands Katla's ECS, component types, and tool capabilities. Tailored for content generation. (`katla_agent/src/co_creator/`)
- [x] **Build co-creator chat panel UI** — `CoCreatorPanel` draggable panel with message history, text input (Enter to send, click-outside unfocus), send button, Ctrl+Shift+A shortcut, View menu entry. Streaming response display with message roles (user/assistant/system). (`katla_app/src/ui/editor_ui/co_creator.rs`)

##### Wire LLM configuration and connect to real backends

- [x] **Create `LlmConfig` struct in `katla_agent`** — `katla_agent/src/config.rs` with `provider` enum (`Disabled`/`OpenAi`/`OpenAiCompatible`), `api_key` with `$ENV_VAR` resolution, `base_url` for custom endpoints, `model`, `max_tokens`, `temperature`. Persisted as `llm.toml` in Katla config dir. 9 unit tests.
- [x] **Extend `OpenAiProvider` to support custom base URLs** — `from_config(config: &LlmConfig)` constructor reads api_key, base_url, model. `with_base_url()` for custom endpoints (Ollama/LM Studio/vLLM). `from_env()` kept as fallback. (`katla_agent/src/llm/openai.rs`)
- [x] **Enable `llm-assistant` feature in `katla_app`** — `katla_agent = { workspace = true, features = ["llm-assistant"] }` in `katla_app/Cargo.toml`. Verified compilation.
- [x] **Replace pattern-matching stub with real LLM calls** — `process_co_creator_request` dispatches to LLM when configured, falls back to local pattern matching when disabled. Per-frame `poll_llm_response()` in editor loop. Tool definitions for all 6 scene tools. Conversation history tracking. `EditorState` extended with `llm_config`, `async_bridge`, `pending_llm_request`, `llm_conversation` fields. (`katla_app/src/application/editor/agent.rs`)
- [x] **Add LLM config UI to preferences panel** — "AI" tab in `PreferencesPanel` with provider selection (Disabled/OpenAI/OpenAI Compatible), API key text input, model input, base URL input (for OpenAI Compatible), temperature slider (0.0–2.0), max tokens buttons (1024/2048/4096/8192), Save button, status display. 7 new `EditorAction` variants wired through `process_editor_actions` to update `LlmConfig` and persist to `llm.toml`. (`katla_app/src/ui/editor_ui/preferences.rs`, `types.rs`, `mod.rs`, `layout.rs`)
- [x] **Handle API key security** — Manual `Debug`/`Display` impls on `LlmConfig` redact API keys to `***`/`<first4chars>***`. Env var references never leak resolved values in Debug. `OpenAiProvider` has manual `Debug` impl with `finish_non_exhaustive()`. `llm.toml` gets `0o600` permissions on Unix. Preferences AI tab never pre-fills the API key. `llm.toml` added to `.gitignore`. No log statements leak keys. Security audit passed with all findings fixed.
- [x] **Add `chat_completion_stream()` to `LlmProvider` trait** — `StreamChunk` with `content_delta` + `finish_reason`. Default impl falls back to non-streaming as single chunk. `OpenAiProvider` uses `async-openai`'s `chat().create_stream()` with `futures::stream::unfold`. (`katla_agent/src/llm/mod.rs`, `openai.rs`)
- [x] **Add streaming channel to `AsyncBridge`** — `submit_chat_stream()` returns `PendingStreamRequest` with `poll_chunks()` (drains all available) and `is_done()`. Background tokio task forwards stream chunks over `mpsc::channel`. (`katla_agent/src/runtime.rs`)
- [x] **Append partial tokens to live assistant message in `CoCreatorState`** — `append_streaming_text()` extends last assistant message or creates new one. `finalize_streaming()` sets `processing = false` and removes empty assistant messages. (`katla_app/src/ui/editor_ui/co_creator.rs`)
- [x] **Wire streaming poll into editor frame loop** — `submit_llm_stream_request()` replaces `submit_llm_request()`. `poll_llm_stream()` drains chunks, appends deltas, finalizes on finish reason. `EditorState.pending_llm_stream` replaces `pending_llm_request`. (`katla_app/src/application/editor/agent.rs`, `mod.rs`)
- [ ] **Add `rmcp` dependency and MCP server skeleton** — Add `rmcp` to `katla_agent/Cargo.toml` behind new `mcp-server` feature. Create `katla_agent/src/mcp.rs` with `KatlaMcpServer` struct implementing `rmcp::ServerHandler`. Server holds a `mpsc::Sender<McRequest>` to forward tool calls to the main thread. Transport: stdio. (~scope: small)
- [ ] **Define MCP tools matching the 6 built-in scene tools** — `rmcp::tool!` macro declarations for `spawn_entity`, `destroy_entity`, `set_component`, `query_entities`, `get_scene_hierarchy`, `duplicate_entity`. Each maps to the existing `SceneOp` enum. JSON schemas match current `ToolDefinition` parameters. (~scope: small)
- [ ] **Bridge MCP tool calls to the main-thread ECS world** — Main thread receives `McpRequest` via channel, executes against `World` + `ComponentRegistry`, returns `McpResponse`. Use `run_scripted_agent()` or direct `SceneOp` execution. Results serialized back to MCP as JSON text content. (~scope: medium)
- [ ] **Start MCP server alongside editor when feature is enabled** — In `Application::new()`, if `mcp-server` feature is active, spawn `KatlaMcpServer` on a background thread with stdio transport. Log connection/disconnect events. Graceful shutdown on app exit. (~scope: small)
- [ ] **Add `.claude/mcp.json` config example** — Document the stdio command to connect Claude Code to Katla's MCP server. Add to `docs/` or as a commented example in project root. (~scope: trivial)
- [ ] **Add local inference backend option** — Behind `llm-assistant-local` feature, wrap `llama-cpp-2` or `mistralrs` for offline use. (~scope: large)

#### Content Generation Tools (world building, tuning, game logic)

- [x] **Build world building tool set** — High-level placement tools: `scatter()`, `place_grid()`, `place_ring()`, `place_cluster()`, `place_along_path()`. Scene templates: `campfire`, `street_lamp`, `village_square`, `forest_clearing`. (`katla_agent/src/tools/`)
- [x] **Build parameter tuning tool set** — `adjust_field()`, `set_field()`, `create_variants()` for iterative feel adjustment. Semantic presets: `warm_light()`, `cool_light()`, `flickery()`. A/B comparison via `create_variants()`. (`katla_agent/src/tools/tuning.rs`)
- [ ] **Build game logic / behavior tool set** — Tools for gameplay: `add_behavior(entity, behavior_template)` for common patterns (patrol, chase, wander, interact), `create_trigger(area, action)` for spatial triggers, `balance_curves(params)` for tuning difficulty curves. These define reusable behavior patterns the AI can compose. (~scope: large)
- [x] **Add scene analysis / suggestion tools** — `analyze_scene()` returns structured observations (entity counts, component distributions) the LLM uses to make suggestions. Scene context serialization provides current state. (`katla_agent/src/context.rs`, `katla_app/src/application/editor/agent.rs`)

#### Glass Box Transparency (see and control what the AI does)

- [x] **Design `AgentAction` data model** — `AgentAction` struct with `ActionId`, `SceneOp`, result/error. `AgentSession` tracks all actions with undo stack. (`katla_ecs/src/agent/session.rs`)
- [x] **Build `ActionLog` with checkpoint storage** — `AgentSession` maintains ordered action log and undo stack with `UndoGroup` entries. Each group corresponds to one agent turn. (`katla_ecs/src/agent/session.rs`)
- [x] **Implement checkpoint-based rollback** — `AgentSession::undo_last()` undoes one turn, `undo_all()` restores to pre-session state. Full undo of an AI session via undo stack. (`katla_ecs/src/agent/session.rs`)
- [ ] **Build viewport entity highlighting for AI actions** — Color-coded outlines on entities the AI just created (green), modified (yellow), or is about to delete (red). Fades out after a few seconds. (~scope: large)
- [ ] **Build action timeline in the editor** — Horizontal timeline showing AI actions as colored blocks. Click any action to highlight affected entities, see what changed, or rollback to that point. (~scope: large)
- [ ] **Build diff view for scene changes** — For any AI action or session: list of entities added/removed/modified with component-level old→new diffs. (~scope: medium)
- [ ] **Add pause/resume/step controls** — Pause the AI mid-operation, step through actions one at a time, or rollback. Essential for reviewing large world-building operations before committing. (`AgentSession.paused` field exists; UI controls not yet built.) (~scope: medium)

#### Game Loop Modes (Edit/Play/Simulate) — orthogonal infrastructure

- [ ] **Implement `EngineMode` enum and state machine** — `EngineMode` enum (`Edit`, `Play`, `Simulate`) with transition logic. `ModeStateMachine` resource in ECS world. Define valid transitions and guards. (~scope: medium)
- [ ] **Add component snapshot support to `Component` trait** — Extend trait (or add `Snapshot` trait) with `clone_into_world(&self, new_entity, &mut World)`. Ensure all scene-relevant components implement it. Add `#[derive(Snapshot)]` macro. (~scope: medium)
- [ ] **Implement ECS world cloning for mode transitions** — `World::clone_for_play()` iterates archetypes, clones component data, builds entity ID remapping table, skips editor-only components. ~scope: large)
- [ ] **Create system set dispatch infrastructure** — `SystemSet` variants (`Editor`, `Runtime`, `Always`). `run_if_mode(EngineMode)` condition. Main loop checks current mode before dispatching. (~scope: medium)
- [ ] **Implement editor world snapshot on mode enter** — On Edit→Play: snapshot editor world, store as `EditorWorldSnapshot`, create play world from clone. On Play→Edit: discard play world, restore snapshot. (~scope: medium)
- [ ] **Add mode transition event system** — `ModeTransitionEvent` (from_mode, to_mode). Systems subscribe for setup/teardown (initialize physics, enable/disable gizmos). (~scope: small)
- [ ] **Implement simulate mode** — `Simulate` variant: runs runtime systems, no player possession, editor camera stays active, viewport interactive while physics runs. (~scope: medium)
- [ ] **Add engine mode UI controls** — Toolbar buttons (Play/Stop/Simulate), current mode indicator, keyboard shortcuts (F5=Play, Shift+F5=Stop, F6=Simulate). (~scope: small)
- [ ] **Handle entity reference remapping** — `EntityRemapper` tracking old→new entity IDs during world clone. Auto-remap entity references in components (parent entities, joint targets) via `RemapEntities` trait. (~scope: medium)
- [ ] **Write integration tests for mode transitions** — Test Edit→Play→Edit roundtrip preserves editor state. Entity references survive cloning. Mode-specific systems only run in correct mode. (~scope: medium)

#### Multithreading — orthogonal infrastructure

- [ ] **Add system access metadata to `katla_ecs`** — Each system declares which components it reads/writes. Build access analysis pass that identifies non-conflicting systems safe for parallel dispatch. (~scope: medium)
- [ ] **Implement parallel ECS system dispatch via rayon** — Use rayon `scope()` to dispatch non-conflicting systems concurrently. Start with independent systems (rendering + physics), expand to full access-based scheduling. (~scope: large)
- [ ] **Add per-thread command pool management to `katla_gfx`** — Vulkan command pools are not thread-safe. Create `CommandPoolManager` that provides per-thread pools for parallel command buffer recording. (~scope: medium)
- [ ] **Implement parallel asset loading** — Background worker threads load textures/meshes via crossbeam channels. Main thread uploads to GPU at frame boundary. Decouple I/O from render loop. (~scope: medium)
- [ ] **Add parallel render graph pass execution** — Identify passes with no resource dependencies, record in parallel using secondary command buffers. Dependency-based scheduling. (~scope: large)
- [ ] **Benchmark and validate threading improvements** — Measure frame time before/after each parallelism change. Verify no data races. Profile with Tracy or custom instrumentation. (~scope: small)

### P3: Cleanup

- [x] **Audit `#[allow(clippy::too_many_arguments)]`** — 17 functions suppress this lint (down from ~20). Consider introducing parameter structs for functions with many arguments. (Done: all 19 functions refactored with parameter structs, zero allow annotations remain)

### P3: Dependency Hygiene

- [x] **Upgrade skrifa to latest and deduplicate** — Upgraded to `skrifa 0.40`, replaced custom `BoundsPen` with `ControlBoundsPen`. Single version in lockfile, no duplicates.
- [x] **Pool `vello_cpu::RenderContext` for CJK workloads** — Currently a fresh `RenderContext` + `Pixmap` is allocated per glyph cache miss. Acceptable for pre-cached ASCII but wasteful for runtime CJK input. Reuse a shared context or pool buffers. (Done: GlyphRenderPool with closure-based acquire pattern)
