# TODO

## AI Panel: Tool Calling

- [x] ~~### 1. Carry tool call data through the streaming pipeline~~ — `StreamChunk.tool_call_deltas` and `convert_stream_chunk()` extraction already implemented.
- [x] ~~### 2. Accumulate complete tool calls in CoCreatorAgent~~ — `tool_call_accumulators`, `pending_tool_calls`, `finalize_tool_call_accumulators()`, tool calls stored in assistant history messages.
- [x] ~~### 3. Execute tool calls against the ECS world~~ — `tool_call_to_scene_op()` maps all tool names to SceneOps, `SceneToolExecutor::execute()` runs them.
- [x] ~~### 4. Send tool results back to the LLM~~ — `execute_and_continue_tool_calls()` adds tool results to history and calls `submit_continuation()` for multi-turn loop.
- [x] ~~### 5. Display tool call activity in the AI panel~~ — `format_tool_call_summary()` and `format_tool_call_result()` show tool activity as system messages.

## AI Panel: UX Polish

- [x] ~~### 6. Auto-scroll to latest message~~ — Implemented via ScrollArea with `stick_to_bottom(true)`.
- [x] ~~### 7. Message area scroll with ScrollArea widget~~ — Replaced `begin_column`/`end_column` with `scroll_area()`, tracking `ScrollAreaState` in `CoCreatorState`.
- [x] ~~### 8. Multiline input with Shift+Enter~~ — Added `multiline` mode to `TextInput`: Shift+Enter inserts newline, Enter submits. Dynamic input height up to 5 lines.

---

## P1: Bugs

~~### 9. UI vertex buffer overflow crash when UI data exceeds 1MB pre-allocation~~ — Fixed in 37b3dbc. BufferObject auto-resizes with 2x growth on overflow.

~~### 10. `Mat2::mul()` has incorrect matrix multiplication indices~~ — Fixed in c3f7b51.
- **Crate:** katla_math
- **File:** `src/mat2.rs`
- **Issue:** `Mat2::mul(&self, rhs)` computes `self[1][0] * rhs[1][0]` where it should compute `self[1][0] * rhs[0][1]`. The `impl Mul<Mat2> for Mat2` has the correct implementation. The `mul()` method produces wrong results for asymmetric matrices.
- **Fix:** Fix the index pattern in `mul()` to match `impl Mul`.

~~### 11. `plane::intersects_aabb` only accounts for X component of normal when selecting corners~~ — Fixed in c3f7b51. Component-wise corner selection now checks all axes.
- **Crate:** katla_math
- **File:** `src/plane.rs`
- **Issue:** The positive/negative corner selection only checks `normal.x() >= 0` and ignores the Y/Z sign of the normal. For normals like `(1, -1, 0)`, the test corners are wrong. The `frustum.rs::intersects_aabb` does this correctly.
- **Fix:** Build positive/negative corners component-wise: for each axis, use `center + extent` if `normal[i] >= 0`, else `center - extent`.

~~### 12. `Quat::slerp` doesn't handle quaternion double-cover (negation equivalence)~~ — Fixed in 4486cb4. Added negation check in both SSE and scalar implementations.

~~### 13. `Mat4::inverse()` panics on singular matrices (produces NaN/inf)~~ — Fixed in 37b3dbc. Now returns `Option<Mat4>` matching Mat3::inverse() pattern.

~~### 14. Transfer queue hardcoded to family index 0~~ — Fixed in b1b222f. Uses queue_indices.transfer_idx.unwrap_or(graphics_queue_idx).
- **Crate:** katla_gfx
- **File:** `src/vulkan/context/mod.rs` (lines ~213, ~310)
- **Issue:** `transfer_queue_idx` is hardcoded to `0` instead of using the `transfer_idx` found by `find_queue_families()`. On GPUs where the graphics family index != 0, the transfer command pool will use the wrong queue family.
- **Fix:** Use `queue_indices.transfer_idx.unwrap_or(graphics_queue_idx)`.

~~### 15. `max_tokens` and `temperature` never sent to OpenAI API~~ — Fixed in b7089dd. OpenAiProvider now stores and passes max_tokens/temperature to requests.
- **Crate:** katla_agent
- **File:** `src/llm/openai.rs` (lines 165-170, 232-237)
- **Issue:** `LlmConfig` stores `max_tokens` and `temperature` but `OpenAiProvider` doesn't pass them to the request. The `CreateChatCompletionRequest` uses `..Default::default()` so both are always unset.
- **Fix:** Store config values in `OpenAiProvider` and set `request.max_tokens = Some(...)` and `request.temperature = Some(...)`.

~~### 16. `frame_count` never increments in normal (infinite) mode~~ — Fixed in c3f7b51. Increment moved outside max_frames guard.
- **Crate:** katla_app
- **File:** `src/application/frame_loop.rs` (lines 277-278)
- **Issue:** `frame_count += 1` is inside the `if let Some(max) = self.info.max_frames` block. In normal infinite mode, `frame_count` stays 0 forever, affecting FPS counters, UI display, and any frame-count-dependent logic.
- **Fix:** Move `self.frame_count += 1` outside the `max_frames` guard.

~~### 17. `RadioButton` builder ignores `id` field~~ — Fixed in 4486cb4. Added optional `id` field with `unwrap_or(label)` fallback.

~~### 18. `i64`/`u64` types get `FieldKind::Int` but no typed `FieldMut` accessor~~ — Fixed in 9ff0698. Added I64/U64 variants to FieldMut, FieldValue, derive macro, and agent context.

~~### 19. Silent parse error swallowing in `#[inspect]` attribute~~ — Fixed in 7014013. Parse errors now propagate via syn::Error::new_spanned().
- **Crate:** katla_derive
- **File:** `src/lib.rs` (`parse_inspect_attr()`)
- **Issue:** `let _ = meta_item.parse_nested_meta(...)` discards errors. Invalid `#[inspect(range("bad"))]` syntax silently does nothing.
- **Fix:** Propagate the error via `syn::Error::new_spanned()`.

~~### 20. Incorrect ForkAwesome codepoints in katla_icons~~ — Fixed in b1b222f. Remapped to valid ForkAwesome equivalents.
- **Crate:** katla_icons
- **File:** `src/lib.rs`
- **Issue:** `PENCIL_ALT` (F303), `TRASH_ALT` (F2ED), `HAMMER` (F6E3), `RULER_COMBINED/HORIZONTAL/VERTICAL` (F546-F548) are Font Awesome 5+ codepoints that don't exist in ForkAwesome. They render as blank/missing glyphs. Currently unused but will break when used.
- **Fix:** Remove or remap to valid ForkAwesome codepoints (e.g., `TRASH_ALT` → `fa-trash-o` at F014, `HAMMER` → `fa-gavel` at F0E3).

---

### ~~86. Remove particle debug readback from katla_app debug builds~~ ✓
- **Crate:** katla_app
- **Files:** `katla_app/src/application/builder.rs`, `katla_app/src/application/renderer.rs`, `katla_app/src/application/mod.rs`, `katla_app/src/application/init.rs`, `katla_gfx/src/render_graph/frame/mod.rs`, `katla_gfx/src/render_graph/frame_graph.rs`
- **Done:** Removed `DebugState` struct, `debug` field on Application, `#[cfg(debug_assertions)]` init/trigger/readback blocks from builder/renderer/init, and `particle_debug_readback` field from Frame and FrameGraph params.

## P2: Missing Features

### 21. No undo/redo system in the editor
- **Crate:** katla_app
- **Issue:** Destructive operations (delete entity, transform changes) have no undo. Slider drags now mutate ECS directly (#54 fix), but there is no undo stack to reverse them.
- **Existing infrastructure:** `UndoGroup`/`SceneCommand` already exist in `katla_ecs/src/scene_tool/command.rs` with spawn, destroy, set-field, duplicate commands. `SceneToolExecutor::execute()` already returns `(ToolResult, UndoGroup)`. `AgentSession` has `push_undo()`/`undo_last()`/`undo_all()` pattern.
- **Sub-tasks:**
  - [x] 21a. Add `undo_stack: Vec<UndoGroup>` and `redo_stack: Vec<UndoGroup>` to `EditorState`, with `push_undo()`, `perform_undo()`, `perform_redo()` helpers (small, low risk) — Done in f34a0d0. Also added `redo_all()` to `UndoGroup`.
  - [x] 21b. Add Ctrl+Z / Ctrl+Shift+Z keyboard shortcuts in `handle_editor_keyboard_shortcuts()` (small, low risk) — Done in f34a0d0. Guards with prev_want_capture_keyboard.
  - [ ] 21c. Capture UndoGroups from `EditorAction::DeleteEntity`, `DuplicateEntity`, `SpawnModel` in `process_editor_actions()` via `ComponentRegistry` snapshots (medium, medium risk)
  - [ ] 21d. Capture slider drag start/end values for undo — snapshot pre-drag ECS values on drag start, push `SetFieldCommand`-based `UndoGroup` on drag end (medium, medium risk)
  - [ ] 21e. Add Undo/Redo items to Edit menu in toolbar (small, low risk)
  - **Recommended order:** 21a → 21b → 21c → 21d → 21e

~~### 22. No texture/mesh/material destroy API on `VulkanRenderer`~~ — False positive. `destroy_api.rs` already implements `destroy_texture`, `destroy_mesh`, `destroy_material`, and `destroy_skeleton` with bindless slot release and tests.

### ~~23. Particle emitter GPU resources not cleaned up on entity destruction~~ — Fixed. `ParticleSystem` now tracks entity-to-handle mappings via `entity_emitters` HashMap and destroys GPU emitters for entities that no longer exist.

~~### 24. No combo box / dropdown select widget~~ — Fixed in f5cf41f. Added ComboBox builder widget with trigger button, dropdown popup, and selection support.

~~### 25. No `clear_history()` or token budget on `CoCreatorAgent`~~ — Fixed in b96aee7. Added clear_history() and truncate_history(max_messages) methods.
- **Crate:** katla_agent
- **File:** `src/co_creator/mod.rs`
- **Issue:** History grows unbounded. No truncation, no token counting, no pruning of old messages. Long sessions will hit context window limits or cost excessive tokens.
- **Fix:** Add `clear_history()`, `truncate_history(max_messages)`, and a token budget system.

~~### 26. No timeout on LLM requests~~ — Fixed in 16568d0. `submit_chat` wrapped with 120s timeout, `submit_chat_stream` with 30s per-chunk timeout.

### 27. No parent-child entity hierarchy in ECS
- **Crate:** katla_ecs / katla_app
- **Issue:** `SceneOp::GetSceneHierarchy` returns all entities flat. `Parent`/`Children` components already exist in `katla_app/src/components/scene/relationship.rs` with serialization and transform hierarchy support, but there is no `SetParent` scene op, no automatic hierarchy maintenance on destroy/duplicate, and no structured hierarchy output.
- **Sub-tasks:**
  - [x] 27a. Add `SceneOp::SetParent { entity, parent: Option<EntityId> }` with cycle detection and automatic `Parent`/`Children` maintenance (medium, low risk) — Done in f34a0d0. Executor validates entities, set_parent_components() maintains Parent/Children with cycle detection. Tool/MCP endpoints added.
  - [x] 27b. Rewrite `exec_hierarchy()` to return structured JSON tree with parent/depth info instead of flat list (small, low risk) — Done in f34a0d0. build_hierarchy_json() returns recursive tree with id/name/depth/children.
  - [ ] 27c. Update `exec_destroy` to clean up `Parent`/`Children` of destroyed entity (cascade or re-parent) (small, low risk)
  - [ ] 27d. Update `exec_duplicate` to optionally preserve hierarchy (small, low risk)
  - [ ] 27e. Add `set_parent` agent tool and MCP endpoint (small, low risk)

### ~~28. No query filtering (`Without<T>`, `With<T>`)~~ — Fixed. Added `With<T>`/`Without<T>` marker types, `QueryFilter` trait with tuple support, `FilteredQueryIter` wrapper, and `World::query_filtered()` method with 9 tests.

~~### 29. No `#[inspect(enum)]` / `#[inspect(struct)]` / `#[inspect(vec)]` attribute support~~ — Fixed in 66cb756. Added ExplicitFieldKind enum and parsing for enum, struct, vec, entity_ref attributes.

~~### 30. Text kerning not implemented (returns 0.0)~~ — Fixed in 16568d0. Implemented kern table lookup via skrifa with Format 0 and Format 2 subtable support.
- **Crate:** katla_ui
- **File:** `src/text/measurement.rs` (line 33)
- **Issue:** `get_kerning()` always returns 0.0 with a TODO comment. Character pairs like "AV", "To" have incorrect spacing.
- **Fix:** Implement GPOS kerning via skrifa's API.

~~### 31. Selection clearing via Escape key~~ — False positive. Already implemented at `layout.rs:135`: `self.selected_entity = None` on Escape key press.

~~### 32. `pace` keybinding / camera speed control~~ — Fixed in 34d4bf6. Shift = 3x speed, Ctrl = 0.3x speed, configurable base speed via EditorSettings.

---

## P3: Improvements

~~### 33. Add `#[inline]` to hot-path methods across math crate~~ — Fixed in 6a186a1. Added #[inline] to Mat3/Mat4/Transform/Quat hot-path methods.

~~### 34. Add `#[inline]` to hot-path ECS methods~~ — Fixed in 93644a9. Added #[inline] to storage, entity_allocator, sparse_set methods.

~~### 35. Add `#[inline]` to hot-path UI text methods~~ — Fixed in 8a5e7a0. Added #[inline] to measure_text, font_ascent, line_height.

~~### 36. Replace `unwrap()` with `expect()` in production UI code~~ — Fixed in 8a5e7a0. Replaced 5 unwrap() calls with expect() across basic.rs, scroll_area.rs, glyph_pool.rs.

~~### 37. Remove deprecated `button_height_*` style fields~~ — Removed in dcc28d7. Deleted button_height_small, button_height_medium, toolbar_height from UiStyle and updated all references.

~~### 38. `Mat4::create_ortho` parameter order doesn't match standard convention~~ — Fixed in f74b05c. Reordered to (left, right, bottom, top, near, far) matching GLM/Vulkan convention.

~~### 39. `Mat4::create_proj` semantics unclear (infinite reverse-Z, no `far` parameter)~~ — Fixed in 9ff0698. Renamed to `create_proj_reverse_z` to communicate infinite reverse-Z semantics.

~~### 40. `Quat::inverse()` is actually `conjugate()` — misleading for non-unit quaternions~~ — Fixed in 13b353e. Renamed to `conjugate_unit()` with doc comment clarifying unit-quat precondition.

~~### 41. `AABB` missing `Copy` derive~~ — Fixed in c3f7b51. Added Copy derive.
- **Crate:** katla_math
- **File:** `src/aabb.rs`
- **Issue:** Both fields are `Copy` but `AABB` only derives `Clone, Debug`. Every use by value requires `.clone()`.
- **Fix:** Add `#[derive(Copy)]`.

~~### 42. `Transform` missing `PartialEq` derive~~ — Fixed in 8a5e7a0. Added PartialEq derive to Transform and implemented it for SSE Quat.

~~### 43. Radio button renders as square instead of circle~~ — Fixed in 7014013. Uses add_circle for circular rendering.
- **Crate:** katla_ui
- **File:** `src/context/widgets/basic.rs` (lines 464-489)
- **Issue:** Uses `draw_rect_border` and `draw_rect` producing a square. Radio buttons should be circular.
- **Fix:** Use `add_circle` or `add_convex_poly` for circular shapes.

~~### 44. `DrawCall::instances` is a `Vec<InstanceData>` — heap allocation per draw call~~ — Fixed in f74b05c. Changed to SmallVec<[InstanceData; 1]> to avoid heap allocation for single-instance draws.

~~### 45. Unused transfer queue infrastructure~~ — False positive. Transfer queue and command pool are used by `picking.rs` and `readback.rs` for GPU readback operations.

~~### 46. `OutlineSubsystem::destroy` doesn't zero out pipeline handles~~ — Fixed in 6a186a1. All 8 pipeline handles zeroed in destroy().

~~### 47. `TextureManager::from_vulkan_resources()` is a stub returning handle 0~~ — Removed in 2659e2e. No callers; proper implementation would require rearchitecting Texture ownership.

~~### 48. Stale TODO comments in renderer module~~ — Fixed in 4486cb4. Removed stale viewport/ui extraction TODOs.

~~### 49. `katla_derive` uses edition 2021 instead of workspace edition 2024~~ — Fixed in 8a5e7a0. Updated to edition 2024.

~~### 50. `Barrier::deduce_transition_masks` panics on unsupported layout transitions~~ — Fixed in dcc28d7. Changed to return Result with warn fallback instead of panic.

~~### 51. Panic-based error messages in derive macro instead of `syn::Error`~~ — Fixed in 7014013 (as part of #19). All panic! calls converted to syn::Error::new_spanned().
- **Crate:** katla_derive
- **File:** `src/lib.rs` (lines ~50, ~59, ~65, ~68)
- **Issue:** `panic!("range() expects numeric literals")` etc. produce ugly errors without file/line context.
- **Fix:** Use `syn::Error::new_spanned()` for span-accurate compile errors.

~~### 52. `DepthFormat::None` maps to `ImageFormat::R8G8B8A8Srgb` placeholder~~ — Fixed in 7014013. Now maps to ImageFormat::Auto.
- **Crate:** katla_gfx
- **File:** `src/viewport.rs`
- **Issue:** Semantically wrong placeholder. Could cause incorrect behavior if used.
- **Fix:** Return `ImageFormat::Auto` or handle `None` at call site.

~~### 53. `created_at` timestamp always overwritten on scene save~~ — Fixed in b1b222f. Preserves original created_at, only updates modified_at.
- **Crate:** katla_app
- **File:** `src/scene/serialization.rs` (lines 35-41)
- **Issue:** `save_scene()` always sets both `created_at` and `modified_at` to now. Original creation time is lost.
- **Fix:** Preserve original `created_at` across saves.

~~### 54. `apply_inspector_slider_changes` pushes duplicate EditorActions every frame during drag~~ — Fixed in 16568d0. Removed redundant EditorAction pushes; ECS components are now mutated directly during drag.
- **Crate:** katla_app
- **File:** `src/application/editor/mod.rs` (lines 159-244)
- **Issue:** ~120 identical `EditorAction::UpdateTransform` actions during a 2-second slider drag at 60fps. All are processed redundantly.
- **Fix:** Only push action on final value change (mouse release), or mark intermediates as "preview".

~~### 55. `destroyed_entities.contains()` is O(n) per component removal event~~ — Fixed in 6a186a1. Changed Vec to HashSet for O(1) lookups.

~~### 56. `create_variants` ignores component/field/values parameters~~ — Fixed in 2659e2e. Added VariantsPlan struct with two-phase execution (duplicates + field_sets).

~~### 57. `scatter()` with `min_spacing` returns fewer entities than requested without warning~~ — Fixed in 2659e2e. Added ScatterResult struct reporting count_placed vs count_requested.

~~### 58. `place_cluster()` radius distribution uses wrong formula~~ — Fixed in 93644a9. Changed cbrt(sqrt(t)) to cbrt(t) for correct t^(1/3).

~~### 59. `McpOp::QueryEntities` loses `name_filter`, `position`, `radius` fields~~ — Fixed in b96aee7. Extended QueryEntitiesParams and McpOp with name_filter, position, radius fields.
- **Crate:** katla_agent
- **File:** `src/mcp.rs` (lines 67-73)
- **Issue:** Conversion hardcodes `name_filter: None, position: None, radius: None`. MCP clients can't do spatial or name-based queries.
- **Fix:** Extend `McpOp::QueryEntities` and MCP tool schema.

~~### 60. `available_templates()` doesn't include `forest_clearing`~~ — False positive. Already present with test coverage.
- **Crate:** katla_agent
- **File:** `src/tools/templates.rs` (lines 58-63)
- **Fix:** Add `("forest_clearing", "Ring of trees around a clearing")` to the list.

~~### 88. AI assistant cannot query, add, read, or set generic components and attributes~~ — Fixed in 42902a3. Added ListAvailableComponents, AddComponent, GetComponentAttributes, SetComponentAttribute tools with MCP endpoints and protected entity checks.

~~### 61. AI LLM should not be able to delete the editor camera~~ — Fixed in 409ddf6. Both LLM and MCP paths now check protected entities (camera/gizmo) before executing destructive ops.

~~### 62. AI-spawned cubes don't appear in the 3D scene or hierarchy view~~ — Fixed in 6d4051a. `attach_spawn_visuals()` adds TransformComponent, DrawableComponent with cube mesh, and EntitySource to AI-spawned entities.

~~### 63. Dead `_TAU` constant in placement tools~~ — Fixed in 4486cb4. Removed unused `_TAU` and `PI` import.

~~### 64. Add `nlerp` to `Quat`~~ — Fixed in dcc28d7. Added nlerp() to both SSE and scalar implementations with tests.

~~### 65. Missing `Vec3` direction constants (FORWARD, UP, RIGHT, etc.)~~ — Fixed in b7089dd. Added FORWARD/BACK/UP/DOWN/RIGHT/LEFT to Vec3.
- **Crate:** katla_math
- **Issue:** Common direction constants needed in a 3D engine are missing.
- **Fix:** Add `pub const FORWARD/UP/RIGHT/LEFT/BACK/DOWN`.

~~### 66. Missing `AABB`-Sphere intersection test~~ — Fixed in 7014013. Added intersects_sphere with closest-point algorithm and 4 tests.
- **Crate:** katla_math
- **File:** `src/aabb.rs`
- **Issue:** Has AABB-AABB test but no AABB-Sphere, needed for frustum culling and broadphase.
- **Fix:** Add `pub fn intersects_sphere(&self, sphere: &Sphere) -> bool`.

~~### 67. `Sphere::create_from_verts` allocates unnecessarily~~ — False positive. Bounds already computed inline in single pass without Vec allocation using generic iterator `I: IntoIterator<Item = &'a [f32; 3]>`.

~~### 68. `SparseSet` memory waste with large EntityId gaps~~ — Fixed in 25349ef. Replaced flat Vec with paged sparse array (1024-entry pages), only allocates used pages.

~~### 69. `query_changed` allocates `HashSet<EntityId>` every call~~ — Fixed in a4067ef. Reusable buffer stored in World, avoids per-frame allocation.

~~### 70. `ComponentRegistry::get` is O(n) linear scan by string comparison~~ — Fixed in dcc28d7. Changed from Vec to HashMap for O(1) lookups.

~~### 71. Redundant debug particle readback in frame_loop~~ — Fixed in 9ff0698. Removed direct read_debug_data() call at frame 10, frame graph mechanism handles readback.

~~### 72. `InputMapper` binds `KeyF` to both `Interact` and focus-camera~~ — Fixed in 2659e2e. Removed KeyF->Interact binding; Interact remains on MouseLeft.

---

## P4: Nice-to-Have / Future

~~### 73. Add unit tests for katla_icons (verify ForkAwesome codepoints)~~ — Fixed in a4067ef. Added 4 tests; caught and fixed REDO (F0E3→F01E) and PENCIL_ALT (duplicate→PENCIL_SQUARE F14B).

~~### 74. Add missing angle icons (ANGLE_LEFT, ANGLE_UP, ANGLE_DOWN)~~ — Fixed in b7089dd. Added ANGLE_LEFT, ANGLE_UP, ANGLE_DOWN to ForkAwesome.
- **Crate:** katla_icons
- **Issue:** `ANGLE_RIGHT` exists but directional counterparts are missing. Used for submenu indicators.
- **Fix:** Add `ANGLE_LEFT` (F104), `ANGLE_UP` (F106), `ANGLE_DOWN` (F107).

~~### 75. Add streaming tests for `CoCreatorAgent` and `AsyncBridge`~~ — False positive. `co_creator_test.rs` already has 8 streaming tests covering text chunks, tool call accumulation, errors, truncation, bridge end-to-end, and finalization.

~~### 76. Add MCP server graceful shutdown and error logging~~ — Fixed in b96aee7. Added watch channel shutdown signal, tokio::select! for graceful termination, and log::error for server failures.
- **Crate:** katla_agent
- **File:** `src/mcp.rs` (lines 128-135)
- **Issue:** No way to signal shutdown. `serve_server` errors silently swallowed.
- **Fix:** Accept a `CancellationToken`, log errors on failure.

~~### 77. No keyboard navigation (Tab between widgets)~~ — Fixed in ffd0b78. Widgets register as focusable during layout, Tab/Shift+Tab cycles focus, focus ring visual on focused widget.

~~### 78. `Fast inverse sqrt` uses single Newton-Raphson iteration~~ — Fixed in f74b05c. Added second iteration for ~0.1% accuracy.

~~### 79. `utils.rs` duplicates `f32` methods as free functions~~ — Fixed in 13b353e. Removed 40 unused utility functions, kept only `compute_bounds()`.

### 80. No archetype-based ECS storage for cache-friendly queries
- **Crate:** katla_ecs
- **Issue:** Multi-component queries iterate one sparse set and lookup others (random-access indirection per entity per component). Archetype-based storage would group entities with the same component set into contiguous SoA columns for cache-friendly iteration.
- **Recommendation:** Implement as a parallel/opt-in storage option, not a full replacement. The existing sparse-set system provides excellent O(1) random access. Archetype storage should be opt-in for entity groups where multi-component iteration is a measured bottleneck.
- **Sub-tasks:**
  - [ ] 80a. Core `Archetype` and `ComponentColumn` data structures — type-erased SoA column storage with contiguous iteration (large, medium risk)
  - [ ] 80b. `ArchetypeRegistry` — manages archetype instances, entity-to-archetype mapping, component add/remove migration with edge caching (large, medium risk)
  - [ ] 80c. `ArchetypeQueryData` trait and macro-generated tuple impls — new query entry point `World::archetype_query::<Q>()` alongside existing `query()` (large, high risk)
  - [ ] 80d. Dual-storage World integration — `spawn_archetype()` coexisting with `spawn()`, `get_component()` dispatches to correct storage (medium, medium risk)
  - [ ] 80e. Criterion benchmarks comparing sparse-set vs archetype for 1-4 component queries at 1K/10K/100K entities (small, low risk)

~~### 81. `from_rows` naming in `Mat3` is misleading (actually column-major)~~ — Fixed in 37b3dbc. Renamed to `from_columns`.

~~### 82. Tooltip convenience method on `Response`~~ — Fixed in a4067ef. Added `Response::on_hover_tooltip(ui, text)`.

~~### 83. `collect_draws_with_context` allocates Vec and HashMap every frame~~ — Fixed in b96aee7 + 16568d0. Added reusable draw_entity_map_entries buffer on EditorState and point_lights_buffer on Application.
- **Crate:** katla_app
- **File:** `src/application/renderer.rs` (lines 65-120)
- **Fix:** Reuse buffers between frames by storing on `Application` or `EditorState`.

~~### 84. Popup panels (preferences, AI chat) close when clicking outside them~~ — Fixed in 409ddf6. Added close_on_outside_click option to DraggablePanel, disabled for preferences and AI panels.

~~### 85. Clicks on overlying panels are forwarded to the 3D scene underneath~~ — Fixed in 409ddf6. update_focused_panel_from_click now checks floating panel bounds before forwarding clicks to viewport.

~~### 86. Use serde JSON derives for tool call arguments instead of raw strings~~ — Fixed in 37ab58a. Added typed Deserialize structs (SpawnEntityArgs, DestroyEntityArgs, etc.) replacing all manual args.get() patterns.
- **Crate:** katla_agent
- **File:** `src/llm/mock.rs`, `src/co_creator/mod.rs`
- **Issue:** Tool call arguments and stream chunks use raw `String` / `serde_json::Value` instead of typed serde-deserialized structs. `ToolCallAccumulator` accumulates `arguments` as a `String`, `MockStreamProvider::tool_call()` takes `&str` for arguments, and `tool_call_to_scene_op()` manually extracts fields from `serde_json::Value`. This is error-prone and verbose.
- **Fix:** Define typed argument structs (e.g. `SpawnEntityArgs`, `DestroyEntityArgs`) with `#[derive(Deserialize)]`, accumulate arguments into a `Vec<u8>` / `String` buffer, then deserialize into the typed struct. Remove all manual `args.get("field").and_then(|v| v.as_str())` patterns.

~~### 87. Preferences panel UX reorganization~~ — Fixed in 37ab58a. Consolidated to 3 tabs (General, Viewport, AI), 3-column theme grid, font scale slider, auto-save AI config.
- **Crate:** katla_app
- **Files:** `katla_app/src/ui/editor_ui/preferences.rs`, `katla_app/src/ui/editor_ui/mod.rs`, `katla_app/src/ui/editor_ui/layout.rs`, `katla_app/src/ui/editor_ui/toolbar.rs`, `katla_app/src/preferences.rs`
- **Issue:** The preferences panel has structural UX problems that make it harder to use than it should be:
  1. **Settings scattered across wrong tabs.** "Show Grid" and "Show Stats Panel" are on the Appearance tab but are viewport visibility toggles, not appearance settings. "Snap to Grid" is on the Editor tab while grid visibility is on Appearance. Grid size, grid visibility, and snap-to-grid are the same feature split across two tabs.
  2. **Two tabs contain no settings.** Keybindings is read-only with a "coming soon" message -- raises an expectation it can't fulfill. About is metadata (version, features list), not a setting. Both inflate the tab count without adding configurability.
  3. **Inconsistent save semantics.** Appearance and Editor tabs apply changes instantly. The AI tab requires an explicit "Save Configuration" button. The user doesn't know which model applies where.
  4. **Font Scale uses 8 preset buttons for a continuous value.** 75%-200% in fixed steps wastes space and prevents fine-tuning (e.g. 1.15x). Camera speed correctly uses a slider for the same kind of continuous range.
  5. **Panel title says "Settings" but menu item says "Preferences..."**. Two different names for the same dialog.
  6. **Theme grid uses 2 columns for 13 items** producing 7 rows with a lone orphan button, requiring excessive scrolling. 3 columns would fit in the 450px panel width and reduce to ~5 rows.
  7. **Hardcoded spacing after theme grid** (`ROW_HEIGHT * 7.0 + GRID_SPACING + SECTION_GAP`) breaks if themes are added or removed.
  8. **Editor tab is sparse.** Only 3 controls (snap toggle, camera slider, grid size buttons) while two related viewport controls sit on a different tab.
  9. **About tab hardcodes version string** instead of using `env!("CARGO_PKG_VERSION")`.
  10. **Camera speed slider has no context.** Label says "Speed: 50" but the user has no reference for whether that's slow or fast. Min/max labels would help.
- **Fix:**
  - Reorganize tabs to reflect user mental models:
    - **General** tab: Theme selection, Font Scale (change to slider).
    - **Viewport** tab: Show Grid, Grid Size, Show Stats, Snap to Grid, Camera Speed. All grid-related settings live together.
    - **AI** tab: Keep as-is but make save behavior consistent with other tabs (auto-save on change, remove explicit Save button).
  - Remove Keybindings tab. Move to a standalone Help > Keyboard Shortcuts window (or add it back when editable).
  - Remove About tab. Move to Help > About menu item.
  - Rename panel title to match menu item (both "Preferences" or both "Settings").
  - Change Font Scale from button grid to slider (0.75–2.0, matching camera speed pattern).
  - Change theme grid from 2-column to 3-column layout.
  - Replace hardcoded theme grid spacing with dynamic calculation based on item count.
  - Use `env!("CARGO_PKG_VERSION")` for version string.
  - Add min/max labels to camera speed slider for context.

### 88. Add undo/redo for AI agent actions
- **Crate:** katla_agent / katla_app
- **Issue:** `SceneToolExecutor::execute()` already returns `(ToolResult, UndoGroup)` but the `_undo_group` is discarded in `execute_tool_call()`. No way to reverse AI operations.
- **Key complication:** `attach_spawn_visuals()` adds GPU resources outside the `UndoGroup`. Undo must also release GPU handles tracked in `GpuResourceTracker`.
- **Sub-tasks:**
  - [ ] 88a. Add `agent_undo_stack: Vec<UndoGroup>` and `agent_redo_stack` to `EditorState` (small, low risk)
  - [ ] 88b. Capture UndoGroups in `execute_tool_call()` — change signature to return `(String, Option<UndoGroup>)`, collect into composite per-turn group (small, low risk)
  - [ ] 88c. Handle GPU resource cleanup on undo — store GPU handle metadata per undo entry, release on undo (medium, medium risk)
  - [ ] 88d. Add `undo_last_agent_action()` method calling `SceneToolExecutor::undo()` with GPU cleanup (small, low risk)
  - [ ] 88e. Add "Undo" button in AI co-creator panel, visible when undo stack non-empty (small, low risk)
  - [ ] 88f. Route local actions (`LocalAction::SpawnCube` etc.) through `SceneToolExecutor` so they produce UndoGroups (small, low risk)
  - [ ] 88g. Clear undo/redo stacks on new scene / clear history (trivial, no risk)
  - **Recommended order:** 88a → 88b → 88c → 88d → 88e → 88f → 88g

---

## UI Review: Fixes & Improvements

### 89. Deduplicate `Theme` in katla_app and `UiStyle`/`ColorScheme` in katla_ui
- **Crate:** katla_app / katla_ui
- **Files:** `katla_app/src/ui/theme.rs`, `katla_ui/src/style.rs`
- **Issue:** `Theme` and `ColorScheme`/`UiStyle` define overlapping color sets for the same UI elements (buttons, panels, text, selections, popups, etc.). `Theme::apply_to_style()` manually maps each field, and `DraggablePanelStyle` in `widgets/draggable_panel.rs` duplicates yet a third set of panel colors. Three separate color definitions for "button background" is a maintenance trap — adding a new theme requires updating all three.
- **Fix:** Extend `ColorScheme` in katla_ui with the editor-specific colors that `Theme` adds (entity type colors, status colors, viewport border, etc.). Remove `Theme` from katla_app entirely and have the 13 named themes as `ColorScheme` constructors. Remove `DraggablePanelStyle` in favor of reading from `ui.style`. `apply_to_style()` becomes unnecessary since the style IS the color scheme.

### 90. `DraggablePanelStyle` is a redundant copy of `Theme` panel colors
- **Crate:** katla_ui
- **File:** `katla_ui/src/widgets/draggable_panel.rs`
- **Issue:** Every call site constructs a `DraggablePanelStyle` by copying colors from `Theme` (e.g., `DraggablePanelStyle { panel_bg: theme.panel_bg, ... }`). The panel should read directly from `ui.style` fields like `window_bg`, `window_border`, `window_title_bg`, `button_text`, etc. This eliminates 8 fields of duplicate state and makes `DraggablePanel::show()` simpler to call.
- **Fix:** Remove `DraggablePanelStyle` struct. Have `DraggablePanel::show()` take only the config + state, and read colors from `ui.style` internally.

### 91. `Response::on_hover_tooltip` takes `&mut UiContext` — deferred tooltip API
- **Crate:** katla_ui
- **Issue:** UI-23 in the existing UI TODO list identifies this. Adding here as a concrete actionable item since it affects ergonomics across the editor. Currently `resp.on_hover_tooltip(ui, "text")` works, but in many call sites (e.g., `if resp.hovered { ui.tooltip("text"); }`) the borrow is manually split. A deferred tooltip stored on the response or context would clean up many patterns.
- **Fix:** Store a `tooltip_text: Option<String>` on the Response or UiContext. Add `Response::tooltip(self, text)` that stores the text. In `end()`, render all pending tooltips at z_index::TOOLTIP. This removes the need for `&mut UiContext` at the tooltip call site.

### 92. `UiContext::add()` always advances cursor — provide opt-out for overlay widgets
- **Crate:** katla_ui
- **Files:** `katla_ui/src/context/mod.rs`, `katla_ui/src/widgets/mod.rs`
- **Issue:** `add()` calls `advance_cursor()` after every widget. Overlay widgets like `Separator`, `Badge`, and custom overlays that position themselves manually still advance the layout cursor. This makes it impossible to place a badge overlaying a button without the cursor jumping. Other immediate-mode UIs like egui differentiate between "sized" and "unsized" widgets.
- **Fix:** Add `add_sized()` (advances cursor) and `add_overlay()` (does not advance). Keep `add()` as `add_sized()` for backward compatibility. Alternatively, let widgets return an `Option<Vec2>` size — `None` means don't advance.

### 93. `text_input` borrows `self.input` fields individually to avoid borrow conflicts
- **Crate:** katla_ui
- **File:** `katla_ui/src/context/widgets/basic.rs`
- **Issue:** Related to UI-09 in the existing list. The `text_input()` method snapshots ~20 individual input fields into local variables before the mutable borrow of `self.text_input_states`. This pattern is fragile — adding a new input field requires remembering to snapshot it. The root cause is that `self.input` and `self.text_input_states` are both fields of `UiContext`, so borrowing both mutably triggers borrow checker conflicts.
- **Fix:** Extract the text input editing logic into a standalone function that takes `(text: &mut String, state: &mut TextInputState, input: &UiInputState, clipboard: &mut dyn ClipboardProvider, max_len: usize)` and returns `(changed, enter_pressed)`. Call it from `text_input()` after the ID/state setup. This eliminates all 20 snapshot variables.

### 94. `DrawList::convert_draw_list` in katla_app assigns texture indices per-vertex inefficiently
- **Crate:** katla_app
- **File:** `katla_app/src/ui/renderer.rs`
- **Issue:** `convert_draw_list()` iterates all indices in all commands to mark vertex texture indices, creating a `Vec<u32>` with one entry per vertex. For a frame with 10K vertices and 50 commands, this is O(commands * indices_per_command + vertices). The draw list already guarantees that all vertices in a command share the same texture, so the mapping is command-based, not vertex-based.
- **Fix:** Build a `Vec<u32>` of per-command bindless indices (one per command, not per vertex). During vertex conversion, determine which command a vertex belongs to by binary search on `index_offset`. Or better: iterate commands and batch-convert vertices per command, avoiding the per-vertex lookup entirely.

### 95. No `draw_rounded_rect` in UiContext despite style having rounding fields
- **Crate:** katla_ui
- **Files:** `katla_ui/src/context/drawing.rs`, `katla_ui/src/style.rs`
- **Issue:** `UiStyle` defines `window_rounding`, `button_rounding`, `input_rounding`, `popup_rounding`, and `menu_rounding` but no widget uses them. All rectangles are drawn with sharp corners. The `DrawList` has no rounded rect primitive (UI-24 mentions pre-tessellated corners). This makes the UI look blockier than intended.
- **Fix:** Add `UiContext::draw_rounded_rect(bounds, color, radius)` that tessellates the rect with corner arcs. Use `DrawList::add_convex_poly` for each corner quadrant. Update `button_with_colors()`, `checkbox()`, `text_input()`, combo box, and popup backgrounds to use their respective `style.*_rounding` values.

~~### 96. `container.rs` `begin_window` hardcodes title bar height instead of using `style.title_bar_height`~~ — Fixed in bd0f57a. Now reads `self.style.title_bar_height`.

~~### 97. `graph.rs` hardcodes `label_height = 18.0` and `padding = 3.0` instead of using style fields~~ — Fixed in bd0f57a. Now reads `self.style.graph_label_height` and `self.style.graph_padding`.

~~### 98. `DraggablePanel` hardcodes `TITLE_BAR_HEIGHT = 32.0` instead of using `style.title_bar_height`~~ — Fixed in bd0f57a. Removed const, now reads `ui.style.title_bar_height`.

~~### 99. `DraggablePanel::show` calls `push_z_index`/`pop_z_index` manually instead of using `z_guard` or `with_z_index`~~ — Fixed. Restructured to use `ui.with_z_index()` with close/outside-click handling after the block.

### 100. `begin_window`/`end_window` has no RAII guard — clip leak on early return
- **Crate:** katla_ui
- **File:** `katla_ui/src/context/widgets/container.rs`
- **Issue:** `begin_window()` calls `push_clip()` but relies on the user calling `end_window()` for `pop_clip()`. If code between begin/end returns early or panics, the clip stack is corrupted. Every other push/pop pattern in the codebase (z_index, layout) has RAII guards.
- **Fix:** Return a `WindowGuard` struct from `begin_window()` that implements `Drop` and calls `pop_clip()`. The guard provides `content_cursor()` and `bounds()` accessors.

~~### 101. `scroll_area` scrollbar width hardcoded to `10.0` in two places~~ — Fixed. Added `scrollbar_width: f32` to `UiStyle`, replaced all hardcoded values.

### 102. Slider lacks value label and format customization
- **Crate:** katla_ui
- **Files:** `katla_ui/src/context/widgets/basic.rs`, `katla_ui/src/widgets/mod.rs`
- **Issue:** The slider has no visible value display. Users can't see the current value while dragging. The `Slider` builder has no `.format()` or `.show_value()` method. Every slider in the editor (camera speed, font scale, transform sliders) needs to manually draw the value text alongside the slider.
- **Fix:** Add `.show_value(bool)` and `.format(fn(f32) -> String)` to `Slider` builder. In `slider()`, when `show_value` is true, draw the formatted value text centered on or beside the grab handle.

### 103. `FontSize::to_pixels()` is not used consistently — raw `f32` font sizes leak into the API
- **Crate:** katla_ui
- **Issue:** `UiStyle` stores `font_size: f32` in pixels, `FontSize` enum converts to pixels, `scaled_font_size()` takes `FontSize` and returns `f32`, but `draw_text()`, `measure_text()`, `draw_icon()` all take raw `f32` size. The `XSmall`/`Small`/`Medium`/`Large`/`XLarge` enum is only used in ~5 places. Raw pixel values like `14.0` and `12.0` appear throughout the codebase.
- **Fix:** This is a design choice, not a bug, but consider: add `draw_text_styled(text, pos, color, FontSize)` and `measure_text_styled(text, FontSize)` convenience methods that call `to_pixels_scaled(font_scale)` internally. This gives a typed entry point for the common case while keeping the raw f32 for advanced use.

### 104. `FontSystem` is embedded in `UiContext` — prevents sharing font data across contexts
- **Crate:** katla_ui
- **File:** `katla_ui/src/context/mod.rs`
- **Issue:** `FontSystem` contains the font atlas texture, glyph cache (HashMap with thousands of entries), and loaded font data. It's owned by `UiContext` and recreated if you create a new context. In a multi-window scenario (future), each window would duplicate all font data. The font atlas texture is also separate from the `UIRenderer` texture registry, requiring manual sync.
- **Fix:** Wrap `FontSystem` in `Arc<RwLock<FontSystem>>` or use a handle-based approach so multiple `UiContext` instances can share the same font data. Alternatively, move `FontSystem` out of `UiContext` and pass it as a reference to `begin()`.

### 105. `MarkdownColors` is not derived from the current `UiStyle`/`Theme`
- **Crate:** katla_ui
- **File:** `katla_ui/src/markdown.rs`
- **Issue:** `MarkdownColors::defaults()` uses hardcoded blue/green colors that don't match any theme. When the user switches to Nord or Tokyo Night, markdown text still uses the same blue accent. `draw_markdown_segments` requires passing colors explicitly instead of reading from the style.
- **Fix:** Add `MarkdownColors::from_style(style: &UiStyle)` that derives bold/code/header/bullet colors from the style's accent colors (e.g., `input_border_focused` for bold, `slider_grab` for code text, `text_color` for headers). Update callers to use it.

~~### 106. `draw_icon_label` hardcoded spacing `4.0` between icon and text~~ — Fixed. Now uses `self.style.item_inner_spacing`.

~~### 107. `checkbox()` label offset hardcoded to `8.0` from check bounds~~ — Fixed in bd0f57a. Now reads `self.style.item_inner_spacing`.

~~### 108. `ListView` virtualization y-offset calculation ignores content padding~~ — Fixed. Uses `ui.style.item_inner_spacing` for symmetric top/bottom padding and updated virtualization row calculation.

### 109. `begin_grid`/`end_grid` doesn't restore cursor X position correctly
- **Crate:** katla_ui
- **File:** `katla_ui/src/context/layout.rs`
- **Issue:** `end_grid()` sets `self.cursor = Vec2::new(layout.start_pos.x(), layout.cursor.y() + item_height + layout.spacing)`. If the grid is inside a horizontal row, the cursor Y jumps down but the parent row doesn't know about the grid's height. This causes overlapping widgets when a grid is placed inside a row.
- **Fix:** After popping the layout, update `self.row_height` with the total grid height. Same pattern as `end_row()` which updates `row_height`.

### 110. `separator_line()` in helpers reads clip rect for width — fragile with nested clips
- **Crate:** katla_ui
- **File:** `katla_ui/src/context/helpers.rs`
- **Issue:** `separator_line()` uses `clip.min.x()` and `clip.max.x()` to determine line endpoints. If called inside a scroll area or popup, the clip rect is the scroll content area, which may be much larger than the intended panel width. The separator line extends across the entire clip region, potentially overflowing the visual panel boundary.
- **Fix:** Accept an explicit `width` parameter or use `available_width()` (if UI-07 is implemented). Alternatively, track the "content width" set by the current container (window/panel) and use that instead of clip rect.

### 111. `DrawList::add_circle` takes `segments` as a count — should auto-calculate from radius
- **Crate:** katla_ui
- **File:** `katla_ui/src/draw_list.rs`
- **Issue:** Callers pass arbitrary segment counts (`16`, `12` in radio button). Small circles with 12 segments look polygonal; large circles with 16 segments look faceted. The ideal segment count scales with the circle's screen-space size.
- **Fix:** Add `add_circle_auto(center, radius, color)` that calculates segments as `(radius * std::f32::consts::PI * 2.0 / 4.0).ceil().max(8.0) as u32` (4px per segment, minimum 8). Keep `add_circle` for callers that want explicit control.

### 112. `ClipRect` duplicates `Rect2D` functionality — consider using `Rect2D` directly in `DrawCmd`
- **Crate:** katla_ui
- **Files:** `katla_ui/src/types.rs`, `katla_ui/src/draw_list.rs`
- **Issue:** `ClipRect` has `x, y, width, height` fields and `to_array()`. `Rect2D` has `min, max` with `width()`/`height()` methods. `DrawCmd.clip_rect` is `Option<[f32; 4]>` (a raw array) instead of `Option<Rect2D>`. The conversion chain is: `Rect2D -> ClipRect -> [f32; 4]` but could be simplified to `Rect2D -> [f32; 4]` directly.
- **Fix:** Remove `ClipRect` struct. Change `DrawCmd.clip_rect` to `Option<Rect2D>` or keep the array but add a `Rect2D::to_clip_array()` method. Remove the `ClipRect` intermediary in `DrawList::finalize()`.

---

## UI Review: What Belongs in katla_ui vs katla_app

These items identify code that currently lives in katla_app but is generic enough to belong in katla_ui — or missing pieces in katla_ui that a second app consumer would need to reinvent.

### 113. Move `Theme` and its 13 named color schemes into katla_ui
- **Crate:** katla_app → katla_ui
- **Files:** `katla_app/src/ui/theme.rs` → `katla_ui/src/style/`
- **Issue:** The 13 named themes (Catppuccin, Nord, Tokyo Night, Dracula, Gruvbox, One Dark, Material Palenight, Ayu Dark, GitHub Dark, Monokai, Rose Pine, Kanagawa, Solarized Dark) are pure color data with no dependency on katla_app. Any app using katla_ui would want theme presets. Right now they're locked behind katla_app's editor feature gate. The `theme!` macro and `Theme::by_name()` + `Theme::all_names()` are fully self-contained. Ties into #89 (deduplicating Theme vs ColorScheme) — if we extend `ColorScheme` to cover all editor colors, these 13 themes become `ColorScheme` constructors and live naturally in katla_ui.
- **Fix:** Extend `ColorScheme` with the editor-specific semantic fields (status colors, entity type colors, viewport border, accent). Move the 13 theme constructors into `katla_ui/src/style/themes.rs` as `ColorScheme::catppuccin()`, etc. Remove `Theme` from katla_app. `apply_to_style()` becomes a one-liner: `UiStyle::with_colors(ColorScheme::nord())`.

### 114. Add `Panel` / `PanelHeader` widget to katla_ui
- **Crate:** katla_ui (new)
- **Issue:** Every panel in the editor (hierarchy, inspector, asset browser, preferences, particle inspector, co-creator) hand-rolls the same pattern: draw `panel_bg` rect, draw `panel_border`, draw `panel_header` rect at the top, draw title text centered in the header. This is ~15 lines repeated verbatim in 6 places. A second app would need the same pattern.
- **Fix:** Add a `Panel` builder widget to katla_ui:
  ```rust
  ui.add(Panel::new("Hierarchy")
      .bounds(panel_bounds)
      .header_height(24.0)
      .subtitle(&format!("({} entities)", count))
      .content(|ui, content_bounds| {
          // draw panel contents in content_bounds
      }));
  ```
  Internally draws bg, border, header rect, title text. Returns a `PanelGuard` (RAII for clip). Reads all colors from `ui.style`.

### 115. Add `LabeledSlider` widget to katla_ui
- **Crate:** katla_ui (new)
- **Issue:** The `inspector.rs` file defines `vec3_slider_row()` and `scalar_slider_row()` — local functions that compose a label, a `Slider`, and a value display into a row. This is the most common slider usage pattern in any editor UI (every property inspector, every settings panel). The `toolbar.rs`/`preferences.rs` build the same pattern manually. A second app would need it too. Related to #102 (slider value display).
- **Fix:** Add `LabeledSlider` builder widget to katla_ui:
  ```rust
  ui.add(LabeledSlider::new("Intensity", &mut value, 0.0..=100.0)
      .bounds(row_bounds)
      .label_width(90.0)
      .show_value(true)
      .precision(2));
  ```
  And a `Vec3Slider` for X/Y/Z rows:
  ```rust
  ui.add(Vec3Slider::new("Position", &mut [x, y, z], -100.0..=100.0)
      .bounds(section_bounds)
      .axis_labels(["X", "Y", "Z"]));
  ```

### 116. Add `Selectable` list item widget to katla_ui (cross-reference UI-15)
- **Crate:** katla_ui
- **Issue:** UI-15 already identifies this gap. Adding more context: the hierarchy panel, asset browser, and any future list UI all implement the same selectable-item pattern manually: check hover, draw selection bg, handle click, handle right-click. A generic `Selectable` widget that handles highlight-on-hover, click, right-click, selected state, and drag detection would eliminate hundreds of lines of ad-hoc interaction code across the editor.
- **Fix:** Add `Selectable` widget:
  ```rust
  let resp = ui.add(Selectable::new("item_label")
      .bounds(item_bounds)
      .selected(is_selected)
      .interactive(true));
  if resp.clicked { /* select */ }
  if resp.right_clicked { /* context menu */ }
  ```
  Internally reads `style.selectable_hovered`/`selectable_selected` for colors. Adds `right_clicked: bool` to `Response` (currently missing — right-click is checked via `ui.input.mouse_clicked(RIGHT)`).

### 117. Add `right_clicked` and `drag_started` / `drag_ended` to `Response`
- **Crate:** katla_ui
- **File:** `katla_ui/src/response.rs`
- **Issue:** `Response` has `clicked` (left click) and `double_clicked` but no `right_clicked`. Every widget that handles right-click (hierarchy items, asset items, entity rows) does `if resp.hovered && ui.mouse_clicked(RIGHT)` manually. Similarly, there's no `drag_started`/`drag_ended` — the `DraggablePanel` and asset browser implement drag detection ad-hoc. A second consumer would need these primitives.
- **Fix:** Add `right_clicked: bool` and `middle_clicked: bool` fields to `Response`. Populate them in `Response::interactive()`. For drag: add `drag_started: bool` (true on the frame active_id is first set while mouse moves past a threshold) and `drag_ended: bool` (true on the frame active_id is released after a drag).

### 118. Add `MenuBar` widget to katla_ui
- **Crate:** katla_ui
- **Issue:** The toolbar in `katla_app` manually draws a horizontal bar and places `menu_bar_dropdown()` calls with manual spacing and cursor management. This is a standard editor pattern. A `MenuBar` widget would provide the common container with automatic layout.
- **Fix:** Add `MenuBar` builder:
  ```rust
  ui.add(MenuBar::new(screen_size, toolbar_height)
      .menu("File", |ui, open| { /* items */ })
      .menu("Edit", |ui, open| { /* items */ })
      .menu("View", |ui, open| { /* items */ })
      .right_side(|ui| {
          ui.draw_text("Katla Engine", ...);
      }));
  ```
  Handles horizontal layout, z-index, styling from `ui.style`.

### 119. Add `StatusBar` widget to katla_ui
- **Crate:** katla_ui
- **Issue:** `status_bar.rs` builds a standard status bar: background rect, top border, left-aligned items (FPS, frame count, entities), right-aligned items (mode indicator, theme name). This is the same in any editor. The widget reads from `Theme` for colors — with #113, it would read from `ui.style`.
- **Fix:** Add `StatusBar` builder:
  ```rust
  ui.add(StatusBar::new(screen_size, 24.0)
      .left_items(|ui| {
          ui.status_label(&format!("FPS: {:.0}", fps), fps_color);
          ui.status_separator();
          ui.status_label(&format!("Frame: {}", count));
      })
      .center_item(|ui| {
          if save_timer > 0.0 { ui.status_label("✓ Saved", success_color); }
      })
      .right_items(|ui| {
          ui.status_label("EDITING", mode_color);
      }));
  ```

### 120. Add `panel_header` / `section_header` helper to katla_ui context
- **Crate:** katla_ui
- **File:** `katla_ui/src/context/helpers.rs`
- **Issue:** `helpers.rs` already has `header()` and `section()` but they're minimal — just text + spacing. The inspector and hierarchy panels need a styled header with background color, proper vertical centering, and optional icon/badge. Six panels in the editor build this pattern manually. With #114 (Panel widget) the header is internal, but a standalone `draw_panel_header()` helper for custom panels is still useful.
- **Fix:** Add `draw_panel_header(ui, bounds, title, icon: Option<char>)` that draws the header background from `style.window_title_bg`, centers text vertically, optionally draws an icon, and returns the content area below.

### 121. Add `ResizablePanel` / resize handle interaction to katla_ui
- **Crate:** katla_ui
- **Issue:** `layout.rs` in the editor has ~80 lines of resize handle logic (left panel, right panel, asset browser). It tracks `resizing_panel: Option<PanelResizeEdge>`, clamps widths, and changes the cursor to resize cursors. Any editor with side panels needs this. Currently it's raw mouse-state checking scattered across `build()`.
- **Fix:** Add a `ResizeHandle` widget:
  ```rust
  let new_width = ui.add(ResizeHandle::horizontal(resize_bounds, current_width)
      .min_width(150.0)
      .max_width(600.0));
  self.left_panel_width = new_width;
  ```
  Handles hover cursor change, drag, clamping, and returns the new dimension. Also `ResizeHandle::vertical()` for asset browser.

### 122. Add `truncate_text` utility to katla_ui
- **Crate:** katla_ui
- **File:** Currently in `katla_app/src/ui/editor_ui/asset_browser/mod.rs`
- **Issue:** `truncate_text()` measures text and adds "..." when it overflows a max width. The asset browser needs it, but any grid/list UI with constrained cell widths would need it too. This is a text layout primitive that belongs next to `measure_text()`.
- **Fix:** Add `UiContext::truncate_text(text, max_width, font_size) -> String` or a standalone `truncate_text(text, max_width, |t| measure(t)) -> Cow<str>` in the text module.

### 123. Add `draw_empty_state` / centered placeholder text to katla_ui
- **Crate:** katla_ui
- **Issue:** Four places in the editor draw centered "No entities in scene" / "No assets found" / "No matching assets" / "No entity selected" text. All follow the same pattern: `measure_text`, compute center, `draw_text` with `text_muted` color. This is a universal empty-state pattern.
- **Fix:** Add `ui.draw_empty_state(bounds, "No items found")` that measures, centers, and draws with `style.text_disabled`.

### 124. Add `FocusablePanel` / panel focus tracking to katla_ui
- **Crate:** katla_ui
- **Issue:** Every panel in the editor (hierarchy, inspector, asset browser, viewport) checks `if ui.is_hovered(bounds) && (mouse_down[LEFT] || mouse_down[RIGHT] || mouse_down[MIDDLE]) { *focused_panel = FocusedPanel::X; }` on every frame. This is the same 5-line pattern repeated 5 times. A second app with panels would need it. The focus tracking itself (`FocusedPanel` enum) is app-specific, but the hover-click detection and focus ring drawing could be provided by the UI layer.
- **Fix:** Add `ui.register_panel("hierarchy", bounds)` in the widget. After `end()`, the app can query `ui.focused_panel()` to get the ID of the topmost panel the mouse clicked in. The UI tracks this internally by checking hover + click across all registered panels. Optionally draws a focus ring using `style.focus_ring_color`.

### 125. Add `TreeNode` / `TreeView` widget to katla_ui (extends UI-19)
- **Crate:** katla_ui
- **Issue:** UI-19 identifies the need for a Tree widget. Adding implementation context from the hierarchy: the hierarchy panel manually handles indentation (depth * 16.0px), tree guide lines, expand/collapse icons, child visibility filtering via `is_entity_visible()`, and depth-aware click targets. A `TreeNode` widget would handle all of this generically, leaving the app to provide only the data (name, icon, depth, has_children, is_expanded).
- **Fix:** Add `TreeView` virtualized tree widget:
  ```rust
  ui.add(TreeView::new("hierarchy", &mut scroll_state)
      .bounds(content_bounds)
      .row_height(22.0)
      .indent_per_level(16.0)
      .data(tree_data)  // vec of TreeItem { id, label, depth, has_children }
      .expanded(&expanded_set)
      .selected(selected_id)
      .render_item(|ui, index, item_bounds, item, state| {
          // custom rendering per item
      }));
  ```
  Handles indentation, expand/collapse toggle, selection, keyboard navigation.
