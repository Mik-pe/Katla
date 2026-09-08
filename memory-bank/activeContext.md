# Active Context

## Current Work

- Issue #85 fixed (2026-09-07, commit 37182eca): mesh index format is now
  preserved end-to-end. `create_mesh` accepts only u16/u32 via the
  `MeshIndexElement` trait (compile-time rejection of other widths),
  `MeshAsset` records a backend-neutral `index_format`, all three Vulkan draw
  paths (draw_calls, draw_helpers, parallel_geometry) bind the recorded format
  instead of hardcoded UINT32, Metal keys its upload conversion off the typed
  format (storage stays u32-by-conversion), and `GpuRenderer::mesh_index_format`
  reports the effective format. Unused `register_mesh` wrapper removed.
  Focused GPU test `katla_gfx/tests/mesh_index_format.rs` (#[ignore], needs
  device) renders u16/u32 meshes byte-identically and was proven to fail
  against the old binding. GOTCHAS for bare `init_headless` GPU tests:
  compiling model_pbr.wgsl requires `init_light_culling` AND
  `init_shadow_resources` first or pipeline creation segfaults the Intel
  driver (ValidationMode::Enabled also segfaults driver-side on this machine —
  use Disabled); the recommended per-frame order is wait_for_frame →
  set_frame_uniforms → execute_draw_calls → render, all per frame slot —
  a one-shot uniforms/object-data write leaves other slots zeroed and the
  screen blank.

- Inspector component listing + add/remove pass (2026-09-08, PR #103, branch feat/inspector-component-sections): the inspector now renders a collapsible section for EVERY component on the selected entity (payload rows for lights/camera/script/particles/audio/physics, muted notes for tag-like components), each registry-removable component gets an `×` in its section header, and the Add Component picker works end-to-end: opens (accent border), live text filter (view-local state slot), excludes owned components, alphabetically sorted rows. Found and fixed three real bugs: (1) the picker could NEVER open — the view read a never-written local state slot instead of the env flag (the old EditorUI-side `add_component_filter`/`add_component_scroll_state` dead state was removed); (2) the UI AddComponent path ran the agent's protected-entity guard, but `gizmo_state.entity` means "currently selected entity", so EVERY add on a selected entity was rejected ("is the editor gizmo and cannot be modified") — the guard stays on agent/MCP paths only, UI actions go straight to `SceneToolExecutor`; (3) the particle emitter inspector payload was hard-`None`d since the Metal feature-gating commit — restored. Also: `collect_entity_info` component names now match registry type names ("NameComponent", "ParticleEmitterComponent", plus VelocityComponent/ReverbZone/CollisionFilter now detected), and `ComponentRegistry::type_names()` sorts (HashMap order was process-random, which made picker rows shuffle between runs). Interaction harness extended to 8/8 checks (add ColliderShape via picker row click, remove it via section ×) with screenshots 11-13; harness budget now 130 frames (`game/src/main.rs`). Verified: 8/8 checks, workspace tests green, clippy clean on touched files, fmt clean.

- Metal visual verification pass (2026-09-05, macOS): the shared sky/UI shader changes from the Vulkan audit are now verified on native Metal. Two Metal-only regressions found and fixed: (1) the UI vertex descriptor lacked the `texture_index` attribute the shader now reads per-vertex, so the UI material failed to compile and EVERY frame errored ("Metal UI record has no material") with a nearly blank canvas; (2) cascaded shadow atlas content was vertically mirrored inside each atlas quadrant — Metal clip space is Y-up while the shared cascade data and sampler follow Vulkan's Y-down convention — which displaced/mirrored all sun shadows ("inverted shadows"). Fixed by a Metal-only encode-side cascade buffer with flipped clip-Y matrices plus `MTLWinding::CounterClockwise` on the shadow pipelines; sampling keeps the shared Vulkan-convention data. Verified headless from default/side/back/top-down angles plus the playground scene; red-shadow-mask shader probe (temporary, reverted) confirmed the mask tracks casters. Metal validation run (`METAL_DEVICE_WRAPPER_TYPE=1 katla -s`) exits clean.
- The game binary gained a `--camera yaw,pitch,distance` diagnostic flag (degrees) for headless captures from explicit orbit poses; `Application::set_editor_camera_pose` drives it.
- Pre-existing macOS-only clippy warnings fixed: `encode_cascade_draws` too-many-arguments allow, and the macOS `collect_and_upload_lights` now reuses `point_lights_buffer` instead of allocating per frame.

## Ongoing Architecture Work

- Complete render-graph execution plans (#56): graph-declared attachment/load/store/clear policy, viewport/scissor, and generic executable payloads still need to reach all native handlers. Exact pass identity, application-owned topology, pass-local submissions, explicit picking, and dead-pass culling already exist.
- Preserve the engine/application boundary: custom graphs may be empty, UI-only, reordered, or repeated. Never invent editor topology in a backend.
- Shadow and depth work remain explicit side-effect roots while their native targets are backend-owned. Vulkan particle emission/simulation, animation, and light-culling work also require explicit side effects until their buffers are graph resources.
- The Metal frame-uniform preparation bridge belongs in the eventual frame-slot/buffer ownership design (#36/#31). Some Metal handlers still resolve backend-owned textures; transient allocation is not live-range aliased.
- Metal particle reset and entity-destruction cleanup still need routing through the common emitter driver.
- Private Metal texture storage sampling still needs an Xcode GPU capture; the storage-mode probe is the starting point. Staged uploads and shared storage already work.

## Conventions and Validation Limits

- Reserve declarative editor state slots unconditionally in a stable order. Conditional slots cause cross-view type confusion. The inspector reserves one expansion slot per `SECTION_TYPES` entry plus the picker filter String slot, all before any conditional build.
- Component type names in `EntityInfo.components` must match `ComponentRegistry` type names exactly — the UI filters the add-picker by comparing them.
- Use UI design tokens for chrome dimensions. Docked content uses panel bodies; dock tab strips provide titles.
- Vulkan frame waits do not reset fences; reset only immediately before submission. Offscreen submissions complete before returning for deterministic readback ownership.
- Canonical Linux and macOS 26 CI are required before merging. This Linux session cannot validate native Metal rendering.
- Synthetic input in headless: inject in `InteractionTestRunner::begin_frame` (before the frame renders) via `ui_context.input_mut()` for UI clicks/scroll and `app.on_mouse_input` for viewport picks; presses and releases must land on separate frames. The interaction harness lives in `katla_app/src/application/interaction_test.rs`; run with `cargo run -- --interaction-test DIR`. Section-header × hit zones are the rightmost ~20px of the header; picker row coordinates shift when the scene changes section counts.
- GOTCHA: /tmp tmpfs quota can fail katla_app doctest linking ("LLVM ERROR: IO failure on output stream: Disk quota exceeded") — rerun with `TMPDIR=<home dir>`. `katla_audio::test_engine_playback_lifecycle` is flaky on this machine (stop/state timing race; passed in the 2026-09-08 run); pre-existing, unrelated to UI work.
