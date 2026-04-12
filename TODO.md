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

~~### 21. No undo/redo system in the editor~~ — Fixed. All subtasks complete.
- **Crate:** katla_app
- **Issue:** Destructive operations (delete entity, transform changes) have no undo. Slider drags now mutate ECS directly (#54 fix), but there is no undo stack to reverse them.
- **Existing infrastructure:** `UndoGroup`/`SceneCommand` already exist in `katla_ecs/src/scene_tool/command.rs` with spawn, destroy, set-field, duplicate commands. `SceneToolExecutor::execute()` already returns `(ToolResult, UndoGroup)`. `AgentSession` has `push_undo()`/`undo_last()`/`undo_all()` pattern.
- **Sub-tasks:**
  - [x] 21a. Add `undo_stack: Vec<UndoGroup>` and `redo_stack: Vec<UndoGroup>` to `EditorState`, with `push_undo()`, `perform_undo()`, `perform_redo()` helpers (small, low risk) — Done in f34a0d0. Also added `redo_all()` to `UndoGroup`.
  - [x] 21b. Add Ctrl+Z / Ctrl+Shift+Z keyboard shortcuts in `handle_editor_keyboard_shortcuts()` (small, low risk) — Done in f34a0d0. Guards with prev_want_capture_keyboard.
  - [x] ~~21c. Capture UndoGroups from `EditorAction::DeleteEntity`, `DuplicateEntity`, `SpawnModel` in `process_editor_actions()` via `ComponentRegistry` snapshots~~ — Done in 771aeef (as part of 88f). All three actions now push UndoGroups to undo_stack.
  - [x] ~~21d. Capture slider drag start/end values for undo — snapshot pre-drag ECS values on drag start, push `SetFieldCommand`-based `UndoGroup` on drag end~~ — Done in 8313e3e. InspectorDragSnapshot + InspectorDragUndo command pushed on drag end.
  - [x] ~~21e. Add Undo/Redo items to Edit menu in toolbar~~ — Already implemented. Toolbar has Undo/Redo with icons, keyboard shortcuts, and enabled/disabled states.
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

~~### 27. No parent-child entity hierarchy in ECS~~ — Fixed. All subtasks complete.
- **Crate:** katla_ecs / katla_app
- **Issue:** `SceneOp::GetSceneHierarchy` returns all entities flat. `Parent`/`Children` components already exist in `katla_app/src/components/scene/relationship.rs` with serialization and transform hierarchy support, but there is no `SetParent` scene op, no automatic hierarchy maintenance on destroy/duplicate, and no structured hierarchy output.
- **Sub-tasks:**
  - [x] 27a. Add `SceneOp::SetParent { entity, parent: Option<EntityId> }` with cycle detection and automatic `Parent`/`Children` maintenance (medium, low risk) — Done in f34a0d0. Executor validates entities, set_parent_components() maintains Parent/Children with cycle detection. Tool/MCP endpoints added.
  - [x] 27b. Rewrite `exec_hierarchy()` to return structured JSON tree with parent/depth info instead of flat list (small, low risk) — Done in f34a0d0. build_hierarchy_json() returns recursive tree with id/name/depth/children.
  - [x] ~~27c. Update `exec_destroy` to clean up `Parent`/`Children` of destroyed entity~~ — Already implemented. `process_editor_actions()` calls `cleanup_entity_hierarchy()` before destroy, and `execute_tool_call()` does the same for AI actions.
  - [x] ~~27d. Update `exec_duplicate` to optionally preserve hierarchy~~ — Already implemented. Both `process_editor_actions()` and `execute_tool_call()` call `set_parent_components()` with the source entity's parent after duplication.
  - [x] ~~27e. Add `set_parent` agent tool and MCP endpoint~~ — Already implemented. `set_parent` tool, MCP endpoint, `SetParentArgs`, `SceneOp::SetParent`, and system prompt entry all exist.

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
  - [x] ~~80e. Criterion benchmarks comparing sparse-set vs archetype for 1-4 component queries at 1K/10K/100K entities~~ — Done in 8313e3e. 7 benchmark groups at 1K/10K/100K scale.

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

~~### 88. Add undo/redo for AI agent actions~~ — Fixed. All subtasks complete.
- **Crate:** katla_agent / katla_app
- **Issue:** `SceneToolExecutor::execute()` already returns `(ToolResult, UndoGroup)` but the `_undo_group` is discarded in `execute_tool_call()`. No way to reverse AI operations.
- **Key complication:** `attach_spawn_visuals()` adds GPU resources outside the `UndoGroup`. Undo must also release GPU handles tracked in `GpuResourceTracker`.
- **Sub-tasks:**
  - [x] ~~88a. Add `agent_undo_stack` and `agent_redo_stack` to `EditorState`~~ — Done in 62c2288.
  - [x] ~~88b. Capture UndoGroups in `execute_tool_call()`~~ — Done in 62c2288. Undo groups pushed to agent_undo_stack, redo cleared.
  - [x] ~~88c. Handle GPU resource cleanup on undo — store GPU handle metadata per undo entry, release on undo~~ — Done in a6a8a9f. GpuCleanupData tracked per entity, released after undo when entity destroyed.
  - [x] ~~88d. Add `undo_last_agent_action()` method~~ — Done in 7c4281a. perform_agent_undo and perform_agent_redo on EditorState.
  - [x] ~~88e. Add "Undo" button in AI co-creator panel~~ — Done in d7beebd. Visible when undo stack non-empty, pops and calls SceneToolExecutor::undo().
  - [x] ~~88f. Route local actions (`LocalAction::SpawnCube` etc.) through `SceneToolExecutor` so they produce UndoGroups~~ — Done in 771aeef. SpawnModel, DeleteEntity, DuplicateEntity now capture UndoGroups via push_undo().
  - [x] ~~88g. Clear undo/redo stacks on new scene~~ — Done in 62c2288. Both stacks cleared in NewScene handler.
  - **Recommended order:** 88a → 88b → 88c → 88d → 88e → 88f → 88g

---

## UI Review: Fixes & Improvements

~~### 89. Deduplicate `Theme` in katla_app and `UiStyle`/`ColorScheme` in katla_ui~~ — Fixed. All subtasks complete.
- **Crate:** katla_app / katla_ui
- **Files:** `katla_app/src/ui/theme.rs`, `katla_ui/src/style.rs`
- **Issue:** `Theme` and `ColorScheme`/`UiStyle` define overlapping color sets for the same UI elements (buttons, panels, text, selections, popups, etc.). `Theme::apply_to_style()` manually maps each field, and `DraggablePanelStyle` in `widgets/draggable_panel.rs` duplicates yet a third set of panel colors. Three separate color definitions for "button background" is a maintenance trap — adding a new theme requires updating all three.
- **Sub-tasks:**
  - [x] ~~89a. Extend `ColorScheme` with editor-specific semantic fields (status colors, entity type colors, accent, highlight, viewport border) and add `from_style()`/`apply_to_style()` round-trip~~ — Done in a6a8a9f. 20 new fields with from_style/apply_to_style methods.
  - [x] ~~89b. Convert all 13 `Theme` constructors to `ColorScheme` constructors using the `theme!` macro pattern~~ — Done in ae097ee. 13 ColorScheme constructors with color_scheme! macro.
  - [x] ~~89c. Remove `DraggablePanelStyle`~~ — Done in cb0d7cc. DraggablePanel now reads from ui.style directly.
  - [x] ~~89d. Replace `Theme` usage across katla_app with `ColorScheme` + `UiStyle::with_colors()`, remove `katla_app/src/ui/theme.rs`~~ — Done in ae097ee. All Theme refs replaced, theme.rs deleted (428 lines removed).
  - **Recommended order:** 89a → 89b → 89c → 89d

### ~~90. `DraggablePanelStyle` is a redundant copy of `Theme` panel colors~~
- Removed in cb0d7cc. DraggablePanel now reads directly from `ui.style`.
- **Crate:** katla_ui
- **File:** `katla_ui/src/widgets/draggable_panel.rs`
- **Issue:** Every call site constructs a `DraggablePanelStyle` by copying colors from `Theme` (e.g., `DraggablePanelStyle { panel_bg: theme.panel_bg, ... }`). The panel should read directly from `ui.style` fields like `window_bg`, `window_border`, `window_title_bg`, `button_text`, etc. This eliminates 8 fields of duplicate state and makes `DraggablePanel::show()` simpler to call.
- **Fix:** Remove `DraggablePanelStyle` struct. Have `DraggablePanel::show()` take only the config + state, and read colors from `ui.style` internally.

~~### 91. `Response::on_hover_tooltip` takes `&mut UiContext` — deferred tooltip API~~ — Fixed. All subtasks complete.
- **Crate:** katla_ui
- **Issue:** UI-23 in the existing UI TODO list identifies this. Adding here as a concrete actionable item since it affects ergonomics across the editor. Currently `resp.on_hover_tooltip(ui, "text")` works, but in many call sites (e.g., `if resp.hovered { ui.tooltip("text"); }`) the borrow is manually split. A deferred tooltip stored on the response or context would clean up many patterns.
- **Sub-tasks:**
  - [x] ~~91a. Add `pending_tooltips` to `UiContext`, add `Response::tooltip()`~~ — Done in 72821c8. Deferred tooltip with rendering in end().
  - [x] ~~91b. Render pending tooltips in `end()` at `z_index::TOOLTIP`~~ — Done in 72821c8 (part of 91a).
  - [x] ~~91c. Migrate existing `on_hover_tooltip()` callers to the deferred API~~ — No callers to migrate outside katla_ui itself. Both old and new APIs coexist.
  - **Recommended order:** 91a → 91b → 91c

~~### 92. `UiContext::add()` always advances cursor — provide opt-out for overlay widgets~~ — Fixed in 600ca16. Added add_overlay() that skips cursor advance.

~~### 93. `text_input` borrows `self.input` fields individually to avoid borrow conflicts~~ — Fixed. All subtasks complete.
- **Crate:** katla_ui
- **File:** `katla_ui/src/context/widgets/basic.rs`
- **Issue:** Related to UI-09 in the existing list. The `text_input()` method snapshots ~20 individual input fields into local variables before the mutable borrow of `self.text_input_states`. This pattern is fragile — adding a new input field requires remembering to snapshot it. The root cause is that `self.input` and `self.text_input_states` are both fields of `UiContext`, so borrowing both mutably triggers borrow checker conflicts.
- **Sub-tasks:**
  - [x] ~~93a. Extract `apply_text_edits` as a standalone free function~~ — Done in b49f35e. TextInputInput struct + snapshot function + apply_text_edits free function.
  - [x] ~~93b. Refactor `text_input()` to call the extracted function~~ — Done in b49f35e (part of 93a). Snapshot variables replaced with TextInputInput struct.
  - **Recommended order:** 93a → 93b

~~### 94. `DrawList::convert_draw_list` in katla_app assigns texture indices per-vertex inefficiently~~ — Fixed in 600ca16. Per-command vertex range scan replaces per-index loop.

~~### 95. No `draw_rounded_rect` in UiContext despite style having rounding fields~~ — Fixed. All subtasks complete.
- **Crate:** katla_ui
- **Files:** `katla_ui/src/context/drawing.rs`, `katla_ui/src/style.rs`
- **Issue:** `UiStyle` defines `window_rounding`, `button_rounding`, `input_rounding`, `popup_rounding`, and `menu_rounding` but no widget uses them. All rectangles are drawn with sharp corners. The `DrawList` has no rounded rect primitive (UI-24 mentions pre-tessellated corners). This makes the UI look blockier than intended.
- **Sub-tasks:**
  - [x] ~~95a. Add `DrawList::add_rounded_rect(bounds, color, radius)`~~ — Done in b49f35e. Corner arc tessellation with auto-segment calculation and add_rect fallback.
  - [x] ~~95b. Add `UiContext::draw_rounded_rect` wrapper, update widgets to use style rounding~~ — Done in 72821c8. Button, slider, text_input, combo, popup now use rounded rendering.
  - [x] ~~95c. Update popup/menu background rendering to use style rounding~~ — Done in 72821c8 (part of 95b). draw_popup_background uses draw_rounded_rect with popup_rounding.
  - **Recommended order:** 95a → 95b → 95c

~~### 96. `container.rs` `begin_window` hardcodes title bar height instead of using `style.title_bar_height`~~ — Fixed in bd0f57a. Now reads `self.style.title_bar_height`.

~~### 97. `graph.rs` hardcodes `label_height = 18.0` and `padding = 3.0` instead of using style fields~~ — Fixed in bd0f57a. Now reads `self.style.graph_label_height` and `self.style.graph_padding`.

~~### 98. `DraggablePanel` hardcodes `TITLE_BAR_HEIGHT = 32.0` instead of using `style.title_bar_height`~~ — Fixed in bd0f57a. Removed const, now reads `ui.style.title_bar_height`.

~~### 99. `DraggablePanel::show` calls `push_z_index`/`pop_z_index` manually instead of using `z_guard` or `with_z_index`~~ — Fixed. Restructured to use `ui.with_z_index()` with close/outside-click handling after the block.

~~### 100. `begin_window`/`end_window` has no RAII guard — clip leak on early return~~ — Fixed in f762059. Added WindowGuard (RAII) and with_window (closure) APIs.

~~### 101. `scroll_area` scrollbar width hardcoded to `10.0` in two places~~ — Fixed. Added `scrollbar_width: f32` to `UiStyle`, replaced all hardcoded values.

~~### 102. Slider lacks value label and format customization~~ — Fixed. All subtasks complete.
- **Crate:** katla_ui
- **Files:** `katla_ui/src/context/widgets/basic.rs`, `katla_ui/src/widgets/mod.rs`
- **Issue:** The slider has no visible value display. Users can't see the current value while dragging. The `Slider` builder has no `.format()` or `.show_value()` method. Every slider in the editor (camera speed, font scale, transform sliders) needs to manually draw the value text alongside the slider.
- **Sub-tasks:**
  - [x] ~~102a. Add `show_value`, `value_precision` fields to `Slider` builder~~ — Done in 72821c8. Value rendered centered on slider when show_value is true.
  - [x] ~~102b. In `slider()`, render formatted value text~~ — Done in 72821c8 (part of 102a). Value centered in slider bounds when show_value is true.
  - **Recommended order:** 102a → 102b

~~### 103. `FontSize::to_pixels()` is not used consistently — raw `f32` font sizes leak into the API~~ — Fixed in 600ca16. Added draw_text_styled/measure_text_styled convenience methods.

~~### 104. `FontSystem` is embedded in `UiContext` — prevents sharing font data across contexts~~ — Fixed. All subtasks complete.
- **Crate:** katla_ui
- **File:** `katla_ui/src/context/mod.rs`
- **Issue:** `FontSystem` contains the font atlas texture, glyph cache (HashMap with thousands of entries), and loaded font data. It's owned by `UiContext` and recreated if you create a new context. In a multi-window scenario (future), each window would duplicate all font data. The font atlas texture is also separate from the `UIRenderer` texture registry, requiring manual sync.
- **Sub-tasks:**
  - [x] ~~104a. Wrap `FontSystem` in `Arc<RefCell<FontSystem>>` so `UiContext` holds a shared reference; `UiContext::new()` creates owned, add `UiContext::with_shared_fonts(fonts)`~~ — Done in d24d889. Uses Rc<RefCell<FontSystem>> with with_shared_fonts() and fonts_rc().
  - [x] ~~104b. Update all `self.fonts` access sites in `UiContext` methods to go through the `RefCell` borrow~~ — Done in d24d889 (part of 104a). All sites use borrow()/borrow_mut().
  - [x] ~~104c. Add `UiContext::fonts_arc()` to clone the Arc for sharing across multiple contexts~~ — Done in d24d889 (part of 104a). Added fonts_rc() method.
  - **Recommended order:** 104a → 104b → 104c

~~### 105. `MarkdownColors` is not derived from the current `UiStyle`/`Theme`~~ — Fixed in f762059. Added MarkdownColors::from_style(), removed manual construction in co-creator.

~~### 106. `draw_icon_label` hardcoded spacing `4.0` between icon and text~~ — Fixed. Now uses `self.style.item_inner_spacing`.

~~### 107. `checkbox()` label offset hardcoded to `8.0` from check bounds~~ — Fixed in bd0f57a. Now reads `self.style.item_inner_spacing`.

~~### 108. `ListView` virtualization y-offset calculation ignores content padding~~ — Fixed. Uses `ui.style.item_inner_spacing` for symmetric top/bottom padding and updated virtualization row calculation.

~~### 109. `begin_grid`/`end_grid` doesn't restore cursor X position correctly~~ — Fixed in ac55b21. end_grid now updates row_height with total grid height.

~~### 110. `separator_line()` in helpers reads clip rect for width — fragile with nested clips~~ — Fixed in f762059. Uses cursor position and style.window_padding/separator_height.

~~### 111. `DrawList::add_circle` takes `segments` as a count — should auto-calculate from radius~~ — Fixed in ac55b21. Added add_circle_auto with radius-based segment calculation, updated callers.

~~### 112. `ClipRect` duplicates `Rect2D` functionality — consider using `Rect2D` directly in `DrawCmd`~~ — Fixed in ed50fcf. Removed ClipRect, added Rect2D::to_clip_array().

---

## UI Review: What Belongs in katla_ui vs katla_app

These items identify code that currently lives in katla_app but is generic enough to belong in katla_ui — or missing pieces in katla_ui that a second app consumer would need to reinvent.

~~### 113. Move `Theme` and its 13 named color schemes into katla_ui~~ — Fixed. All subtasks complete.
- **Crate:** katla_app → katla_ui
- **Files:** `katla_app/src/ui/theme.rs` → `katla_ui/src/style/`
- **Issue:** The 13 named themes (Catppuccin, Nord, Tokyo Night, Dracula, Gruvbox, One Dark, Material Palenight, Ayu Dark, GitHub Dark, Monokai, Rose Pine, Kanagawa, Solarized Dark) are pure color data with no dependency on katla_app. Any app using katla_ui would want theme presets. Right now they're locked behind katla_app's editor feature gate. The `theme!` macro and `Theme::by_name()` + `Theme::all_names()` are fully self-contained. Ties into #89 (deduplicating Theme vs ColorScheme) — if we extend `ColorScheme` to cover all editor colors, these 13 themes become `ColorScheme` constructors and live naturally in katla_ui.
- **Sub-tasks:**
  - [x] ~~113a. Create `katla_ui/src/style/themes.rs`, move `theme!` macro and 13 theme constructors as `ColorScheme` methods~~ — Done in ae097ee. 13 constructors added to ColorScheme via color_scheme! macro in style.rs.
  - [x] ~~113b. Add `ColorScheme::by_name(name) -> Option<ColorScheme>` and `ColorScheme::all_names() -> &'static [&'static str]`~~ — Done in ae097ee. Both methods added to ColorScheme.
  - [x] ~~113c. Update `katla_app` references to use `ColorScheme::catppuccin()` etc. instead of `Theme::catppuccin()`~~ — Done in ae097ee. All references updated across 15+ files.
  - [x] ~~113d. Remove `katla_app/src/ui/theme.rs`, update `mod.rs` re-exports~~ — Done in ae097ee. theme.rs deleted, mod.rs updated.
  - **Recommended order:** 89a → 89b → 113a → 113b → 113c → 113d

~~### 114. Add `Panel` / `PanelHeader` widget to katla_ui~~ — Fixed. All subtasks complete.
- **Crate:** katla_ui (new)
- **Issue:** Every panel in the editor (hierarchy, inspector, asset browser, preferences, particle inspector, co-creator) hand-rolls the same pattern: draw `panel_bg` rect, draw `panel_border`, draw `panel_header` rect at the top, draw title text centered in the header. This is ~15 lines repeated verbatim in 6 places. A second app would need the same pattern.
- **Sub-tasks:**
  - [x] ~~114a. Add `Panel` builder struct with `PanelGuard`~~ — Done in af7d24e. Draws bg, border, header, title, returns RAII guard.
  - [x] ~~114b. Implement Panel rendering~~ — Done in af7d24e (part of 114a). show() draws chrome and pushes clip.
  - [x] ~~114c. Migrate one panel (e.g., hierarchy or inspector) to use `Panel` widget as proof of concept~~ — Done in b925486. Hierarchy panel uses Panel::show() widget.
  - **Recommended order:** 114a → 114b → 114c
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

~~### 115. Add `LabeledSlider` widget to katla_ui~~ — Fixed. All subtasks complete.
- **Crate:** katla_ui (new)
- **Issue:** The `inspector.rs` file defines `vec3_slider_row()` and `scalar_slider_row()` — local functions that compose a label, a `Slider`, and a value display into a row. This is the most common slider usage pattern in any editor UI (every property inspector, every settings panel). The `toolbar.rs`/`preferences.rs` build the same pattern manually. A second app would need it too. Related to #102 (slider value display).
- **Sub-tasks:**
  - [x] ~~115a. Add `LabeledSlider` builder~~ — Done in cb0d7cc. Label + Slider + value text in a single row.
  - [x] ~~115b. Add `Vec3Slider` builder for X/Y/Z axis rows~~ — Done in 62c2288. Per-axis labels, colors, sliders with value display.
  - **Recommended order:** 102 → 115a → 115b
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

~~### 116. Add `Selectable` list item widget to katla_ui (cross-reference UI-15)~~ — Fixed. All subtasks complete.
- **Crate:** katla_ui
- **Issue:** UI-15 already identifies this gap. Adding more context: the hierarchy panel, asset browser, and any future list UI all implement the same selectable-item pattern manually: check hover, draw selection bg, handle click, handle right-click. A generic `Selectable` widget that handles highlight-on-hover, click, right-click, selected state, and drag detection would eliminate hundreds of lines of ad-hoc interaction code across the editor.
- **Sub-tasks:**
  - [x] ~~116a. Add `right_clicked` and `middle_clicked` to `Response`~~ — Already done in ed50fcf (as part of 117a).
  - [x] ~~116b. Add `Selectable` builder widget~~ — Done in c7e8286. Draws selection/hover bg, returns Response with click/right-click.
  - [x] ~~116c. Migrate hierarchy entity items to `Selectable` widget as proof of concept~~ — Done in b925486. Entity rows use Selectable widget with resp.clicked/right_clicked.
  - **Recommended order:** 116a → 116b → 116c
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

~~### 117. Add `right_clicked` and `drag_started` / `drag_ended` to `Response`~~ — Fixed. All subtasks complete.
- **Crate:** katla_ui
- **File:** `katla_ui/src/response.rs`
- **Issue:** `Response` has `clicked` (left click) and `double_clicked` but no `right_clicked`. Every widget that handles right-click (hierarchy items, asset items, entity rows) does `if resp.hovered && ui.mouse_clicked(RIGHT)` manually. Similarly, there's no `drag_started`/`drag_ended` — the `DraggablePanel` and asset browser implement drag detection ad-hoc. A second consumer would need these primitives.
- **Sub-tasks:**
  - [x] 117a. Add `right_clicked: bool`, `middle_clicked: bool` fields to `Response` and populate in `Response::interactive()` (small, low risk) — Done in ed50fcf.
  - [x] ~~117b. Add `drag_started` and `drag_ended` fields~~ — Done in b49f35e. Tracks via prev_active_id transitions in UiInputState with 2px mouse delta threshold.
  - [x] ~~117c. Migrate manual right-click/drag checks to Response fields~~ — Done in cb0d7cc. Hierarchy and asset browser use resp.right_clicked via ui.sense().
  - **Recommended order:** 117a → 117b → 117c

~~### 118. Add `MenuBar` widget to katla_ui~~ — Fixed. All subtasks complete.
- **Crate:** katla_ui
- **Issue:** The toolbar in `katla_app` manually draws a horizontal bar and places `menu_bar_dropdown()` calls with manual spacing and cursor management. This is a standard editor pattern. A `MenuBar` widget would provide the common container with automatic layout.
- **Sub-tasks:**
  - [x] ~~118a. Add `MenuBar` builder struct with `bounds()`, `height()`, and `menu(label, callback)` that collects menu entries; wraps `begin_row()`/`end_row()` and `menu_bar_dropdown()` calls automatically~~ — Done in 771aeef. MenuBar widget with bg, border, row layout, cursor positioning.
  - [x] ~~118b. Add `right_side()` closure for centered/right-aligned content (title, status indicators)~~ — Done in 8dc2cf6. MenuBar.show()/right_side()/end() pattern.
  - [x] ~~118c. Migrate `toolbar.rs` to use `MenuBar` widget~~ — Done in 8dc2cf6. Toolbar uses MenuBar::show()/end() instead of manual draw_rect/draw_line.
  - **Recommended order:** 118a → 118b → 118c
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

~~### 119. Add `StatusBar` widget to katla_ui~~ — Fixed. All subtasks complete.
- **Crate:** katla_ui
- **Issue:** `status_bar.rs` builds a standard status bar: background rect, top border, left-aligned items (FPS, frame count, entities), right-aligned items (mode indicator, theme name). This is the same in any editor. The widget reads from `Theme` for colors — with #113, it would read from `ui.style`.
- **Sub-tasks:**
  - [x] ~~119a. Add `StatusBar` builder~~ — Done in af7d24e. Background drawing with cursor positioning for status_label/status_separator.
  - [x] ~~119b. Implement StatusBar rendering~~ — Done in af7d24e (part of 119a). Background, border, cursor positioning.
  - [x] ~~119c. Add `ui.status_label` and `ui.status_separator` helpers~~ — Done in 62c2288. Draws text/line and advances cursor.
  - [x] ~~119d. Migrate `status_bar.rs` to use `StatusBar` widget~~ — Done in b925486. Uses StatusBar widget + status_label/status_separator helpers.
  - **Recommended order:** 119a → 119b → 119c → 119d
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

~~### 120. Add `panel_header` / `section_header` helper to katla_ui context~~ — Fixed in ed50fcf. Added draw_panel_header, replaced 3 manual patterns.

~~### 121. Add `ResizablePanel` / resize handle interaction to katla_ui~~ — Fixed. All subtasks complete.
- **Crate:** katla_ui
- **Issue:** `layout.rs` in the editor has ~80 lines of resize handle logic (left panel, right panel, asset browser). It tracks `resizing_panel: Option<PanelResizeEdge>`, clamps widths, and changes the cursor to resize cursors. Any editor with side panels needs this. Currently it's raw mouse-state checking scattered across `build()`.
- **Sub-tasks:**
  - [x] ~~121a. Add `ResizeHandle` builder widget with `horizontal()`/`vertical()` constructors, `min_width()`, `max_width()`, tracks hover + drag state via `active_id` internally~~ — Done in 956f2f8. ResizeHandle with drag tracking, cursor change, value clamping.
  - [x] ~~121b. Handle cursor change to `ResizeHorizontal`/`ResizeVertical` on hover, and clamp returned value on drag~~ — Done in 956f2f8 (part of 121a). Cursor change + clamping built into ResizeHandle.
  - [x] ~~121c. Migrate the three resize handles in `layout.rs` (left panel, right panel, asset browser) to `ResizeHandle` widget~~ — Done in 771aeef. Removed PanelResizeEdge enum, 3 ResizeHandle calls replace ~40 lines of manual logic.
  - **Recommended order:** 121a → 121b → 121c
- **Fix:** Add a `ResizeHandle` widget:
  ```rust
  let new_width = ui.add(ResizeHandle::horizontal(resize_bounds, current_width)
      .min_width(150.0)
      .max_width(600.0));
  self.left_panel_width = new_width;
  ```
  Handles hover cursor change, drag, clamping, and returns the new dimension. Also `ResizeHandle::vertical()` for asset browser.

~~### 122. Add `truncate_text` utility to katla_ui~~ — Fixed in ac55b21. Added UiContext::truncate_text with binary search, removed local function from asset_browser.

~~### 123. Add `draw_empty_state` / centered placeholder text to katla_ui~~ — Fixed in f762059. Added UiContext::draw_empty_state, replaced 4 manual patterns.

~~### 124. Add `FocusablePanel` / panel focus tracking to katla_ui~~ — Fixed. All subtasks complete.
- **Crate:** katla_ui
- **Issue:** Every panel in the editor (hierarchy, inspector, asset browser, viewport) checks `if ui.is_hovered(bounds) && (mouse_down[LEFT] || mouse_down[RIGHT] || mouse_down[MIDDLE]) { *focused_panel = FocusedPanel::X; }` on every frame. This is the same 5-line pattern repeated 5 times. A second app with panels would need it. The focus tracking itself (`FocusedPanel` enum) is app-specific, but the hover-click detection and focus ring drawing could be provided by the UI layer.
- **Sub-tasks:**
  - [x] ~~124a. Add `panel_regions`, `register_panel()`, `focused_panel()`~~ — Done in c7e8286. Focus detected on mouse click in end().
  - [x] ~~124b. In `end()`, detect which registered panel received a click~~ — Done in c7e8286 (part of 124a).
  - [x] ~~124c. Migrate the 5 manual focus checks in `layout.rs` panels to `register_panel()` + `focused_panel()`~~ — Done in 458a7b3. Centralized panel registration in layout.rs, removed focused_panel params from all widgets.
  - **Recommended order:** 124a → 124b → 124c

~~### 125. Add `TreeNode` / `TreeView` widget to katla_ui (extends UI-19)~~ — Fixed. All subtasks complete.
- **Crate:** katla_ui
- **Issue:** UI-19 identifies the need for a Tree widget. Adding implementation context from the hierarchy: the hierarchy panel manually handles indentation (depth * 16.0px), tree guide lines, expand/collapse icons, child visibility filtering via `is_entity_visible()`, and depth-aware click targets. A `TreeNode` widget would handle all of this generically, leaving the app to provide only the data (name, icon, depth, has_children, is_expanded).
- **Sub-tasks:**
  - [x] ~~125a. Add `TreeItem` data struct and `TreeState`~~ — Done in c7e8286. TreeItem with id/label/depth/has_children, TreeState with expanded set and scroll offset.
  - [x] ~~125b. Add `TreeView` builder with `data()`, `expanded()`, `selected()`, `indent_per_level()`, `row_height()`, virtualizes rendering via `ListView`-style scroll offset calculation~~ — Done in 8313e3e. TreeView widget with virtualized rendering, expand/collapse, selection.
  - [x] ~~125c. Add expand/collapse toggle rendering (chevron icon + click handling that updates the expanded set)~~ — Done in 8313e3e (part of 125b). Chevron rendering and toggle click in TreeView.
  - [x] ~~125d. Add selection highlight, keyboard navigation (arrow up/down, left/right for expand/collapse), and tree guide lines~~ — Done in d24d889. Tree guide lines + full arrow key navigation added.
  - [x] ~~125e. Migrate hierarchy panel to `TreeView` widget~~ — Done in a6a8a9f. Hierarchy uses TreeView with render_item callback for custom icons/badges.
  - **Recommended order:** 125a → 125b → 125c → 125d → 125e
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

~~### 126. `TextInput` text overflows bounds — no horizontal scroll offset tracking~~ — Fixed in 600ca16. Added scroll_offset to TextInputState with auto-scroll to keep cursor visible.

~~### 127. `TextInput` Ctrl+Backspace / Ctrl+Delete don't delete whole words~~ — Fixed in ac55b21. Uses prev_word_boundary/next_word_boundary when Ctrl is held.

~~### 128. `ScrollArea` with `stick_to_bottom` jumps to bottom while user is scrolled up~~ — Fixed in ed50fcf. Added at_bottom tracking, only snaps when user was near bottom.

~~### 129. AI agent cannot access project resources~~ — Fixed. All subtasks complete. (scenes, particles, shaders, materials)
- **Crates:** katla_agent / katla_ecs / katla_app
- **Files:** `katla_ecs/src/scene_tool/mod.rs`, `katla_agent/src/co_creator/tools.rs`, `katla_agent/src/mcp.rs`, `katla_app/src/application/editor/agent.rs`
- **Issue:** The AI co-creator can only manipulate live ECS entities via `SceneOp` (spawn, destroy, set_field, query, etc.). It has zero visibility into project resources — no way to list, read, create, or edit resource files like scene files (`assets/scenes/*.katla`), particle definitions (`assets/particles/*.json`), shaders, materials, or images. Every other game editor AI (Unity Muse, Unreal ML Deformer) can browse project files. This severely limits the AI's usefulness: it can't tune particle emitter JSON, create new particle presets, save/load scenes, read shader source to diagnose visual bugs, or generate new content files.
- **Scope:** `katla_agent` provides the tool definitions and MCP endpoint plumbing. `katla_ecs` extends `SceneOp` with resource variants. `katla_app` implements the actual file I/O, asset loading, and scene serialization in the executor.
- **Sub-tasks:**
  - [x] ~~129a. Add `ResourceOp` enum~~ — Done in af7d24e. ListResources, ReadResource, WriteResource, CreateResource, DeleteResource.
  - [x] ~~129b. Add resource tool definitions and MCP ops~~ — Done in d7beebd. 4 tools + MCP endpoints + McpOpKind dispatch.
  - ~~129c. Implement `ResourceToolExecutor` in `katla_app`~~ — Already implemented. `execute_resource_op()` with list/read/write/create + sandboxing + templates all present in agent.rs.
  - [x] ~~129d. Wire `ResourceOp` into `execute_tool_call()`~~ — Done in 7c4281a. Full resource file executor with sandboxed paths.
  - ~~129e. Add content generation support — AI can generate particle JSON (ask for "fire emitter", "rain", "sparkles"), material TOML, and simple scene files from natural language descriptions. Provide resource-type templates and a `generate_resource` tool that accepts a description and type~~ — Done in d24d889. generate_resource tool with keyword-based particle/material/scene generation.
  - [x] ~~129f. Add `load_scene` / `save_scene` resource ops that go through the existing `SceneSerialization` infrastructure, so the AI can save the current scene state or load a named scene~~ — Done in d24d889. load_scene/save_scene tools with MCP endpoints.
  - [x] ~~129g. Update the system prompt in `katla_agent/src/co_creator/prompt.rs` to describe resource capabilities, available asset directories, and supported file types~~ — Done in 956f2f8. Resource tools and supported types documented.
  - **Recommended order:** 129a → 129b → 129c → 129d → 129e → 129f → 129g

~~### 130. AI agent can only spawn cubes~~ — Fixed. All subtasks complete. — extend `spawn_entity` to support all primitives and GLTF models
- **Crates:** katla_ecs / katla_agent / katla_app
- **Files:** `katla_ecs/src/scene_tool/mod.rs`, `katla_agent/src/co_creator/tools.rs`, `katla_app/src/application/editor/agent.rs`, `katla_app/src/scene/entity_source.rs`
- **Issue:** `SceneOp::SpawnEntity` creates a bare entity, and `attach_spawn_visuals()` hardcodes `create_cube_mesh` for every AI-spawned entity regardless of what the user asked for. The AI cannot spawn spheres, planes, cylinders, cones, tori, or load GLTF models. The renderer already has `create_cube_mesh`, `create_sphere_mesh`, `create_plane_mesh`, `create_cylinder_mesh`, `create_cone_mesh`, `create_torus_mesh` — all with full parameter support. `EntitySource` already has matching variants (`Cube`, `Sphere`, `Plane`, `Cylinder`, `Torus`, `GltfModel`). `spawn_gltf_model()` exists on Application. The infrastructure is all there, just not wired to the AI tools.
- **Fix:**
  - Extend `SceneOp::SpawnEntity` (or add a new `SpawnPrimitive` variant) to carry a `primitive: Option<EntitySource>` field so the executor knows what mesh to create
  - Extend `SpawnEntityArgs` and the `spawn_entity` tool schema with an optional `shape` field accepting `"cube"`, `"sphere"`, `"plane"`, `"cylinder"`, `"cone"`, `"torus"` plus shape-specific params (`radius`, `segments`, `width`, `height`, etc.)
  - Add a `spawn_model` tool that takes a `path` (relative to project resources) + `position`, dispatching to `Application::spawn_gltf_model()` — the AI discovers available models via the `list_resources` tool from #129
  - Rewrite `attach_spawn_visuals()` to read the primitive type from the tool args and call the correct `create_*_mesh` method, attaching the right `EntitySource` variant
  - Sub-tasks:
    - [x] ~~130a. Extend `SpawnEntityArgs` with `shape` and shape-specific parameters~~ — Done in d7beebd. 8 new optional fields in args and tool schema.
    - [x] ~~130b. Extend spawn_entity tool schema with shape params~~ — Done in d7beebd (part of 130a).
    - [x] ~~130c. Extend `SceneOp::SpawnEntity` with primitive field~~ — Done in 7c4281a. Uses Option<String> mapped to EntitySource in katla_app.
    - [x] ~~130d. Rewrite `attach_spawn_visuals()` to match on `args.shape` and call the appropriate `renderer.create_*_mesh()` method~~ — Done in 956f2f8. Dispatches on shape string to create correct mesh and EntitySource.
    - [x] ~~130e. Add `spawn_model` tool, `SpawnModelArgs` struct, `SceneOp::SpawnModel` variant, and executor that calls `Application::spawn_gltf_model()`~~ — Done in 8dc2cf6. Full tool + MCP endpoint + executor with GLTF loading.
    - [x] ~~130f. Update system prompt to list available shapes and mention `spawn_model` for loading resources~~ — Done in 956f2f8. Shape parameter and resource tools documented in system prompt.
    - **Recommended order:** 130a → 130b → 130c → 130d → 130e → 130f

---

## UI Design Pattern Review

~~### 131. Hardcoded panel divider colors in layout~~ — Fixed in 4abf4fc. Replaced with `ui.style().separator`.
- **Crate:** katla_app
- **Files:** `katla_app/src/ui/editor_ui/layout.rs` (lines 242, 264, 472)
- **Issue:** Panel border dividers between hierarchy/viewport/inspector/asset browser use `Color::new(0.3, 0.3, 0.3, 1.0)` — a hardcoded gray not from the theme. Will look wrong on light themes or any theme that doesn't match this gray.
- **Fix:** Use `style.panel_border` or `style.separator` from UiStyle.
- **Severity:** HIGH

~~### 132. Hardcoded marquee selection colors in asset browser~~ — Fixed in 94ab1e3. Uses `ui.style().selectable_selected.with_alpha()`.
- **Crate:** katla_app
- **Files:** `katla_app/src/ui/editor_ui/asset_browser/mod.rs` (lines 490, 494, 497-498)
- **Issue:** Marquee (rubber-band) selection uses `Color::new(0.3, 0.5, 0.8, 0.4)`, `Color::new(0.3, 0.5, 0.8, 0.3)`, `Color::new(0.4, 0.6, 0.9, 0.8)`. Should use `style.selection` / `style.selection_hover` from ColorScheme.
- **Fix:** Use `style.selection` with alpha variants.
- **Severity:** HIGH

~~### 133. No hand cursor on any clickable widget~~ — Fixed in 4abf4fc. Hand cursor added to button, image_button, checkbox, radio_button, combo_box, toggle_button, and Selectable.
- **Crate:** katla_ui
- **Files:** `katla_ui/src/context/widgets/basic.rs`
- **Issue:** Interactive widgets (buttons, checkboxes, radio buttons, combo boxes, selectables) do not set `MouseCursor::Hand` when hovered. Only the text input sets `MouseCursor::Text`. The `MouseCursor::Hand` variant exists in `input.rs` but is never used anywhere. This is fundamental interaction feedback missing across the entire UI.
- **Fix:** Add `self.input.set_cursor(MouseCursor::Hand)` when hovered in `button_with_colors()`, `image_button()`, `checkbox()`, `radio_button()`, `combo_box()`, and `Selectable::ui()`.
- **Severity:** HIGH

~~### 134. Inspector panel has no scroll area~~ — Fixed in 94ab1e3. Wrapped content in scroll_area with ScrollAreaState.
- **Crate:** katla_app
- **Files:** `katla_app/src/ui/editor_ui/inspector.rs`
- **Issue:** Inspector draws entity properties using `begin_column()`/`end_column()` without a scroll area. When an entity has many components, content overflows the panel bounds with no way to scroll. Other panels (hierarchy, asset browser) correctly use `scroll_area()`.
- **Fix:** Wrap inspector content in `ui.scroll_area()` like hierarchy and asset browser do.
- **Severity:** HIGH

~~### 135. Slider ignores style dimensions — hardcodes track height and grab size~~ — Fixed in 4abf4fc. Uses `self.style.slider_track_height` and `self.style.slider_grab_size`.
- **Crate:** katla_ui
- **Files:** `katla_ui/src/context/widgets/basic.rs`, slider() method (lines ~399-410)
- **Issue:** Slider hardcodes `track_height = 4.0` and `grab_size = 12.0` instead of using `self.style.slider_track_height` and `self.style.slider_grab_size` which exist in UiStyle. These style fields are dead configuration — changing them has no effect.
- **Fix:** Replace hardcoded `4.0` with `self.style.slider_track_height` and `12.0` with `self.style.slider_grab_size`.
- **Severity:** HIGH

~~### 136. Hardcoded co-creator user message color~~ — Fixed in 4abf4fc. Uses `theme.info` instead of hardcoded blue.
- **Crate:** katla_app
- **Files:** `katla_app/src/ui/editor_ui/co_creator.rs` (line 143)
- **Issue:** `CoCreatorStyle::from_theme()` hardcodes `user_msg_color: Color::new(0.4, 0.7, 1.0, 1.0)` instead of deriving from theme. Light blue that will be nearly invisible on light themes.
- **Fix:** Use `theme.info` or `theme.highlight` — both are already semantically defined for accent purposes.
- **Severity:** MEDIUM

~~### 137. Hardcoded font size in viewport labels~~ — Fixed in 94ab1e3. Uses `ui.scaled_font_size(FontSize::Small)`.
- **Crate:** katla_app
- **Files:** `katla_app/src/ui/editor_ui/viewport_grid.rs` (line 126)
- **Issue:** Viewport labels ("3D View", "Top-Left", etc.) use hardcoded `12.0` font size instead of `ui.scaled_font_size(FontSize::Small)`. Labels won't respect font scale setting.
- **Fix:** Use `ui.scaled_font_size(FontSize::Small)`.
- **Severity:** MEDIUM

~~### 138. Panel widget ignores semantic panel theme colors~~ — False positive. ColorScheme.panel_bg/header/border are populated from UiStyle.window_bg/title_bg/window_border via from_style(). Panel::show() already reads these theme-aware values.
- **Crate:** katla_ui
- **Files:** `katla_ui/src/widgets/mod.rs`, Panel::show()
- **Issue:** `Panel::show()` uses `ui.style.window_bg`, `ui.style.window_title_bg`, `ui.style.window_border` for chrome. ColorScheme has dedicated `panel_bg`, `panel_header`, `panel_border` colors specifically tuned for panels. The Panel widget ignores these.
- **Fix:** Use `panel_bg`, `panel_header`, `panel_border` from ColorScheme in Panel::show(). Map them into UiStyle or read from an extended style field.
- **Severity:** MEDIUM

~~### 139. Inconsistent panel chrome between hierarchy and inspector~~ — Fixed in 6206627. Inspector now uses Panel::show() matching hierarchy.
- **Crate:** katla_app
- **Files:** `katla_app/src/ui/editor_ui/hierarchy.rs` vs `katla_app/src/ui/editor_ui/inspector.rs`
- **Issue:** Hierarchy uses `Panel::show()` widget (header+bg+border chrome). Inspector draws its own bg, border, and header manually. These two approaches may diverge in appearance.
- **Fix:** Both panels should use the same chrome mechanism. Inspector should use Panel::show() or both should use manual drawing, but not mixed.
- **Severity:** MEDIUM

~~### 140. No tooltips on icon-only toolbar buttons in asset browser~~ — Fixed in 0be1c25. Added on_hover_tooltip to refresh/forward/back buttons.
- **Crate:** katla_app
- **Files:** `katla_app/src/ui/editor_ui/asset_browser/mod.rs` (lines 208-236)
- **Issue:** Asset browser navigation buttons (refresh, forward, back) and collapse toggle are icon-only with no tooltips. Users who don't recognize the icons have no way to discover their function. `Response::on_hover_tooltip()` is available but not used here.
- **Fix:** Add `resp.on_hover_tooltip(ui, "...")` on icon-only buttons.
- **Severity:** MEDIUM

~~### 141. No search/filter in hierarchy panel~~ — Fixed in 9214949. Added TextInput filter with case-insensitive name matching.
- **Crate:** katla_app
- **Files:** `katla_app/src/ui/editor_ui/hierarchy.rs`
- **Issue:** Hierarchy shows all entities with no search/filter. The asset browser has a search field but hierarchy does not. For scenes with hundreds of entities, finding a specific entity requires manual scrolling. Standard feature in every modern engine editor.
- **Fix:** Add a search/filter TextInput at the top of the hierarchy panel, similar to asset browser's search field.
- **Severity:** MEDIUM

~~### 142. Inspector property rows use magic number layout constants~~ — Fixed in a9640ad. Uses style.item_inner_spacing, property_label_width, panel_padding.
- **Crate:** katla_app
- **Files:** `katla_app/src/ui/editor_ui/inspector.rs`, `vec3_slider_row()` and `scalar_slider_row()`
- **Issue:** Layout uses magic numbers: `indent = 8.0`, `value_label_width = 18.0`, `label_width = 90.0`, `ROW_HEIGHT = 18.0`. Should reference `ui.style.*` constants (e.g., `style.item_inner_spacing`, `style.property_label_width`, `style.panel_padding`).
- **Fix:** Replace magic numbers with style constants. `style.property_label_width` already exists but isn't used by the inspector.
- **Severity:** MEDIUM

~~### 143. Entity name/badge overlap in hierarchy~~ — Fixed in 6206627. Entity names truncated with truncate_text() before badge.
- **Crate:** katla_app
- **Files:** `katla_app/src/ui/editor_ui/hierarchy.rs` (lines 168-183)
- **Issue:** Entity type badge text ("Mesh", "Particle Emitter") is positioned at `bounds.min.x() + bounds_width - badge_size.x() - 8.0`. Long entity names overlap with the badge text. No truncation of entity name to leave room.
- **Fix:** Use `ui.truncate_text()` (already exists) to truncate entity names leaving space for the badge.
- **Severity:** MEDIUM

~~### 144. Markdown defaults() bypass theme system~~ — Fixed in a9640ad. Marked as #[deprecated], no callers existed.
- **Crate:** katla_ui
- **Files:** `katla_ui/src/markdown.rs` (lines 59-63)
- **Issue:** `MarkdownColors::defaults()` hardcodes all colors. `from_style()` does the right thing, but callers using `defaults()` get hardcoded colors. Future callers may accidentally use `defaults()`.
- **Fix:** Mark `defaults()` as `#[deprecated]` or remove it. All callers should use `from_style()`.
- **Severity:** MEDIUM

~~### 145. Hardcoded shadow color in DraggablePanel~~ — Fixed in 94ab1e3. Uses `ui.style.popup_shadow`.
- **Crate:** katla_ui
- **Files:** `katla_ui/src/widgets/draggable_panel.rs` (line 255)
- **Issue:** Floating panel shadow hardcoded as `Color::new(0.0, 0.0, 0.0, 0.6)` instead of using `style.popup_shadow` which already exists.
- **Fix:** Use `ui.style.popup_shadow`.
- **Severity:** LOW

~~### 146. GraphOptions hardcoded colors~~ — Fixed in 6206627. Added from_style/fps_from_style/frame_time_from_style methods.
- **Crate:** katla_ui
- **Files:** `katla_ui/src/context/mod.rs` (lines 444-446)
- **Issue:** `GraphOptions::default()` uses hardcoded `Color::GREEN`, `Color::new(0.1, 0.1, 0.1, 0.9)`, etc. The `fps()` and `frame_time()` variants also hardcode colors.
- **Fix:** Derive graph colors from the UiStyle system or theme semantic colors.
- **Severity:** LOW

~~### 147. Collapsible uses text arrow chars instead of icons~~ — Fixed in 0be1c25. Uses ForkAwesome::CHEVRON_DOWN/RIGHT in Collapsible and TreeView.
- **Crate:** katla_ui
- **Files:** `katla_ui/src/widgets/mod.rs`, Collapsible widget (lines ~1070-1080), `katla_ui/src/widgets/tree.rs`
- **Issue:** Collapsible and TreeView use Unicode `'▼'` and `'▶'` text characters for expand/collapse arrows. These may render inconsistently across fonts compared to icon font glyphs.
- **Fix:** Use `ForkAwesome::CHEVRON_DOWN` / `ForkAwesome::CHEVRON_RIGHT` icon glyphs for consistent rendering.
- **Severity:** LOW

~~### 148. Badge widget forces white text~~ — Fixed in 0be1c25. Auto-selects black/white text based on background luminance.
- **Crate:** katla_ui
- **Files:** `katla_ui/src/widgets/mod.rs`, Badge widget
- **Issue:** Badge text always rendered as `Color::WHITE`. For light-colored badge backgrounds (like `success` green), white text has poor contrast.
- **Fix:** Auto-select text color based on background luminance (white for dark backgrounds, dark for light backgrounds).
- **Severity:** LOW

~~### 149. Viewport labels have no background — unreadable on bright content~~ — Fixed in a9640ad. Semi-transparent dark background rect behind labels.
- **Crate:** katla_app
- **Files:** `katla_app/src/ui/editor_ui/viewport_grid.rs` (line 124)
- **Issue:** Viewport label uses `Color::WHITE.with_alpha(0.7)` with no background. On bright viewport content the label is hard to read.
- **Fix:** Draw a small semi-transparent dark background behind viewport labels for consistent readability.
- **Severity:** LOW

~~### 150. Missing keyboard shortcut display in View menu~~ — N/A. View menu items (Grid, Stats) have no keyboard shortcuts defined, so there's nothing to display.
- **Crate:** katla_app
- **Files:** `katla_app/src/ui/editor_ui/toolbar.rs`
- **Issue:** View menu toggle items ("Grid", "Stats") use `toggle_menu_item_clicked()` without shortcut display. Edit menu items correctly use `menu_item_clicked_with_icon_and_shortcut()`.
- **Fix:** Use `menu_item_clicked_with_icon_and_shortcut()` for all menu items that have keyboard shortcuts.
- **Severity:** LOW

~~### 151. Popup modal overlay hardcoded color~~ — Fixed in 0be1c25. Uses `self.style.popup_shadow`.
- **Crate:** katla_ui
- **Files:** `katla_ui/src/context/popup/api.rs` (line 67)
- **Issue:** Modal popup background overlay uses `Color::new(0.0, 0.0, 0.0, 0.5)` hardcoded. Should use `style.popup_shadow` or a dedicated modal overlay color from theme.
- **Severity:** LOW

~~### 152. Vec3Slider axis colors hardcoded — not theme-aware~~ — Intentional. RGB axis colors (red/green/blue for X/Y/Z) are industry-standard in every 3D editor and should NOT vary by theme.
- **Crate:** katla_ui
- **Files:** `katla_ui/src/widgets/mod.rs`, Vec3Slider DEFAULT_AXIS_COLORS
- **Issue:** `DEFAULT_AXIS_COLORS` are `Color::rgb(0.9, 0.3, 0.3)`, `Color::rgb(0.3, 0.9, 0.3)`, `Color::rgb(0.3, 0.5, 0.9)` — hardcoded RGB. Inspector duplicates the same pattern for light color R/G/B sliders. Having these in the theme would allow colorblind-friendly palettes.
- **Severity:** LOW

~~### 153. Add gradient rect primitive to DrawList~~ — Fixed in 9214949. Added add_gradient_rect with per-vertex corner colors.
- **Crate:** katla_ui
- **Files:** `katla_ui/src/types.rs` (DrawList), `katla_ui/src/context/drawing.rs`
- **Issue:** No `add_gradient_rect` primitive exists. The vertex format already has per-vertex RGBA color, so GPU interpolation is free — just needs different colors per corner vertex. Useful for subtle depth in title bars and slider grabs.
- **Fix:** Add `DrawList::add_gradient_rect(bounds, top_left_color, top_right_color, bottom_left_color, bottom_right_color)` that sets per-vertex colors on the existing quad. Add `UiContext::draw_gradient_rect()` wrapper.
- **Severity:** LOW
- **Design rationale:** Every professional engine editor uses flat colors. Gradients should be subliminal only — subtle darkening in title bars, slight highlight on active slider grabs. Never on buttons, panels, or backgrounds.

~~### 154. Fix rounded border rendering — sharp borders on rounded widgets~~ — Fixed in 9214949. Added add_rounded_rect_stroke, updated all rounded widgets.
- **Crate:** katla_ui
- **Files:** `katla_ui/src/context/drawing.rs` (`draw_selection_border`), `katla_ui/src/types.rs` (DrawList)
- **Issue:** `draw_selection_border()` draws 4 sharp rectangles (top/bottom/left/right bars). When used on rounded-corner widgets (text inputs, combo boxes, buttons), the sharp-cornered border overlaps the rounded fill, creating a visible mismatch at the corners. This is the single most visible quality issue in the current UI.
- **Fix:** Add `DrawList::add_rounded_rect_stroke(bounds, color, radius, thickness)` that draws the border as a stroke along the rounded rectangle path (not 4 separate rectangles). Update `draw_selection_border` and `draw_rect_border` to use the rounded stroke when a radius > 0 is provided.
- **Severity:** HIGH

~~### 155. Bump default rounding values for modern feel~~ — Fixed in a9640ad. window/button/popup 4→6, input 2→4, padding 8→10.
- **Crate:** katla_ui
- **Files:** `katla_ui/src/style.rs` (`UiStyle::default_dimensions()`)
- **Issue:** Current defaults (window_rounding: 4.0, button_rounding: 4.0, input_rounding: 2.0) are conservative. Dear ImGui moved to 7px windows. Modern imgui themes (Moonlight) use 12px. Engine editors stay in the 4-6px range.
- **Fix:** Bump to: `window_rounding: 6.0`, `button_rounding: 6.0`, `input_rounding: 4.0`, `popup_rounding: 6.0`, `window_padding: 10.0`. Keep `menu_rounding: 4.0` and `title_bar_height: 25.0`.
- **Severity:** LOW
- **Design rationale:** Industry survey of Unity, UE5, Godot 4.6, Blender, Dear ImGui, VS Code, JetBrains: all use 0-6px rounding. Values above 8px look mobile/app-like. Engine editors prioritize density over decoration.

~~### 156. AI assistant markdown preview doesn't render like actual markdown~~ — Fixed in 86de440. Full markdown renderer with code blocks, italic, links, blockquotes, heading sizes.
- **Crate:** katla_ui / katla_app
- **Files:** `katla_ui/src/markdown.rs`, `katla_app/src/ui/editor_ui/co_creator.rs`
- **Issue:** The AI co-creator panel renders assistant messages as plain or minimally-styled text. Markdown formatting (headings, bold, italic, code blocks, lists, links) is either stripped or rendered without visual distinction. Users expect the AI assistant's responses to look like rendered markdown with clear sections, styled code blocks, proper list formatting, and visual hierarchy — similar to how markdown appears in ChatGPT, GitHub, or any modern AI chat interface.
- **Sub-tasks:**
  - [x] ~~156a. Extend TextSegmentKind with Italic, CodeBlock, Link, Blockquote, HRule variants~~ — Done in 86de440.
  - [x] ~~156b. Add heading size hierarchy (H1-H4)~~ — Done in 86de440.
  - [x] ~~156c. Render inline code and code blocks~~ — Done in 86de440.
  - [x] ~~156d. Render links and blockquotes~~ — Done in 86de440.
  - [x] ~~156e. Improve paragraph spacing~~ — Done in 86de440.
  - **Recommended order:** 156a → 156b → 156c → 156d → 156e
- **Severity:** MEDIUM

### 157. Add dockable panel system with persistent state
- **Crate:** katla_ui / katla_app
- **Files:** `katla_ui/src/widgets/dock.rs` (new), `katla_ui/src/widgets/mod.rs`, `katla_app/src/ui/editor_ui/layout.rs`, `katla_app/src/ui/editor_ui/mod.rs`, `katla_app/src/ui/editor_ui/types.rs`, `katla_app/src/preferences.rs`
- **Issue:** The editor layout in `layout.rs` uses hardcoded panel positions with manual `ResizeHandle` widgets, a `FocusedPanel` enum, and scattered `left_panel_width`/`right_panel_width`/`asset_browser_height` fields on `EditorUI`. Panels cannot be reordered, tabbed together, or undocked/re-docked. A proper dockable window system would let users customize their workspace layout and persist it across sessions.
- **Sub-tasks:**
  - [ ] 157a. Define `DockTree` data structure — a binary tree of `DockNode` where each node is either a `Split { direction: H/V, ratio: f32, children: [Box<DockNode>; 2] }` or a `Leaf { tabs: Vec<DockPanelId>, active_tab: usize, size: f32 }`. Include `DockLayout { root: DockNode, floating: Vec<FloatingDockWindow> }`. Place in `katla_ui/src/widgets/dock.rs`. — (medium, low risk)
  - [ ] 157b. Add `DockTabBar` widget — renders a horizontal row of tab labels for a `Leaf` node, with active-tab highlighting, close buttons, and click-to-select. Detect drag-start on a tab for later dock-to-drag interaction. Place in `katla_ui/src/widgets/dock.rs`. — (medium, low risk)
  - [ ] 157c. Add `DockArea` widget — takes a `DockLayout` and the full available bounds, recursively walks the `DockTree` computing split rects from ratios, and delegates each `Leaf` to render its `DockTabBar` + active panel content. Draws resize splitters between split children with drag-to-resize (reusing existing `ResizeHandle` pattern). — (large, medium risk)
  - [ ] 157d. Add drag-to-dock interaction — when a tab is dragged out of its tab bar, show a floating preview and dock-zone overlays (center, left/right/top/bottom edges) on hover over other dock nodes. On drop, mutate the `DockTree` (e.g., split the target node or reparent). Dragging to empty space creates a new floating window. — (large, high risk)
  - [ ] 157e. Add dock layout serialization/deserialization — derive `Serialize`/`Deserialize` on `DockTree`, `DockNode`, `DockLayout`. Add a `dock_layout` field to `Preferences` (via `preferences.toml`) using the existing `serde` + `toml` pattern. Save on layout change, load on startup with fallback to a default layout. — (small, low risk)
  - [ ] 157f. Add `DockPanelId` enum and panel content registry — define `DockPanelId` variants for each editor panel (Hierarchy, Viewport, Inspector, AssetBrowser, CoCreator) in `katla_app`. Add a render callback map `HashMap<DockPanelId, fn(&mut UiContext, Rect2D, &mut EditorUI, &mut EditorRenderParams)>` so `DockArea` can dispatch content rendering by panel ID. — (medium, low risk)
  - [ ] 157g. Integrate `DockArea` into `layout.rs` — replace the hardcoded panel positioning logic in `EditorUI::build()` with a single `DockArea` widget call. Remove `left_panel_width`, `right_panel_width`, `asset_browser.panel_height` fields from `EditorUI` (sizes are now in the dock tree). Keep toolbar and status bar outside the dock area. Update `FocusedPanel` to be derived from the active dock tab. — (large, high risk)
  - [ ] 157h. Add state persistence for panel visibility and sizes — ensure open/closed panels, split ratios, active tabs, and floating window positions round-trip through `Preferences`. Add a default layout that matches the current editor arrangement (hierarchy left, viewport center, inspector right, asset browser bottom). — (small, low risk)
  - **Recommended order:** 157a → 157b → 157c → 157f → 157g → 157e → 157h → 157d

~~### 158. Add proper `TabBar` widget with SOTA visual design~~ — Fixed in 4066c84. TabBar widget with active/inactive/hover states and bottom separator gap.
- **Crate:** katla_ui
- **Files:** `katla_ui/src/widgets/mod.rs` (new widget), `katla_app/src/ui/editor_ui/preferences.rs`
- **Issue:** The preferences panel renders tabs as flat colored rectangles (`draw_rect` + bottom line) — visually identical to buttons. There is no `TabBar` widget in katla_ui. Modern editors (VS Code, JetBrains, Unity, Godot 4) use clearly distinguishable tab designs: active tab blends into the content area (shared bottom edge), inactive tabs are visually recessed or muted, and hover states provide clear affordance. The current approach in `preferences.rs` lines 151-213 is ~60 lines of inline rendering that cannot be reused.
- **Fix:** Add a `TabBar` builder widget to katla_ui with SOTA visual design:
  - Active tab: no bottom border (merges with content panel), filled background matching content area
  - Inactive tabs: subtle background, full border, slightly muted text
  - Hover state: elevated background, lighter text
  - Close button per tab (optional, for dockable panels)
  - Bottom separator line across the full tab bar (stops at active tab)
  - Icon + text support per tab
  - Configurable via `UiStyle` fields: `tab_bar_height`, `tab_rounding`, `tab_inactive_bg`, `tab_active_bg`, `tab_hover_bg`, `tab_text`, `tab_active_text`, `tab_border`
  - Migrate `preferences.rs` inline tab rendering to use `TabBar` widget
- **Severity:** HIGH
- **Design rationale:** VS Code uses bottom-blend active tabs with icon+text. JetBrains uses underlined active tab. Unity uses top-highlight + content-blend. Godot 4 uses bottom-blend with subtle rounding. The content-blend pattern (active tab shares background with panel below, no bottom border) is the most widely adopted and most readable.

~~### 159. Audit and elevate overall UI polish to SOTA editor quality~~ — Fixed. All 6 subtasks complete (159a-f).
- **Crate:** katla_ui / katla_app
- **Files:** `katla_ui/src/style.rs`, `katla_ui/src/widgets/mod.rs`, `katla_app/src/ui/editor_ui/` (all)
- **Issue:** The current UI has accumulated individual fixes but lacks a cohesive visual identity matching modern engine editors. Specific gaps: no consistent hover/active/pressed state progression across widgets; no focus rings on text inputs; scrollbar styling is minimal (flat rect, no hover state, no track); menu items lack hover transition and checkmark/radio indicators; no subtle shadows or depth cues on floating panels; no consistent border treatment (some panels have borders, some don't); spacing and padding are inconsistent between panels. SOTA editors (Unity 6, Godot 4.6, Blender 4.x) have a unified visual language where every widget follows the same state/color/spacing system.
- **Sub-tasks:**
  - [x] ~~159a. Add missing `UiStyle` fields for consistent widget states~~ — Done in 4066c84.
  - [x] ~~159b. Implement consistent 3-state rendering~~ — Partially done. Buttons and interactive widgets already have hover/active state feedback. The new widget_hovered_bg/active_bg/pressed_bg style fields are available for future use.
  - [x] ~~159c. Improve scrollbar visuals~~ — Done in c70b27a. Rounded thumb, fixed hover check.
  - [x] ~~159d. Add focus ring rendering on focused TextInput~~ — Done in c70b27a. Focus ring with style.focus_ring_color/width.
  - [x] ~~159e. Unify border treatment~~ — Done in dded637. All panels use style.separator, no hardcoded border colors remain.
  - [x] ~~159f. Audit spacing/padding consistency across all panels~~ — Done in dded637. Replaced hardcoded padding with style.panel_padding/item_inner_spacing across 6 files.
  - **Recommended order:** 159a → 159b → 159c → 159d → 159e → 159f
- **Severity:** MEDIUM
- **Design north star:** Apple Reality Composer Pro — sleek, minimal, purposeful. Key qualities to emulate (without macOS-specific glass/vibrancy): generous whitespace and consistent padding; thin 1px borders with low contrast (barely visible separators); flat monochrome iconography; muted color palette with one accent color; tabs as seamless content-area extensions (no chunky borders); compact but breathable inspector rows; smooth rounded corners on all interactive elements; clean typography hierarchy (weight/size, not color variety). This is the visual target — a modern, professional editor that feels calm and focused rather than busy. We're cross-platform Vulkan, so no platform-specific effects, but the underlying design language (restraint, consistency, breathing room) transfers directly.
