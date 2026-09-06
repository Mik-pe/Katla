# Active Context

## Current Work

- Interaction test pass (2026-09-06, working tree, UNCOMMITTED): new `--interaction-test DIR` headless harness drives synthetic mouse input through the real pipelines (UI hit-testing, wheel scroll, viewport GPU picking) and reports 6 programmatic checks + 10 screenshots. Found and fixed three real bugs: (1) the hidden Preferences modal's phantom rect blocked first-click viewport picking (`is_click_on_floating_panel` now checks visibility); (2) Vulkan object-ID transients lacked `TRANSFER_SRC` usage, making pick readback invalid (color-attachment transients now match swapchain usage); (3) wheel scrolling was dead over any Selectable row because `Ignore` stopped dispatch — wheel-only events now bubble to ancestor scroll containers (katla_ui input.rs). UX: hierarchy auto-reveals the selected entity on external selection changes; headless builds now install the console logger so the console panel shows log entries. Verified: 6/6 checks, 50 workspace test suites, strict clippy, fmt, windowed `-s` exit 0. `katla_audio::test_engine_playback_lifecycle` is timing-flaky under parallel test load (passes in isolation).

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

- Reserve declarative editor state slots unconditionally in a stable order. Conditional slots cause cross-view type confusion.
- Use UI design tokens for chrome dimensions. Docked content uses panel bodies; dock tab strips provide titles.
- Vulkan frame waits do not reset fences; reset only immediately before submission. Offscreen submissions complete before returning for deterministic readback ownership.
- Canonical Linux and macOS 26 CI are required before merging. This Linux session cannot validate native Metal rendering.
- Synthetic input in headless: inject in `InteractionTestRunner::begin_frame` (before the frame renders) via `ui_context.input_mut()` for UI clicks/scroll and `app.on_mouse_input` for viewport picks; presses and releases must land on separate frames. The interaction harness lives in `katla_app/src/application/interaction_test.rs`; run with `cargo run -- --interaction-test DIR`.
