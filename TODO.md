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

### 18. `i64`/`u64` types get `FieldKind::Int` but no typed `FieldMut` accessor
- **Crate:** katla_derive / katla_ecs
- **File:** `katla_derive/src/lib.rs` (`field_mut_arm()`)
- **Issue:** `infer_field_kind()` maps `i64`/`u64` to `FieldKind::Int`, but `field_mut_arm()` has no match arms for them. They fall to `Unknown` — the inspector can't edit them.
- **Fix:** Either add `I64`/`U64` variants to `FieldMut` + derive, or remove `i64`/`u64` from `infer_field_kind()`.

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
- **Issue:** Destructive operations (delete entity, transform changes) have no undo. Slider drgs push `EditorAction::UpdateTransform` every frame (~120 identical actions during a 2s drag at 60fps), making a simple action stack impractical.
- **Fix:** Implement a command pattern with undo support. First fix the duplicate slider action issue (only push final value on mouse release).

### 22. No texture/mesh/material destroy API on `VulkanRenderer`
- **Crate:** katla_gfx
- **File:** `src/renderer/mod.rs`
- **Issue:** Resources can be created but never freed. Dynamic scenes accumulate GPU memory. Bindless texture slots (capped at 4096) will eventually exhaust.
- **Fix:** Add `destroy_texture(TextureHandle)`, `destroy_mesh(MeshHandle)` that unregister from bindless and free GPU resources.

### 23. Particle emitter GPU resources not cleaned up on entity destruction
- **Crate:** katla_app
- **File:** `src/systems/particle_system.rs` (line 110)
- **Issue:** When an entity with `ParticleEmitterComponent` is destroyed, the GPU emitter is not cleaned up. `EditorAction::DeleteEntity` handles it manually, but programmatic entity destruction leaks GPU emitters.
- **Fix:** In `ParticleSystem::update()`, iterate active emitter handles and check if owning entity still exists. Or add entity destruction hooks to ECS.

### 24. No combo box / dropdown select widget
- **Crate:** katla_ui
- **Issue:** `UiStyle` has `combo_bg`, `combo_border`, `combo_hovered`, `combo_text` fields but no combo box widget exists. Commonly needed for settings panels and inspectors.
- **Fix:** Implement a `ComboBox` builder widget.

### 25. No `clear_history()` or token budget on `CoCreatorAgent`
- **Crate:** katla_agent
- **File:** `src/co_creator/mod.rs`
- **Issue:** History grows unbounded. No truncation, no token counting, no pruning of old messages. Long sessions will hit context window limits or cost excessive tokens.
- **Fix:** Add `clear_history()`, `truncate_history(max_messages)`, and a token budget system.

### 26. No timeout on LLM requests
- **Crate:** katla_agent
- **Files:** `src/runtime.rs`, `src/llm/openai.rs`
- **Issue:** `LlmError::Timeout` exists but is never used. If the LLM provider hangs, the pending request never completes.
- **Fix:** Wrap async calls with `tokio::time::timeout()`.

### 27. No parent-child entity hierarchy in ECS
- **Crate:** katla_ecs
- **File:** `src/scene_tool/mod.rs`
- **Issue:** `SceneOp::GetSceneHierarchy` exists but returns all entities flat. No `Parent(EntityId)` / `Children(Vec<EntityId>)` components, no hierarchy traversal.
- **Fix:** Add `Parent`/`Children` components with automatic maintenance and hierarchy traversal.

### 28. No query filtering (`Without<T>`, `With<T>`)
- **Crate:** katla_ecs
- **File:** `src/query/mod.rs`
- **Issue:** Queries only support positive component inclusion. No way to query for entities with A but NOT B.
- **Fix:** Add `Without<T>` and `With<T>` filter types.

### 29. No `#[inspect(enum)]` / `#[inspect(struct)]` / `#[inspect(vec)]` attribute support
- **Crate:** katla_derive
- **Issue:** `FieldKind` has variants for `Struct`, `Enum`, `Vec`, `EntityRef` but the derive macro has no attributes to annotate fields with these kinds. They silently become `FieldKind::Unknown`.
- **Fix:** Add `#[inspect(enum)]`, `#[inspect(struct)]`, `#[inspect(vec)]`, `#[inspect(entity_ref)]` attribute support.

### 30. Text kerning not implemented (returns 0.0)
- **Crate:** katla_ui
- **File:** `src/text/measurement.rs` (line 33)
- **Issue:** `get_kerning()` always returns 0.0 with a TODO comment. Character pairs like "AV", "To" have incorrect spacing.
- **Fix:** Implement GPOS kerning via skrifa's API.

### 31. Selection clearing via Escape key
- **Crate:** katla_app
- **Issue:** No keyboard shortcut to deselect the current entity selection. Users must click empty space.
- **Fix:** Bind Escape to clear selection when viewport is focused and an entity is selected.

### 32. `pace` keybinding / camera speed control
- **Crate:** katla_app
- **Issue:** No adjustable camera movement speed. Users cannot control the pace of camera navigation.
- **Fix:** Add scroll-wheel or modifier key camera speed adjustment (e.g., Shift = fast, Ctrl = slow).

---

## P3: Improvements

~~### 33. Add `#[inline]` to hot-path methods across math crate~~ — Fixed in 6a186a1. Added #[inline] to Mat3/Mat4/Transform/Quat hot-path methods.

~~### 34. Add `#[inline]` to hot-path ECS methods~~ — Fixed in 93644a9. Added #[inline] to storage, entity_allocator, sparse_set methods.

~~### 35. Add `#[inline]` to hot-path UI text methods~~ — Fixed in 8a5e7a0. Added #[inline] to measure_text, font_ascent, line_height.

~~### 36. Replace `unwrap()` with `expect()` in production UI code~~ — Fixed in 8a5e7a0. Replaced 5 unwrap() calls with expect() across basic.rs, scroll_area.rs, glyph_pool.rs.

~~### 37. Remove deprecated `button_height_*` style fields~~ — Removed in dcc28d7. Deleted button_height_small, button_height_medium, toolbar_height from UiStyle and updated all references.

~~### 38. `Mat4::create_ortho` parameter order doesn't match standard convention~~ — Fixed in f74b05c. Reordered to (left, right, bottom, top, near, far) matching GLM/Vulkan convention.

### 39. `Mat4::create_proj` semantics unclear (infinite reverse-Z, no `far` parameter)
- **Crate:** katla_math
- **File:** `src/mat4.rs`
- **Issue:** Function name `create_proj` doesn't communicate that it's infinite reverse-Z. Callers expecting a standard perspective projection get incorrect results.
- **Fix:** Rename to `create_proj_reverse_z` or add a `far` parameter variant.

### 40. `Quat::inverse()` is actually `conjugate()` — misleading for non-unit quaternions
- **Crate:** katla_math
- **Files:** `src/sse/quat.rs`, `src/scalar/quat.rs`
- **Issue:** `inverse()` returns `conjugate()` which is only correct for unit quaternions.
- **Fix:** Rename to `inverse_unit()` or divide by `length_squared()`. Document the precondition.

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

### 45. Unused transfer queue infrastructure
- **Crate:** katla_gfx
- **File:** `src/vulkan/context/mod.rs`
- **Issue:** A `transfer_queue` and `transfer_command_pool` are created but never used. All transfers go through the graphics queue.
- **Fix:** Either use the transfer queue for staging copies, or remove the dead infrastructure.

~~### 46. `OutlineSubsystem::destroy` doesn't zero out pipeline handles~~ — Fixed in 6a186a1. All 8 pipeline handles zeroed in destroy().

### 47. `TextureManager::from_vulkan_resources()` is a stub returning handle 0
- **Crate:** katla_gfx
- **File:** `src/texture/manager.rs` (line ~183)
- **Issue:** Always returns `TextureHandle::new(0)` (default white texture). Transient frame graph textures can't be wrapped for UI rendering.
- **Fix:** Implement properly or remove the stub.

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

### 54. `apply_inspector_slider_changes` pushes duplicate EditorActions every frame during drag
- **Crate:** katla_app
- **File:** `src/application/editor/mod.rs` (lines 159-244)
- **Issue:** ~120 identical `EditorAction::UpdateTransform` actions during a 2-second slider drag at 60fps. All are processed redundantly.
- **Fix:** Only push action on final value change (mouse release), or mark intermediates as "preview".

~~### 55. `destroyed_entities.contains()` is O(n) per component removal event~~ — Fixed in 6a186a1. Changed Vec to HashSet for O(1) lookups.

### 56. `create_variants` ignores component/field/values parameters
- **Crate:** katla_agent
- **File:** `src/tools/tuning.rs` (lines 33-46)
- **Issue:** Only uses `values.len()` for iteration count. Never actually sets different variant values on duplicates.
- **Fix:** Return a two-phase operation plan (duplicate, then set field once entity IDs are known).

### 57. `scatter()` with `min_spacing` returns fewer entities than requested without warning
- **Crate:** katla_agent
- **File:** `src/tools/placement.rs` (lines 19-56)
- **Issue:** Entities too close are silently skipped. The grid is sized for `count` but positions are dropped.
- **Fix:** Return actual count placed, or retry with adjusted jitter.

~~### 58. `place_cluster()` radius distribution uses wrong formula~~ — Fixed in 93644a9. Changed cbrt(sqrt(t)) to cbrt(t) for correct t^(1/3).

### 59. `McpOp::QueryEntities` loses `name_filter`, `position`, `radius` fields
- **Crate:** katla_agent
- **File:** `src/mcp.rs` (lines 67-73)
- **Issue:** Conversion hardcodes `name_filter: None, position: None, radius: None`. MCP clients can't do spatial or name-based queries.
- **Fix:** Extend `McpOp::QueryEntities` and MCP tool schema.

~~### 60. `available_templates()` doesn't include `forest_clearing`~~ — False positive. Already present with test coverage.
- **Crate:** katla_agent
- **File:** `src/tools/templates.rs` (lines 58-63)
- **Fix:** Add `("forest_clearing", "Ring of trees around a clearing")` to the list.

### 61. AI LLM should not be able to delete the editor camera
- **Crate:** katla_agent
- **Issue:** The AI can issue `SceneOp::DeleteEntity` targeting the editor camera entity, breaking the viewport.
- **Fix:** Filter out protected entities (editor camera, editor gizmo) from delete/spawn-modify operations in `SceneToolExecutor`.

### 62. AI-spawned cubes don't appear in the 3D scene or hierarchy view
- **Crate:** katla_agent / katla_app
- **Issue:** When the AI tries spawning cubes via `SceneOp::SpawnEntity`, the entities are created but never render in the 3D viewport or show in the hierarchy panel. Likely missing component registration, transform initialization, or the entities aren't being added to the render/world correctly.
- **Fix:** Debug the spawn path end-to-end: verify the entity gets all required components (Transform, Mesh, Material), is registered in the render world, and appears in hierarchy queries.

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

~~### 67. `Sphere::create_from_verts` allocates unnecessarily~~ — Fixed in f74b05c. Rewritten to compute bounds inline in a single pass without Vec allocation.

### 68. `SparseSet` memory waste with large EntityId gaps
- **Crate:** katla_ecs
- **File:** `src/sparse_set.rs`
- **Issue:** Sparse array indexed by entity index. After creating/destroying many entities, the vec grows very large with mostly `None` entries.
- **Fix:** Use paged/chunked sparse array or `HashMap` fallback for large indices.

### 69. `query_changed` allocates `HashSet<EntityId>` every call
- **Crate:** katla_ecs
- **File:** `src/world.rs` (line ~251)
- **Fix:** Accept pre-allocated set as out-parameter, or store reusable buffer in `World`.

~~### 70. `ComponentRegistry::get` is O(n) linear scan by string comparison~~ — Fixed in dcc28d7. Changed from Vec to HashMap for O(1) lookups.

### 71. Redundant debug particle readback in frame_loop
- **Crate:** katla_app
- **File:** `src/application/frame_loop.rs` (lines 136-175)
- **Issue:** Large `#[cfg(debug_assertions)]` block at frame 10 reads debug data directly, but `renderer.rs` (lines 317-340) also sets up a frame graph readback. The direct read may race with frame graph execution.
- **Fix:** Remove the direct `read_debug_data()` call, rely on frame graph mechanism.

### 72. `InputMapper` binds `KeyF` to both `Interact` and focus-camera
- **Crate:** katla_app
- **Files:** `src/input/map.rs` (lines 51, 88), `src/application/gizmo.rs` (line 258)
- **Issue:** Pressing F both triggers `Action::Interact` and focuses camera. `Interact` is a game action that shouldn't fire in the editor viewport.
- **Fix:** Gate `Action::Interact` on a game-play mode flag, or unmap when editor is active.

---

## P4: Nice-to-Have / Future

### 73. Add unit tests for katla_icons (verify ForkAwesome codepoints)
- **Crate:** katla_icons
- **Issue:** Zero tests. Basic smoke tests would catch the 4 wrong codepoints.
- **Fix:** Add tests verifying all codepoints are in valid ForkAwesome PUA range (F000-F3FF), no duplicates, `common_icons()` non-empty.

~~### 74. Add missing angle icons (ANGLE_LEFT, ANGLE_UP, ANGLE_DOWN)~~ — Fixed in b7089dd. Added ANGLE_LEFT, ANGLE_UP, ANGLE_DOWN to ForkAwesome.
- **Crate:** katla_icons
- **Issue:** `ANGLE_RIGHT` exists but directional counterparts are missing. Used for submenu indicators.
- **Fix:** Add `ANGLE_LEFT` (F104), `ANGLE_UP` (F106), `ANGLE_DOWN` (F107).

### 75. Add streaming tests for `CoCreatorAgent` and `AsyncBridge`
- **Crate:** katla_agent
- **Issue:** The core streaming loop and tool call accumulation are completely untested. `McpBridge` and `KatlaMcpServer` also have zero tests.
- **Fix:** Add tests with mock provider returning streaming tool call data.

### 76. Add MCP server graceful shutdown and error logging
- **Crate:** katla_agent
- **File:** `src/mcp.rs` (lines 128-135)
- **Issue:** No way to signal shutdown. `serve_server` errors silently swallowed.
- **Fix:** Accept a `CancellationToken`, log errors on failure.

### 77. No keyboard navigation (Tab between widgets)
- **Crate:** katla_ui
- **Issue:** No tab-order system or focus ring. Only text inputs receive keyboard focus.
- **Fix:** Implement focus chain with Tab/Shift+Tab navigation.

~~### 78. `Fast inverse sqrt` uses single Newton-Raphson iteration~~ — Fixed in f74b05c. Added second iteration for ~0.1% accuracy.

### 79. `utils.rs` duplicates `f32` methods as free functions
- **Crate:** katla_math
- **File:** `src/utils.rs`
- **Issue:** `round()`, `ceil()`, `floor()`, etc. are thin wrappers over `f32` methods with no added value.
- **Fix:** Remove or document why the free-function form is preferred.

### 80. No archetype-based ECS storage for cache-friendly queries
- **Crate:** katla_ecs
- **Issue:** Multi-component queries iterate one sparse set and lookup others. Archetype-based storage would be more cache-friendly for common component tuples.
- **Fix:** Future optimization if query performance becomes a bottleneck.

~~### 81. `from_rows` naming in `Mat3` is misleading (actually column-major)~~ — Fixed in 37b3dbc. Renamed to `from_columns`.

### 82. Tooltip convenience method on `Response`
- **Crate:** katla_ui
- **Issue:** Users must manually check `response.hovered` and call `tooltip()`. A `response.on_hover_tooltip(ui, text)` would be more ergonomic.
- **Fix:** Add convenience method.

### 83. `collect_draws_with_context` allocates Vec and HashMap every frame
- **Crate:** katla_app
- **File:** `src/application/renderer.rs` (lines 65-120)
- **Fix:** Reuse buffers between frames by storing on `Application` or `EditorState`.

### 84. Popup panels (preferences, AI chat) close when clicking outside them
- **Crate:** katla_app
- **Issue:** Clicking anywhere outside the preferences or AI chat panel dismisses/hides the panel. This is annoying — these are toggleable panels that should stay open until explicitly closed (e.g., via their toggle button or a close action).
- **Fix:** Remove the click-outside-dismiss behavior for these panels. Only close them via their toggle button, close button, or Escape key.

### 85. Clicks on overlying panels are forwarded to the 3D scene underneath
- **Crate:** katla_app
- **File:** `src/application/mod.rs` (input handling / editor UI)
- **Issue:** When a panel (preferences, AI chat, etc.) is rendered on top of the 3D viewport, clicking on the panel still forwards the click to the 3D scene underneath. This causes unintended camera movement, entity selection, or gizmo interaction.
- **Fix:** Check if a panel is focused/hovered before forwarding input to the viewport. Consume the click event when a panel is on top.

### 86. Use serde JSON derives for tool call arguments instead of raw strings
- **Crate:** katla_agent
- **File:** `src/llm/mock.rs`, `src/co_creator/mod.rs`
- **Issue:** Tool call arguments and stream chunks use raw `String` / `serde_json::Value` instead of typed serde-deserialized structs. `ToolCallAccumulator` accumulates `arguments` as a `String`, `MockStreamProvider::tool_call()` takes `&str` for arguments, and `tool_call_to_scene_op()` manually extracts fields from `serde_json::Value`. This is error-prone and verbose.
- **Fix:** Define typed argument structs (e.g. `SpawnEntityArgs`, `DestroyEntityArgs`) with `#[derive(Deserialize)]`, accumulate arguments into a `Vec<u8>` / `String` buffer, then deserialize into the typed struct. Remove all manual `args.get("field").and_then(|v| v.as_str())` patterns.

### 87. Add undo/redo for AI agent actions
- **Crate:** katla_agent / katla_app
- **Files:** `katla_agent/src/co_creator/mod.rs`, `katla_app/src/application/editor/agent.rs`
- **Issue:** When the AI spawns, destroys, or modifies entities via tool calls, there is no way for the user to reverse those actions. The `SceneToolExecutor` already produces `UndoGroup`s from each operation, but they are discarded (the `_undo_group` is unused in `execute_tool_call`). Long chains of AI operations cannot be rolled back, which is dangerous if the AI makes a mistake.
- **Fix:** Collect `UndoGroup`s produced by `SceneToolExecutor::execute()` into a per-session undo stack on the `CoCreatorAgent` or `EditorState`. Add an `undo_last_agent_action()` method that calls `SceneToolExecutor::undo()` on the most recent group. Expose this to the user via a button in the AI panel or Ctrl+Z when the AI panel is focused. Composite operations (e.g. a template that spawns N entities) should produce a single undo group so the user can reverse the whole operation in one step.
