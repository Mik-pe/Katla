# Active Context

## Current Work

- Issue #87 fixed (2026-09-09, PR on fix/87-instanced-draw-allocation): geometry
  instancing now allocates/uploads/encodes every instance. `DrawList` owns
  frame-local object-slot allocation — `push` assigns a unique base slot per
  draw (bump allocator starting at 1; slot 0 stays reserved) and RETURNS it;
  `DrawList::from_draws` preserves already-assigned slots for filtered/merged
  lists (shadow/outline clones, Metal upload merge — re-push would reassign and
  break upload/encode agreement). `DrawCall::with_instance_index` and the
  FrameContext counter are deleted; `instance_index` is `pub(crate)` with
  `base_object_slot()` getter. Upload loops write every instance (Vulkan
  frame_lifecycle + Metal metal_renderer); capacity validation checks the whole
  base+count range (typed ObjectLimitExceeded). All encode sites pass the real
  instance count: Vulkan draw_calls/draw_helpers/parallel_geometry (firstInstance
  = base slot; shader walks objects[@builtin(instance_index)]), Metal
  geometry/depth_prepass/shadow/outline/picking (buffer-offset rebind means
  instance_id 0..N-1 already reads the right consecutive slots). The app's
  entity→slot picking map (billboards + scene draws) is built from the value
  returned by push/submit. Focused GPU test
  `katla_gfx/tests/instanced_draws.rs` (#[ignore], needs device): one 4-instance
  draw renders byte-identically to 4 direct draws; mixed list; frame-slot reuse;
  recolor-a-late-instance; capacity exhaustion → typed error. Proven to fail
  against both original bugs (upload-only-first, count-1 encode). GOTCHA: in
  Vulkan headless readback the target's row 0 is NDC y=+1 (y points DOWN) —
  pixel probing math must use row = (ndc_y+1)/2*H.

- Inspector PR #103 merged (2026-09-09, squash as 1886e070-ish via gh; branch
  deleted). Working tree was clean before starting — nothing uncommitted.

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

- Object storage slots are allocated ONLY by `DrawList::push` (assigns a unique base range and returns it) — never set `instance_index` by hand. Filtered/merged/cloned draw lists must use `DrawList::from_draws` to preserve uploaded slots; re-pushing clones would reassign and desynchronize upload from encode.
- Reserve declarative editor state slots unconditionally in a stable order. Conditional slots cause cross-view type confusion. The inspector reserves one expansion slot per `SECTION_TYPES` entry plus the picker filter String slot, all before any conditional build.
- Component type names in `EntityInfo.components` must match `ComponentRegistry` type names exactly — the UI filters the add-picker by comparing them.
- Use UI design tokens for chrome dimensions. Docked content uses panel bodies; dock tab strips provide titles.
- Vulkan frame waits do not reset fences; reset only immediately before submission. Offscreen submissions complete before returning for deterministic readback ownership.
- Canonical Linux and macOS 26 CI are required before merging. This Linux session cannot validate native Metal rendering.
- Synthetic input in headless: inject in `InteractionTestRunner::begin_frame` (before the frame renders) via `ui_context.input_mut()` for UI clicks/scroll and `app.on_mouse_input` for viewport picks; presses and releases must land on separate frames. The interaction harness lives in `katla_app/src/application/interaction_test.rs`; run with `cargo run -- --interaction-test DIR`. Section-header × hit zones are the rightmost ~20px of the header; picker row coordinates shift when the scene changes section counts.
- GOTCHA: /tmp tmpfs quota can fail katla_app doctest linking ("LLVM ERROR: IO failure on output stream: Disk quota exceeded") — rerun with `TMPDIR=<home dir>`. `katla_audio::test_engine_playback_lifecycle` is flaky on this machine (stop/state timing race; passed in the 2026-09-08 run); pre-existing, unrelated to UI work.
