# Progress

## Completed Recently

- **Issue #35 slice 1: Vulkan allocates aliased transients from compiled
  lifetimes (2026-09-11, PR #125 squash-merged as 57f3c3b9, CI green both
  platforms, main green post-merge)** — the `TransientAllocationPlan`
  stopped being a diagnostics-only projection: `initialize_transient_textures`
  builds it (lifetimes from the same compiler pass that schedules/syncs) and
  routes slot groups through the new `RenderGraphBackend::create_transient_slot`
  (replaces `create_transient_texture`; returns textures one-to-one with
  member descs). Vulkan lowers multi-member slots to `ALIAS` images bound at
  offset 0 of one `VkDeviceMemory` per frame slot sized for the largest
  member; memory type = intersection of member requirements, preferring
  LAZILY_ALLOCATED for attachment-only slots (unsampled depth gains
  TRANSIENT_ATTACHMENT usage) else device-local; `Rc<VkSlotMemory>` keeps
  slot memory alive until the last member dies (drop-order safe). Metal loops
  standalone allocations over the same grouping. `set_transient_aliasing(false)`
  switch keeps standalone allocations without changing semantics.
  TWO latent sync bugs found by the new device tests: (1) undefined-source
  bootstrap transitions used srcAccess NONE — under aliasing the image's
  memory carries the prior member's attachment store, so the transition is a
  write needing ALL_COMMANDS/MEMORY_WRITE sources (GPU-assisted validation:
  WRITE_AFTER_WRITE); (2) `ImageBarrier::deduce_transition_masks` had no
  TRANSFER_SRC→COLOR_ATTACHMENT case — the picking readback's restore
  transition silently fell back to TOP_OF_PIPE/NONE. Diagnostics schema v9:
  `transient_slots` (members, bytes, live execution span) in JSON + text.
  GOTCHAS learned: single-member plan slots must STILL create their textures
  (an early `.filter(len > 1)` silently skipped them — the backend's
  standalone path is the right router); headless `render()` steps the frame
  slot BEFORE returning, so post-frame transient readback must capture the
  slot pre-render (picking readback via `queue_picking_readback` restores the
  tracked layout, making it safe mid-graph-lifetime). The editor graph's 5
  transients are all exported/incompatible → 5 standalone groups (no savings
  there); the diagnostics golden shows shadow_atlas+hdr_color sharing slot 0.
  Validation: 455 lib tests; `tests/transient_aliasing.rs` (aliased storage
  proven by cross-member pixel readback across repeated frames on one frame
  slot, + disabled-switch contrast, both validation-clean); full device
  suites; contract 12/12 serial on real Intel; clippy CI-exact (lib +
  workspace); 100-frame headless 0 graphics errors / 1 WARN, re-init count
  identical to main.
- **Issue #37 slice 2: sync transitions in all diagnostics exports + golden
  snapshots (2026-09-11, PR #124 squash-merged as 0c1da394, CI green both
  platforms)** — diagnostics schema v7 → v8. `RenderGraphDiagnosticTransition`
  now carries the sync op's subresource range (producer∩consumer intersection);
  transitions ordered by execution position in every export (was declared-pass
  order); text export renders every op (frame-start steady-state seeds,
  per-pass ops, frame-end `imported_final` contract ops) with
  `[producer -> consumer] rN (name), aspects, mips a+b, layers c+d: before ->
  after (reason|hazard H)`; DOT renders frame-boundary ops as dashed edges
  through `frame_start`/`frame_end` nodes — including the anchor-less
  `frame_start -> frame_end` edge for contract finals on subresources no live
  pass wrote — while pass-to-pass ops stay on the dependency edges (no
  double-render). Golden snapshots `katla_gfx/tests/goldens/
  render_graph_diagnostics.{json,text,dot}` pin a canonical
  shadow→geometry→lighting→present chain (backbuffer contract
  ColorAttachment→PresentSrc, RAW hazards, aspect-fragment bootstraps,
  prev-frame seeds); bless via `KATLA_BLESS_GOLDENS=1 cargo test -p
  katla_gfx --lib render_graph::diagnostics`. GOTCHA confirmed: CI runs NO
  integration tests except `--test contract`, so golden files must be loaded
  by a `#[cfg(test)]` lib test (CARGO_MANIFEST_DIR path), not a tests/ file.
  GOTCHA: mid-flight rebase onto #123 (which landed while this branch was
  validating) — fetched, rebased clean, re-ran lib tests (446 → 452 incl.
  #123's six) + fmt/clippy + headless gate on the rebased tree before push.
  Validation: 452 lib tests (12 diagnostics incl. golden), workspace suite
  green, headless 100-frame exit 0 / 0 ERROR / 1 WARN (= baseline), evidence
  comment on #37 (issue stays open: encoder/queue boundary → #33 remainder,
  allocation/aliasing views → #35, frame slots → #36, Metal residency → #55,
  encoder traces + comparison → #56, CI artifact upload, capture docs).
- **Issue #98 emission slice: DrawCall emission typed as TextureHandle
  (2026-09-11, PR #122 squash-merged as a011ec7f, CI green both
  platforms)** — `DrawCall.emission` and app `DrawableComponent.emission`
  hold `TextureHandle` (default NONE) instead of a raw f32 bindless slot;
  both backends resolve at prepare/encode time — Vulkan
  `resolve_emission_texture_slot` (pub in bindless_queries, mirroring the
  #117 `resolve_material_texture_slots` pattern), Metal
  `resolve_emission_texture_slot_impl` at object upload;
  `DrawCall::material_params` takes the backend-resolved slot, keeping the
  `material_params.w` ABI (billboard's reuse of w as albedo index intact;
  no shader edits). NONE/stale resolve to 0 — the shaders'
  `emission_idx > 0u` sentinel — deliberately not DEFAULT_EMISSION_SLOT
  (0 skips the sample). App: GLTF loader binds the emission handle
  directly, billboard path binds the icon handle (app-local
  `get_bindless_index` helper deleted); write-only
  `EditorState::stencil_indicator_bindless_index` deleted. New device
  suite `tests/typed_emission_binding.rs` (registered → own slot,
  NONE/stale → 0 ≠ withheld slot, later registration unaffected).
  Validation: 50 katla_gfx device tests, workspace tests, 100-frame
  headless run 0 ERROR / 1 WARN (baseline), interaction harness 8/8; all
  re-run after rebasing onto #119's b0f42fc3 mid-flight.
  GOTCHA: never run `cargo test --workspace --tests -- --ignored` —
  pre-existing ignored katla_app tests hang forever (transform_hierarchy
  cycle test spins at 100% CPU; ui hit-testing); scope device suites to
  `-p katla_gfx --tests -- --ignored`.
- **Issue #97 CLOSED: one cross-backend graphics contract suite (2026-09-11,
  PR #119 squash-merged as b0f42fc3, CI green both platforms incl. the
  suite)** — `katla_gfx/tests/contract/` runs 12 scenarios against the
  platform backend through a backend-neutral harness: Vulkan on Linux
  (lavapipe in CI, real Intel locally), Metal on macOS (first real Metal
  device execution in CI, with MTL_DEBUG_LAYER +
  METAL_DEVICE_WRAPPER_TYPE). Harness: `ContractRenderer` (headless init,
  capability table, canonical frame order, per-frame BGRA readback per
  backend — Vulkan async readback / Metal headless drawable + getBytes),
  `build_graph` → AnyFrameGraph, `finish()` asserts the captured validation
  log. Scenarios: u16/u32 indexed draws byte-identical; per-object
  transform/color swap; instanced == 4 direct draws + mixed lists + late
  recolor + typed capacity exhaustion; dynamic mesh
  same-size/shrink/grow/empty/repopulate with retirement draining;
  material across three alternating graph configs; declared Load extends /
  Clear replaces; stale handles never alias reused slots; double destroy
  harmless; destroyed slot withheld until frames drain then returns
  exactly; frame slots independent; typed invalid-descriptor rejections;
  capability table + documented direct-UI-pass no-op.
  FOUND+FIXED four real divergences: Metal honored neither
  `PipelineDescriptor.color_format` nor `DepthState` (every non-UI
  pipeline hardcoded RGBA16Float + D32S8 depth) — now honored with Auto
  keeping the HDR default; Metal skipped the static-mesh-immutable
  rejection (MetalMesh records MeshUsage now); Metal's geometry/UI record
  encoders ignored graph-declared attachments (depth bound only when the
  record declares it + declared color ops honored; UI record renders the
  PASS's material — set_ui_material trait plumbing removed end-to-end);
  Metal reports u32 for u16 meshes (upload conversion). Also added
  `GeometryPass::without_depth()` (builder injects reverse_z_default into
  every uses_depth pass — depth-free passes must opt out) and
  `pipeline_descriptor::CullMode` re-export (the descriptor field type was
  unnameable without the validation feature). CI: Linux runs the suite on
  lavapipe SERIAL with `--skip graphics::pbr` (lavapipe segfaults in the
  PBR path and races concurrent instance creation); macOS runs everything
  under Metal API validation. Capabilities encode the honest backend
  differences: `api_validation_capture`, `retirement_diagnostics`,
  `preserves_index_width`, `flips_direct_ndc_y` (direct-to-drawable graphs
  render y-mirrored on Metal — the app's tonemap does the flip; UI pixel
  space is unmirrored on both). Verified: gfx lib 445, contract 12/12
  Linux + 12/12 macOS, existing device suites green, workspace check +
  suite green. Docs: docs/contract-suite.md.


- **Issue #98 UI slice: dead `UiDrawCommand.texture` deleted (2026-09-11,
  PR #121 squash-merged as 44c64a37, CI green both platforms; issue remains
  open)** — the field was write-only: Vulkan's `execute_ui_draw_list` never
  consumed it and Metal pushed `.index()` into a `UiUniforms` push-constant
  member no shader entry point reads (headless_render's device test already
  proved inertness — commands carry mismatched textures while pixel assertions
  depend solely on baked indices). `UiUniforms` collapsed to one `vec4f`
  `[width, height, ndc_y_flip, unused]` (WGSL uniform structs need a 16-byte
  stride), making Metal's inline `setVertexBytes` push byte-identical to the
  layout Vulkan already wrote; the app `UIRenderer` stops fabricating
  `TextureHandle::from_raw(bindless_slot, 0)`. Per-instance/per-vertex
  `texture_index` remains the shader-visible ABI, resolved per frame through
  `GpuRenderer::get_bindless_slot`.

- **Issue #33 slice 1: compiled synchronization plan (2026-09-11, PR
  #118 squash-merged as 922778f9, CI green both platforms; issue remains
  open)** — `ExecutionPlan` now carries a backend-neutral `SyncPlan`
  (new `render_graph/sync_plan.rs`): ordered per-pass image sync ops with
  subresource ranges, before/after states (`ImageSyncState`:
  usage/pipeline-stage/access-mode), hazard reasons (RAW/WAR/WAW), and
  imported-contract initial seeding plus frame-end final ops — all derived
  from the same typed accesses that build the dependency DAG. Computed by
  one forward scan run twice: a discovery pass, then a seeded pass where
  each transient starts in its previous-frame end state (frame-periodic
  steady state); imports always seed from their contract's initial state.
  Fresh textures get discard-safe Undefined→target bootstrap ops
  (frame-start state == target) that the backend coalesces once the
  tracked layout matches; same-state cross-pass hazards encode same-layout
  ordering barriers (render-pass instances may overlap without one).
  Vulkan (`frame/barriers.rs`, rewritten) translates states directly to
  sync2 masks/layouts/aspect-intersected ranges — no layout-pair
  inference, no TOP_OF_PIPE fallback, tracked layout is ground truth
  (oldLayout=tracked; UNDEFINED source → srcStage=dstStage/srcAccess=0).
  Frame-end contract ops run after the last live pass. Deleted:
  PassBarrierCache + ensure_barrier_cache + barrier_cache(),
  insert_post_pass_barriers, the shadow-pass `set_state` poke, the three
  dead `RenderGraphBackend` transition hooks, `ResourceState::
  to_vk_stage_flags/to_vk_access_flags` (+ test), and
  `TransientTextureOps` state()/set_state() with both backends' Cells
  (Vulkan keeps only the layout Cell for the bootstrap guard). Metal
  consumes the same plan: `MetalExecutionPlan` records per-op
  `MetalSyncCoverage::TrackedResource` classification (private-storage
  tracked textures; load/store realize attachments; tonemap fence
  documented as pre-plan), observable via trace logs, pinned by unit
  test. Diagnostics schema v7: synchronization entries carry
  before/after states + reason + frame-start/frame-end anchors
  (before_pass/to_pass are Options); dependencies now derive from live
  DAG edges + coarse set intersection (an intervening access subsumes a
  transitive hazard in the sync ops while the edge still orders the
  passes). Tests: 12 sync-plan compiler tests incl. steady-state cycle,
  bootstrap, import contracts, subresource ranges, RAR coalescing,
  same-state WAW; 445 gfx lib tests; all 35 device-suite tests on Intel
  Vulkan; full workspace suite; 100-frame headless editor run exit 0
  with ZERO validation errors and ERROR/WARN parity with main (an
  earlier iteration rendered single-write transients from UNDEFINED on
  frame 0 — VUID-09592 x10, caught ONLY by this run); capture
  pixel-diff vs main 60,735 px vs main-vs-main noise floor 62,625 px
  (same bbox — timing-driven fire); interaction harness 8/8. Remaining
  #33 slices in the issue comment: backbuffer contract consumption
  (renderer acquire/present barriers remain), out-of-graph barrier
  callers keep deduce_transition_masks until #31, scene depth keeps
  depth_render_pass_sync while backend-owned, queue/encoder boundary
  modeling (semaphores, Metal fence).

- **Issue #98 core slice: typed material texture bindings (2026-09-11, PR
  #117 squash-merged as 6da53d3c, CI green both platforms; issue remains
  open for follow-up slices)** — `MaterialTextures` is now four
  `TextureHandle` roles (albedo/normal/metallic_roughness/occlusion,
  default all-NONE) instead of a raw `[u32; 4]` of bindless slots.
  `set_material_textures(material, MaterialTextures)` replaces
  `set_material_texture_indices` on the GpuRenderer trait, AnyRenderer,
  Vulkan, Metal, and the renderer_features mock. Each backend resolves
  handles to slots at exactly one point — Vulkan in `execute_draw_calls`
  via pub `resolve_material_texture_slots` (public so tests can validate
  the binding table), Metal at object upload via
  `resolve_material_texture_slots_impl` — with one shared fallback
  policy: NONE and stale handles resolve to per-role default slots
  (`DEFAULT_*_SLOT`, now pub in `katla_gfx::texture`). A stale handle can
  never sample whatever occupies a recycled slot (composes with #83
  generations + #84 slot withholding — device-verified). App: the GLTF
  loader binds handles directly and its per-role bindless-index derivation
  is deleted (emission stays a raw DrawCall field, tracked in #98).
  Device suite `tests/typed_material_textures.rs`: defaults resolve to
  [0,1,2,3]; per-role handles resolve to their exact registered slots;
  destroyed-texture handle falls back to the role default and NOT the
  destroyed slot; shared texture across materials resolves identically.
  Verified: 423 gfx lib tests, workspace suites, all 35 device-suite
  tests, CI-exact clippy both gates, fmt, 100-frame headless game run
  (DamagedHelmet GLTF exercises the typed path; ERROR/WARN count matches
  main), interaction harness 7/8 on this branch (the
  viewport_click_picks_object failure was the pre-#116 shadow-cull
  regression — this branch predates #116; corrected on the PR). Metal
  diff hand-audited (type-level only; argument buffer default-fill keeps
  the unified occlusion fallback safe). Remaining #98 slices listed in
  the issue evidence comment: UiDrawCommand fabricated handles + Metal
  vertex-UI `.index()` pass-through, viewport transient TextureIds +
  `set_overlay_texture_indices`, `DrawCall.emission`, `Texture::resize`
  replacement retirement.

- **Issue #30 CLOSED: typed template accesses + imported-image state contracts
  (2026-09-11, PR #116 squash-merged as 09573dcf, CI green both platforms,
  main green post-merge)** — Also REPAIRED MAIN: since slice 1 (c3500878,
  2026-09-10) every editor frame failed with `Pass 'shadow' was culled` and
  rendered black. Root cause: `refine_inferred_image_accesses` narrowed the
  shadow pass's write to a DEPTH-only range while the geometry pass's
  generic read stayed a COLOR-only `sampled_read`; disjoint aspects
  produced no RAW edge, so liveness culled the shadow pass and its
  submission was rejected mid-frame (stuck command buffer, black 17 KB
  capture vs healthy 586 KB). CI and the gfx device suites never exercise
  the editor graph — only the game binary did. Fix: generic sampled reads
  declare the whole-resource range (ALL aspects — sampling reads whichever
  aspect the image carries; the graph cannot narrow it without the format;
  `.with_range` for precision). The interaction harness's recorded 7/8
  `viewport_click_picks_object` "pre-existing" failure was this same
  regression; 8/8 after. Contracts: `ImportedImageContract {initial,
  required_final}` with `undefined()`/`arrives_in(state)`/`.must_end_in(state)`;
  `import_resource(name, handle, contract)`; `backbuffer_contract(...)`
  overriding the built-in backbuffer default (observable contents — the
  old load exemption, now declarative); compile validation
  (`LoadingUndefinedImportedContents` when loading imported contents with
  Undefined initial state, `UnreachableImportedFinalState` when a required
  final state has no live accessing pass and differs from initial);
  diagnostics expose contracts per imported resource in JSON/text/DOT
  (schema v6; #33's sync plan consumes them later). Templates: all 11
  built-ins hand-declare `NamedImageAccess` via `named_image_access`
  (passes/mod.rs) — attachment targets as ColorAttachment/
  DepthStencilAttachment accesses (read-write when loading/blending),
  generic reads as all-aspect sampled accesses; per-template tests pin the
  declarations; `refine_inferred_image_accesses` remains only for the
  low-level `PassDesc`/`SimplePass` API (ResourceId-based). New
  `color_attachment_read_write` constructor. Regression test
  `sampling_a_depth_atlas_orders_after_the_shadow_pass_and_keeps_it_live`
  pins the editor-shape hazard. Verified: 430 gfx lib tests, full
  workspace suite, device suites (attachment_semantics incl. --ignored,
  render_graph_test, viewport_pass_test, headless_render), headless
  editor capture pixel-verified with cast shadows, interaction harness
  8/8, fmt clean, both CI clippy gates clean, metal/ audited (no touched
  API referenced). Buffer resources → #31; sync-plan consumption → #33.

- **Issue #84: deferred native-resource retirement generalized (2026-09-11,
  PR #115 squash-merged as 126312ce, CI green both platforms, issue
  closed)** — `BufferRetirementQueue` became `RetirementQueue` whose
  `RetiredResource` variants own their native object and free it by Drop at
  drain: Buffer, Texture (`Rc<Texture>` — frees image/view/sampler at last
  reference), Pipeline (`Box<AnyPipeline>`), DescriptorSetLayout,
  SkeletonBuffer, and BindlessSlot (returned by drains so the caller
  releases it through `BindlessTextureManager`). Converted paths:
  `destroy_texture` (Rc AND its bindless slot retire — the slot stays
  withheld from the free list until expiry, so new textures can never
  resolve through a slot an in-flight submission still reads; released at
  `wait_for_frame` drain, `drain_all` in destroy/recreate_swapchain),
  `destroy_material` (descriptor layout + both pipelines),
  `destroy_skeleton` (joint buffer), material hot reload
  (`reload_material_shader` old pipeline) and descriptor-layout
  invalidation (`invalidate_compiled_materials` now returns the removed
  pipelines for retirement), and UI auto-grow (`BufferObject::resize`
  returns a `RetiredBuffer`; `upload_data` returns `Option<RetiredBuffer>`;
  all 6 UI upload sites in ui_rendering.rs retire; mesh creation-sized
  uploads `debug_assert!` none). `TextureManager::destroy` returns
  `Option<Rc<Texture>>`. Diagnostics: public
  `pending_retirements() -> RetirementSnapshot` (per-kind counts,
  knowable device bytes via `Allocation::size`, oldest pending frame,
  `summary()` string; root-exported next to VulkanRenderer) replaces
  `pending_buffer_retirements`; drains debug-log each retired resource.
  Device suite `tests/resource_retirement.rs`: slot withhold → different
  slot allocated → exact slot released at expiry → reusable after
  (bindless free list is a LIFO stack, so the released slot is popped by
  the next registration); material+skeleton retire+drain (graph keeps a
  separate live material — empty-submission renders still validate pass
  materials, a destroyed graph material errors every later frame);
  12-iteration create/destroy stress bounded + fully drained with
  validation callback clean. Sabotage-verified: restoring immediate
  free/release fails the slot-withholding test. Verified: 422 gfx lib
  tests, full workspace suites (katla_audio flake re-ran green), ALL
  device suites, CI-exact clippy both gates, fmt (CI caught one post-fmt
  formatting drift — rerun fmt immediately before pushing), 100-frame
  headless game exit 0 with ERROR/WARN count identical to clean main
  (124/124), interaction harness 7/8 (only the known pre-existing
  viewport_click_picks_object failure). Deferred: `Texture::resize` image
  replacement (#98 — queue unreachable from Texture internals); Metal
  needs no equivalent (MTLCommandBuffer retains referenced resources,
  extending native lifetime past destroy through in-flight submissions).

- **Issue #37 slice 1: typed image accesses in text and DOT diagnostics
  (2026-09-11, PR #113 squash-merged as bd5e9a1f, CI green both
  platforms)** — Parallel session's work, finished and landed here: export
  access labels now carry mode, usage, stage, aspects, and base/count mip
  and layer ranges; read-write declarations emit both edge directions;
  culled accesses keep dotted styling and stay visible. JSON already
  contained these fields. Regression coverage compares an explicit nonzero
  depth/stencil range across text, DOT, and JSON and checks quoted
  resource-name escaping. No compiler or backend semantics changed — the
  branch was rebased onto post-#114 main (its macOS CI failure was the
  pre-existing private-handles-module breakage #114 fixed) and its leaked
  memory-bank hunks were stripped before merge. 419 gfx lib tests, strict
  clippy green. Remaining #37 scope: synchronization transition details
  (#33), physical allocation/aliasing views (#35), frame-slot ownership
  (#36), Metal argument-table/residency fields (#55), runtime encoder
  traces and compiled-vs-emitted comparison (#56), golden snapshots, CI
  artifact upload, capture/compare documentation.

- **Issue #83: generational resource handles (2026-09-10, PR #114
  squash-merged as 18959760, CI green both platforms)** —
  `Handle<T>` identifies `(slot, generation)`; `ResourceStorage<T, M>` is
  marker-keyed, bumps a slot's generation on every removal, and validates
  both parts on every get/get_mut/remove/contains (`live_slot`; wrapping
  generation skips 0 so a slot's initial generation is never re-issued).
  `Handle::new(index)` deleted repo-wide; `from_raw(index, generation)` is
  the only raw constructor. Copy/Clone are manual impls (derive adds a
  `T: Copy` bound generic code can't prove for markers). Migrated: Vulkan
  AssetRegistry (mesh/material/pipeline), TextureManager, the parallel
  skeleton descriptor+buffer storages (insert back-to-back, one handle
  addresses both, debug_assert), Metal meshes/materials/textures/skeletons
  storages and all encode paths, and BOTH particle emitter pools (shared
  pool lifecycle refactored onto `ParticleEmitterPool` for pure testing;
  bespoke `particles::types::EmitterHandle` deleted for the shared alias).
  App layer: GpuResourceTracker refcounts by full handle; skeleton copy
  commands carry SkeletonHandle through the frame graph (tuple typed);
  collider scene descriptors persist mesh_handle_generation;
  asset-browser `TextureId::from_handle(index, generation)` packs both;
  UIRenderer bindless map keyed by full handle. Two latent bugs fixed:
  `TextureManager::list_unregistered_textures` derived handles from
  enumerate position over live slots; tracker index-keyed refcounts let a
  stale release destroy a slot's replacement. Tests: storage lifecycle
  suite in handle.rs (stale-after-reuse, double-remove, generation
  cycling, iter_enumerated, NONE/out-of-range), emitter pool pure suite,
  tracker stale-release test, device suite
  `tests/generational_handles.rs` (mesh/texture/skeleton/material/emitter
  destroy→reuse→stale, replacement survives stale destroys;
  mutation-verified). Verified: 418 gfx lib tests, 2094 workspace tests,
  all 8 device suites, 100-frame headless run exits 0, interaction
  harness 7/8 (viewport_click_picks_object fails IDENTICALLY on clean
  main — pre-existing, not this change), exact CI clippy/fmt/check gates.
  ALSO REPAIRED MAIN: macOS CI had been red since #95's merge
  (61ad68e9) — metal/execution_plan.rs test referenced the private
  `render_graph::handles` module; test-only cfg code compiles only in
  the macOS `cargo test --lib` step. Deferred: raw bindless-slot
  pass-through in UiDrawCommand.texture + viewport transient TextureIds
  (#98); deferred native retirement (#84).

- **Issue #30 slice 1: range-aware dependency analysis (2026-09-10, main as
  c3500878)** — Compiler analyzes typed `ImageAccess` declarations
  (`PassInfo.image_accesses`) instead of coarse read/write lists.
  `ResourceAccessState` tracks outstanding writer/reader VERSIONS WITH
  RANGES; overlapping ranges → minimal RAW/WAR/WAW edges, disjoint
  mips/layers/aspects independent (same parallel level). A writer replaces
  only the subresources it covers — `ImageSubresourceRange::subtract`
  (which already existed for exactly this) keeps partial versions alive on
  the remainder, so later reads of untouched ranges still bind to the
  original version. RMW never self-depends. Convenience ImageAccess
  constructors added (attachment/transfer/present). 7 new compiler tests
  (disjoint mips/layers/aspects, overlap WAW, subrange RAW, partial
  overwrite retention, RMW). Whole-range graphs compile identically to the
  previous analysis (all 409 gfx lib tests + device suites + workspace
  check + fmt + clippy green). Remaining on #30: imported-image
  initial/final state contracts; explicit typed declarations in built-in
  templates.

- **Issue #95: Vulkan attachments obey declared graph ops (2026-09-10,
  merged to main as 61ad68e9 + 098456cd)** —
  New `AttachmentOps` + `DepthStencilAttachmentOps` (render_pass/types.rs);
  `PassDesc.color_attachments` became `Vec<(ResourceId, AttachmentOps)>`,
  `depth_attachment` per-aspect (depth+stencil). All pass templates declare
  their attachment semantics (write_color_ops / depth_config two-arg APIs;
  UIPass/OverlayLoad, fullscreen+composite Clear canvas, shadow Clear 1.0,
  object-id Clear uint0). Builder resolves name→resource generically,
  normalizes the reverse-Z depth default, and validates pre-encode:
  missing/stray ops, clear-value aspect, Load-without-producer (imported
  backbuffer exempt), compute rejection, uses_depth contradictions, depth
  range. Deleted backbuffer_written + transient-state heuristics and every
  hardcoded clear; per-aspect exec resolution shared via
  resolve_color_attachments / resolve_frame_depth_attachments +
  depth_attachment_info (DontCare→NONE_EXT). Diagnostics Display +
  RenderGraphDiagnosticPass carry declared ops; Metal execution plan reads
  the same decls (format via new FrameGraph::resource_format). Tests: 11
  pure validation/normalization + trace test + device
  tests/attachment_semantics.rs (Clear replaces / Load extends,
  sabotage-verified: flipping UI Load→Clear fails it). GOTCHAS: validation
  runs on live passes post-culling (unobserved test graphs need
  export_resource); GeometryPass::write_color_with renamed
  write_color_ops(ops struct), depth_config now (AttachmentOps,
  AttachmentOps); game bench `performance_benchmark.rs` missing `rand` is a
  pre-existing workspace-check failure. Verified: 401 gfx lib tests, all
  workspace suites, 6 device suites incl. headless_render (backbuffer now
  asserts DECLARED opaque black), clippy clean on touched files, fmt clean,
  100-frame headless editor run with VK validation exits 0, screenshot
  pixel-checked (shadows/particles/sky/UI intact).

- **Issue #96: static meshes staged into device-local memory (2026-09-09,
  PR #111 merged as deb7dd7a)** —
  Every mesh lived in CpuToGpu streaming memory regardless of mutability.
  Fix: `MeshUsage::Static` → `GpuOnly` buffers populated by new
  `vulkan/staged_upload.rs` `StagedUploadBatch` (one staging allocation +
  one copy submission per creation; post-copy buffer barrier
  TRANSFER_WRITE→VERTEX_INPUT; queue submission order makes the data
  visible to any later draw); `MeshUsage::Dynamic` keeps direct
  host-visible writes (#86 path unchanged). Submission completion is
  deferred: fence + command buffer + staging parked in
  `VulkanContext::pending_staged_uploads`, released at frame-slot waits
  and after device-idle in destroy/recreate_swapchain — a blocking
  submit_and_wait per creation measured 74x slower on many small meshes
  (~215 µs fence round-trip on Intel iGPU); deferred lands at ~26 µs/mesh.
  Device-local allocation failure falls back to host-visible + direct
  write + warn; placement observable via `mesh_memory_report` (per-buffer
  location counts + index class) and `pending_staged_uploads`.
  `destroy_mesh` retires buffers through the #86 retirement queue (staged
  copies / in-flight frames may still reference them — first slice of
  #84). Buffers gained `from_native` wrappers, fallible
  `try_new`/`try_with_usage`, and a loud guard against direct uploads to
  device-local memory. Bench `benches/mesh_upload.rs`: 64 small meshes
  191 µs→1.69 ms, one 48.6k-vertex mesh 1.25 ms→1.45 ms (creation pays one
  submission per mesh; steady-state encoding unchanged). Tests:
  `tests/static_mesh_placement.rs` (staged render parity + placement
  reports + retirement draining across rendered frames; 50 small + 48k
  mesh with bounded staging/retirement release), in-crate fallback +
  typed exhaustion via the inject hook, placement pure test; all existing
  render device suites pass through the staged path (mesh_index_format
  u16/u32 byte-identical, instanced_draws byte-exact). Verified: 55
  workspace suites green (audio flake excluded), CI-style clippy clean,
  fmt clean, headless 100-frame game run clean. CI FIX included: removed
  the runner's preinstalled google-chrome apt source before apt-get
  update (dl.google.com served hash-mismatched metadata from 17:16 UTC;
  file is deb822 `.sources`, glob must not stop at `.list`).
  Exclusions: cross-call batching into one submission per frame needs
  #89's frame-scoped flush; general retirement (#84) beyond meshes.

- **Issue #86: consistent, atomic dynamic mesh updates (2026-09-09, PR #110
  merged as 71077a92)** —
  Vulkan's update ignored vertex_count, copied the interleaved blob into the
  Position-only SOA buffer (other attributes stale), never updated logical
  counts, dropped index payloads when no index buffer existed, and grew via
  `expect()`-panicking realloc that freed old buffers under in-flight
  submissions; Metal failed typed on any growth and ignored vertex_count.
  Fix: shared `validate_dynamic_update` (root-exported) gates updates on
  both backends; `MeshAsset` records `index_count` + `attributes`, MetalMesh
  records `vertex_count`/`vertex_stride`; all Vulkan draw sites encode
  `mesh.index_count` and skip empty meshes (Metal guards added to 6 sites);
  updates deinterleave every attribute via shared `split_attribute_bytes`;
  growth uses fallible `VertexBuffer/IndexBuffer::try_new` replacements
  committed atomically (allocation failure → typed error, mesh intact —
  proven with the #94 inject hook); replaced Vulkan buffers retire through
  new `vulkan/retirement.rs` `BufferRetirementQueue` (monotonic
  `SwapData::frame_counter`, drain age >= frames_in_flight in
  wait_for_frame, drain-all after device-idle in destroy/recreate_swapchain);
  Metal growth relies on MTLCommandBuffer resource retention; empty ↔
  populated transitions and u32 index width preserved; new
  mesh_vertex_count/mesh_index_count/pending_buffer_retirements queries.
  Tests: 3 pure contract tests + `tests/dynamic_mesh_updates.rs` device
  suite (render-level grow/shrink/same-size/empty round-trip/repeated
  interleaved updates/retirement draining, typed rejections) + in-crate
  allocation-failure atomicity + extended Metal unit test (macOS CI).
  Verified on Intel Vulkan: all gfx test targets green incl. mesh
  device suites, 54 workspace suites green (katla_audio known flake
  excluded), CI-style clippy clean, fmt clean, headless 100-frame game
  run exits 0. Exclusions: BufferObject::resize (UI auto-grow) and
  destroy_mesh still free immediately — #84 should reuse this retirement
  queue. GOTCHA: headless capture "background" is lit gray [89,89,89],
  not the clear color, and readback bytes are B8G8R8A8 (red = byte 2);
  pixel probes need a distinct tint + channel-aware comparison.

- **Issue #90: typed mesh descriptors, no Pod guessing (2026-09-09, PR #107
  merged as c266c367)** —
  `create_mesh<T: Vertex, U: MeshIndexElement>` + explicit topology replaces
  `Pod`+`TypeId` guessing and the blob-as-position fallback (deleted with
  both hand-rolled deinterleaves); new `MeshDescriptor`/`PrimitiveTopology`/
  `MeshUsage` recorded on `MeshAsset` and `MetalMesh`; validation (empty,
  attribute/format agreement, stride, index range, Position, topology)
  before upload; all mesh creation returns `Result` (SOA, dynamic,
  primitives included); callers migrated (init propagate,
  spawn fallbacks, serialization strings). Tests: `mesh_descriptors.rs`
  (4 pure + 5 device, sabotage-verified); `mesh_index_format` +
  `instanced_draws` device tests byte-identical through the new generic
  deinterleave. Verified: workspace check (pre-existing bench failure only),
  380 lib tests, clippy clean. Exclusions: per-topology encoding (feeds
  #100); stacks on #106, rebases to main on merge.

- **Issue #99: typed gfx errors, loud texture creation (2026-09-09, PR #106
  on fix/99-typed-gfx-errors, CI running)** —
  New `InvalidDescriptor`/`AllocationFailed`/`UploadFailed`/`StaleHandle`
  variants with structured context; pure `TextureDescriptor::validate_data`
  gate; `create_texture`/`create_texture_solid`/`create_ui_font_atlas`
  fallible on Vulkan + Metal + `AnyRenderer` (Metal placeholder masquerade
  removed; Vulkan bindless panics typed with rollback); update paths return
  `UploadFailed`/`StaleHandle` instead of warn-and-Ok; replacement-first
  font-atlas updates; explicit logged asset-layer fallbacks (GLTF default
  texture, thumbnail skip, icon skip); Metal mesh-update truncation now
  fails typed. Tests: `texture_errors.rs` (4 pure + 3 device, all green on
  Intel Vulkan; sabotage-verified). Verified: workspace check, 380 lib
  tests, clippy clean, headless_render device test green. Exclusions: mesh
  creation Result-ification moves with #90; RenderGraphBackend + capability
  branching with #92/#93. Stacks on merged #105.

- **Issue #91: required renderer operations have no silent no-op defaults
  (2026-09-09, PR #105 MERGED as e7659517)** —
  `GpuRenderer` had successful no-op defaults (`init_light_culling`,
  `init_shadow_resources`, `init_pass_pipeline` returned `Ok(())`; nine more
  methods were silent no-ops) plus guidance recommending new ones. Fix: new
  `RendererFeature` capability vocabulary (`renderer/features.rs`) with a
  required default-free `supports_feature` query; 9 required ops lost their
  defaults (Vulkan gained 3 explicit documented no-ops; `AnyRenderer` gained
  a real `set_viewport_bindless_slot` dispatch — it had inherited the no-op);
  6 optional ops fail with `UnsupportedFeature` before mutating state
  (3 aligned from `InvalidOperation`); timestamp hooks keep documented
  no-op-when-unsupported semantics. Vulkan reports all but `DirectUiPass`,
  Metal reports all; app call sites untouched (all handle `Err` generically).
  Mock-backend contract test `katla_gfx/tests/renderer_features.rs` (5 tests;
  sabotage run FAILED as required against restored defaults, passes with the
  fix). Verified: workspace check, 380 lib tests, clippy clean on touched
  files, fmt on touched files only. Exclusions: app capability-branching
  (#92/#93), `RenderGraphBackend`'s 2 empty defaults (noted for #93),
  `katla_gfx/AGENTS.md` text needs maintainer approval (protected file).

- **Issue #87: geometry instancing allocates and encodes every submitted
  instance (2026-09-09, branch fix/87-instanced-draw-allocation)** —
  Instanced draws uploaded only `instances.first()` and every Vulkan/Metal
  draw site hardcoded instanceCount=1, so instances 1..n never reached the
  GPU; callers chose raw storage indices (default slot 0) and could silently
  overwrite each other. Fix: `DrawList` owns frame-local slot allocation —
  `push` assigns a unique base range and returns it; `from_draws` preserves
  slots in filtered/merged lists (shadow/outline clones, Metal upload merge);
  `with_instance_index` deleted, `instance_index` pub(crate) +
  `base_object_slot()`; FrameContext counter and gizmo/physics/reverb manual
  `next_instance_index` threading removed (dead `generate_raycast_vis` +
  orphaned ray colors deleted too); upload loops write every instance with
  whole-range capacity validation (typed ObjectLimitExceeded); all 3 Vulkan +
  5 Metal encode sites pass the real instance count (Vulkan firstInstance =
  base slot; Metal keeps its buffer-offset rebind so instance_id walks the
  uploaded range). Entity→slot picking map now built from push/submit return
  values. Focused GPU test `katla_gfx/tests/instanced_draws.rs` (#[ignore]):
  4-instance draw byte-identical to 4 direct draws, mixed list, frame-slot
  reuse, late-instance recolor, capacity exhaustion; mutation-verified to
  fail against both original bugs. Verified on Intel Vulkan: interaction
  harness 8/8 (incl. viewport pick through the reworked picking map),
  legacy ui-test 5 states + screenshots healthy, windowed `katla -s` exits 0,
  workspace tests green (known katla_audio parallel-load flake only),
  CI-style clippy clean, fmt clean. GOTCHA: Vulkan headless readback row 0 is
  NDC y=+1 (y down) — pixel probes use row=(ndc_y+1)/2*H. Metal compile
  covered by CI macOS 26.

- **Issue #94: transactional leak-free Vulkan resource construction
  (2026-09-07, merged 2026-09-08 as cc31089a via PR #102)** —
  Fallible multi-step constructors leaked already-created objects: buffer
  leaked on allocation failure, buffer+allocation on bind failure
  (allocate_buffer/create_image), and GlobalParticleBuffer::new leaked every
  prior buffer on mid-sequence failure including its late alignment-validation
  return. GpuAllocator::free silently abandoned allocations on borrow
  conflict. Fixes: OwnedBuffer/OwnedImage RAII guards (commit-on-success);
  allocate_buffer_named/create_image_named transactional helpers; all direct
  create/allocate/bind call sites (gpu_buffer.rs, animation/pose_compute.rs,
  particles/buffer.rs incl. its new PartialParticleBuffers whole-function
  guard, particles/descriptors.rs, particles/debug_readback.rs) routed through
  them; BufferObject::resize replacement-first; borrow-conflict frees queued
  and drained deterministically; debug_allocation_stats() (live, pending)
  accounting plus test-only inject_allocation_failures hook; 6 injected-
  failure GPU tests in memory.rs `#[cfg(test)]` (#[ignore] device tests).
  Verified on Intel Vulkan: 6/6 injection tests, 2,032 workspace tests green,
  particle stress/preset suites green (they exercise the rewritten paths on
  device), CI-style clippy clean, fmt clean, headless scene render intact,
  interaction harness 8/8. Pre-existing on main: particle_validation example
  broken under `--features validation`; scene-disk test flaky under parallel
  load. Gotcha: `[T; N]::map` consumes the array — destructure into lets when
  splitting (Buffer, Allocation) pairs.

- **Issue #85: mesh index format preserved through upload/storage/draw
  (2026-09-07, commit 37182eca)** —
  The Vulkan mesh draw paths hardcoded `vk::IndexType::UINT32` while
  `create_mesh` accepted any Pod index width, so a u16 mesh rendered garbage.
  Fix: `MeshIndexElement` trait (u16/u32 only, compile-time rejection of other
  element types replacing the size_of guess), `MeshAsset.index_format`
  (backend-neutral, from `backend::command::IndexType`, now root-exported),
  all three Vulkan draw sites bind the recorded format (draw_calls,
  draw_helpers, parallel_geometry's ResolvedDrawCommand carries
  index_type), Metal's upload conversion keyed off the typed format
  (MetalMesh stores u32-by-conversion so its Uint32 binds stay correct),
  `GpuRenderer::mesh_index_format` diagnostics accessor, dead
  `VulkanRenderer::register_mesh` (pre-built-buffer path, zero callers)
  removed end-to-end. Focused GPU test
  `katla_gfx/tests/mesh_index_format.rs` (#[ignore = "requires a Vulkan
  device"]): u16 and u32 triangles render byte-identically across 4 frames
  with destroy/recreate interleaved; run with
  `TMPDIR=$HOME/tmp cargo test -p katla_gfx --test mesh_index_format -- --ignored`.
  Verified the test FAILS against the old hardcoded binding (u16 mesh draws
  nothing). App-level regression: default-scene headless screenshot intact,
  interaction harness 8/8 + screenshots, gfx 375 lib tests + workspace suites
  green (only the known katla_audio timing failure), CI-style clippy clean,
  fmt clean. Bare-init GPU test gotchas recorded in activeContext (shadow
  resources before PBR compile; per-frame uniforms/object-data writes).
  Metal runtime rendering remains unverified on Linux — CI macOS 26 covers
  compile + unit tests.

- **Inspector component listing + add/remove from the UI (2026-09-08, merged
  2026-09-09 via PR #103)** —
  The inspector now lists every component on the selected entity as a
  collapsible section in a canonical `SECTION_TYPES` order (17 slots reserved
  unconditionally per the state-slot convention). Registry-removable components
  get a header `×` wired through `InspectorAction::Remove` →
  `EditorAction::RemoveComponent` → `SceneOp::RemoveComponent` (undo-grouped,
  protected-entity-checked on agent/MCP paths). The Add Component picker was
  dead code (view read a never-written local slot); it now opens from the env
  flag, shows a live filter textfield (view-local String slot), excludes
  components the entity already has, and sorts rows alphabetically — with
  hover-selectable full-width rows (selectable + padded hstack like the
  hierarchy). Three real bugs fixed: (1) picker could never open (state-slot
  vs env mixup); (2) UI AddComponent ran the agent protected-entity guard —
  `gizmo_state.entity` tracks the CURRENTLY SELECTED entity, so every add was
  rejected as "editor gizmo"; guard removed from the UI path (agent/MCP keep
  it); (3) particle emitter payload was hard-`None`d (d3665768) — restored.
  `collect_entity_info` names now match the registry exactly
  ("NameComponent", "ParticleEmitterComponent") and detect
  VelocityComponent/ReverbZone/CollisionFilter; `ComponentRegistry::type_names()`
  sorts because HashMap order randomized the picker between runs.
  Verified on Intel Vulkan headless: interaction harness 8/8 (hierarchy click,
  viewport pick, empty deselect, light+dark themes, modal close, add Collider
  via picker row, remove Collider via section ×) + screenshots 01-13, legacy
  `--ui-test` 5 states intact, workspace tests green, clippy clean on touched
  files, fmt clean. Environmental notes: /tmp tmpfs quota breaks doctest
  linking (use `TMPDIR=<home>`); `katla_audio::test_engine_playback_lifecycle`
  is flaky on this machine (stop/state timing race; passed 2026-09-08),
  pre-existing.

- **Headless interaction harness + 3 input/picking bugs fixed (2026-09-06, commits e6211bd2 + 50cee9f4)** —
  Built `--interaction-test DIR` (katla_app/src/application/interaction_test.rs):
  a state machine that injects synthetic mouse input at headless-frame boundaries —
  `ui_context.input_mut()` for UI clicks/wheel (press and release on separate
  frames), `app.on_mouse_input` for the editor press path (focused panel, gizmo
  hit test, `pending_pick`). Runs the full real pipeline: hit-test → widget
  callbacks → actions → process_editor_actions, plus GPU picking readback.
  Reports PASS/FAIL checks + screenshots (6/6 PASS: hierarchy row click selects
  Sphere_1_0, viewport click GPU-picks LimeTorus, empty click deselects, Light
  theme applies, Dark restores, modal close works; scroll both directions and
  hierarchy auto-reveal judged from screenshots).
  1. **First viewport click never picked**: `is_click_on_floating_panel`
   (editor_ui/mod.rs) tested the centered Preferences modal rect without
   checking visibility — with the modal closed, clicks in the viewport center
   hit the phantom rect, `update_focused_panel_from_click` bailed, the pick
   gate saw stale `FocusedPanel`, and `pending_pick` was never set (users had
   to click twice). Fixed with a `preferences_panel.is_visible()` guard;
   added `preferences_panel_visible()` accessor.
  2. **Invalid picking readback (Vulkan validation errors)**: render-graph
   color-attachment transients lacked `TRANSFER_SRC` usage, so
   `vkCmdCopyImageToBuffer`/layout transitions on the object-ID image were
   illegal. Color transients now include `TRANSFER_SRC` (vulkan_backend.rs),
   matching the swapchain image usage precedent.
  3. **Wheel scroll dead over list rows**: widget dispatch stopped at
   `InputResult::Ignore`, and Selectable rows return Ignore when not clicked,
   so the wheel never reached ancestor ScrollViews (hierarchy/inspector/asset
   browser unscrollable over content). In katla_ui input.rs, wheel-only events
   (scroll_delta != 0) now treat Ignore as bubble-to-parent; consumers
   (ScrollView, code editor) still stop propagation.
  UX: hierarchy auto-reveals the selected entity when selection changes
  outside the panel (viewport pick) — scroll offset moves the row into view,
  tracked via a `last_selected_id` state slot in HierarchyView::build. Headless
  builds now install the console logger via shared
  `ApplicationBuilder::install_console_logger` (console panel shows live log
  entries in captures; the plain env_logger init previously claimed the global
  logger slot first).
  Verified on Intel Vulkan: 6/6 interaction checks, legacy `--ui-test` 5
  states intact, 50 workspace test suites green (`katla_audio
  ::test_engine_playback_lifecycle` flakes only under parallel load), strict
  workspace clippy clean, fmt clean, windowed `katla -s` exits 0. Note:
  viewport pick coordinates must avoid the selected entity's gizmo (12px
  screen-space axis threshold); the harness targets the LimeTorus for this
  reason.

- **Metal UI + shadow fixes (2026-09-05, macOS)** —
  1. **UI fully broken on Metal**: ui.wgsl reads `texture_index` per-vertex
   (location 3, offset 20, 24-byte stride) since 4afa063b, but
   `ui_vertex_descriptor()` (katla_gfx/src/metal/context.rs) still bound only
   3 attributes, so pipeline creation failed ("Vertex attribute
   texture_index(3) is missing"), every frame logged "Metal UI record has no
   material", and the canvas rendered nearly blank (17KB screenshot). Fixed
   by adding the UInt attribute at offset 20.
  2. **"Inverted" shadows on Metal**: the atlas quadrant layout and the
   shared sampler assume Vulkan's Y-down clip convention; Metal's Y-up clip
   space mirrored each cascade's content inside its quadrant, so shadows
   landed displaced/mirrored and appeared to move wrongly with the camera.
   Diagnostic trail: multi-angle headless captures (new `--camera
   yaw,pitch,distance` game flag + `Application::set_editor_camera_pose`),
   then a temporary red shadow-mask probe in model_pbr.wgsl (reverted) that
   showed torus-ring artifacts under unrelated geometry. Fix:
   `flip_projection_y` (katla_gfx/src/shadow/cascade.rs, unit-tested) builds
   a Metal-only encode buffer (`shadow_cascade_encode_buffer`) with mirrored
   clip-Y; shadow pipelines switch to `MTLWinding::CounterClockwise` to
   compensate the winding flip; the sampler keeps the shared buffer.
   Verified: red probe shows masks under every caster; clean captures at
   default/90°/180°/top-down plus playground scene show short coherent
   shadows matching the 70° sun; workspace tests green (2,0xx pass), strict
   workspace clippy clean (also fixed two pre-existing macOS-only warnings),
   fmt clean, `METAL_DEVICE_WRAPPER_TYPE=1 katla -s` exits 0.
  3. macOS-only clippy: `encode_cascade_draws` got the same
   too-many-arguments allow as `render_cascades`; the macOS
   `collect_and_upload_lights` now reuses `point_lights_buffer` (the
   non-macOS path already did).

- **Windowed Vulkan limited-frame validation (2026-09-05, working tree)** —
  fixed Wayland's unspecified surface extent being passed as an image size,
  stale signaled semaphores surviving swapchain recreation, and a shutdown
  segfault caused by destroying the surface after Wayland teardown. Actual
  window dimensions now reach initialization and resize; surface release
  is explicit and idempotent. The extent-selection regression test and
  strict workspace Clippy passed. The windowed GPU-assisted validation run
  rendered 100 frames and exited with status zero and no Vulkan validation
  errors. Khronos layers are not installed system-wide: these checks use
  the temporary extracted layer package, not the normal loader search path.

- **Vulkan headless rendering and visual audit (2026-09-05, working tree)** —
  Linux now renders the normal scene/editor graph into offscreen targets and
  saves PNGs without a window or display server. Fixed validation-layer
  fallback startup, fence reuse, readback synchronization, missing UI
  pipelines and texture selection, imported mesh attributes, HDR pipeline
  formats, panel-sized scene targets, and stale shadow descriptors. Restored
  particle, animation, and lighting work that graph liveness had culled.
  Cascaded shadows use one atlas pass with independent cascade parameters;
  the unsafe parallel shadow recorder was removed. The sky gradient is
  continuous and reconstructs rays from rasterized NDC. Hierarchy selection
  and inspector sections fill their panels, Preferences scale controls fit
  at full height, and unit gizmo meshes render solid shafts at a shared,
  smaller draw/hit-test size.
  Verified on Intel Vulkan: 2,023 workspace tests passed (116 ignored),
  explicit GPU pixel regression passed, strict workspace Clippy and format
  checks passed. Default and playground scenes plus all five UI test states
  were captured and viewed; final runs with DISPLAY/WAYLAND_DISPLAY removed
  produced no Vulkan validation errors. Validation-layer absence also falls
  back successfully. Captures: `.zcode/vulkan-captures/`; reproduction and
  opt-in GPU test commands are in README. Native Metal remains unverified
  on this Linux machine.

- **Editor UI/UX polish pass (2026-08-31)** — dock tabs
  left-stacked at a 160px cap (shared draw/hit-test `tab_hit_width`); bottom
  dock default 0.64→0.74; gizmo toolbar icon-only with accent selected state
  (`UiStyle.accent` added, `ToolLabelButton` deleted); Inspector "+ Add
  Component" wired through `EditorAction::AddComponent` →
  `SceneToolExecutor` + protected-entity guard; ScrollView fix trilogy —
  `min_size.height=0` so it shrinks inside panels, 3px scrollbar thumb, and
  taffy `content_size` exposed as unclamped child bounds for
  `draw_after_children`/`post_layout` (wheel scrolling previously always
  clamped back to 0); scrollbar handle color derived from `text_muted`
  (macro themes defaulted it to black). Judge-passed over 5 ui-test states;
  workspace tests/clippy/fmt green.

- **Headless render visual audit — 3 renderer bugs fixed (2026-08-31)** — systematic screenshot pass (default scene 100 frames,
  playground, 5 ui-test states, 2560×1440 Metal) plus shader-as-instrument
  probes (solid-green billboards, red sky, target-pixel stripes) root-caused:
  1. **Billboard scale dead** — `billboard.wgsl`/`billboard_depth.wgsl` VS
     ignored the model matrix scale, so every gizmo was a fixed 1×1 world
     unit (FillLight bulb rendered ~90px; size didn't respond to 10× desired
     size — the decisive probe). Fix: derive scale from
     `length(model[0].xyz)/length(model[1].xyz)` in both shaders.
  2. **Fire icon canvas overflow** — ForkAwesome fire glyph at 64px exceeds
     the canvas; clipped edge rows bled a bright bar through the FS alpha
     threshold. Rasterizer now scales glyphs to fit (6% margin).
  3. **Sky orientation investigation** — the earlier Metal-only clip-Y fix
     was superseded by the 2026-09-05 Vulkan audit. Sky reconstruction now
     uses the rasterized NDC, and the actual discontinuity between horizon
     and ground colors has been removed. Native Metal verification remains
     necessary after this shared shader change.
  4. **Gizmo shadow slivers** — with an entity selected, the move gizmo's
     arrows/cones cast flat shadow-map streaks (only billboards were filtered
     from `shadow_draw_list`). Shadow list now filters ALL editor-overlay
     materials (billboard, gizmo, physics-debug — reverb debug shares the
     physics-debug material).
  Non-bugs confirmed during audit: the "orange plate" behind one bulb is the
  scene's own CenterCube lit by WarmLight; the white underline under flame
  icons is part of the ForkAwesome fire glyph; "bulb-shaped shadows" were
  cube/sphere shadows offset by light_direction (0.3, 1.0, 0.2) — billboard
  shadow exclusion works. Verified: uniform 40px gizmos, no band, clean
  ground, workspace tests + clippy + fmt green. Note: `compute_gizmo_scale`'s
  vp_h=391 (logical panel height) is correct, not stale.

- **Preferences modal redesign + UI primitives pass (2026-08-31)**
  — Preferences is now a centered `Modal` (560×520, scrim 0.6, Escape /
  outside-click / 28px close button, focus-trapped) instead of a draggable
  panel; categories moved to an icon sidebar (Appearance/Viewport/Audio/AI —
  General deleted, category index lives in
  `EditorUI.preferences_category` via `PreferencesAction::SetCategory`); the
  15-miniature-editor-preview theme grid became a compact two-column list
  with color-swatch rows (`theme_preview.rs` → `theme_swatch.rs`); font scale
  became discrete 80–130% segmented options over the same persisted f32.
  Primitives: `Widget::press_action` gives Enter/Space activation to
  Button/Selectable/ImageButton/ToolButton (text inputs return None);
  `Modal::wants_global_input` when open so Escape works wherever the mouse
  is; Modal draws its own title bar/scrim/two-layer shadow (tokens
  `MODAL_TITLE_HEIGHT`, `RADIUS_WINDOW`, `MODAL_CLOSE_SIZE`); `LabeledSlider`
  gained `value_display(multiplier, suffix)` (volumes now show %) plus hover
  emphasis and a focus ring; status-bar telemetry is all-muted Small with no
  pipe separators. Deleted the orphaned `TabBar` widget end-to-end. All
  preferences state slots are reserved unconditionally in a fixed order at
  the top of `PreferencesView::build` (cross-tab type-confusion guard).
  Verified: workspace tests green (254 app / 635 ui), clippy clean,
  headless ui-test 5 states re-rendered and judge-passed.

- **Particles render on Metal (2026-08-30, commit 970e07d9)** — the Metal
  particle subsystem was fully built but orphaned: every API `#[cfg(test)]`,
  the plan compiler rejected `PassKind::Particles`, nothing drove it. Now:
  subsystem un-gated (render pipeline created in `init_particle_system` via
  the `alpha_blended` variant — empty vertex descriptor, the default PBR one
  makes validation demand buffer 10); two new shader profiles (`ParticleRender`,
  `ParticleCompute`) with explicit naga binding maps; `encode_compute` runs
  emit/simulate/draw-command inline in `MetalRenderer::render()` before passes
  (light-culling pattern); `encode_particle_record` renders indirect
  camera-facing billboards onto hdr_color (Load) with depth test/no-write;
  `ParticleEmitterDriver` trait unifies VK/Metal emitter sync (ECS
  `ParticleSystem::update` now takes the trait object; headless path covered
  because `step_particle_simulation` syncs emitters before stepping — headless
  never runs frame_loop). Deleted the test-gated `dispatch_compute`: it sized
  the emit dispatch from a pre-emit GPU readback, which reads the PREVIOUS
  frame's alive count and silently drops emissions. Verified: 511/251 tests,
  clippy clean, Metal validation clean, orange fountain + white sparks visible
  headless, static probes 8/8 byte-identical vs pre-change capture.

## Completed Recently (prior)

- **Application-owned frame graphs (#60, #61, #62)** — applications can provide a one-shot graph factory and runtime policy; Katla's editor renderer is an explicit preset rather than an engine invariant. Empty, UI-only, geometry-only, and custom graphs are supported.
- **Optional Metal topology (#49)** — removed the requirement that every Metal frame contain scene depth, geometry, tonemapping, and UI. Absent passes no longer encode hidden work or wait on unsignaled fences.
- **Structured Metal execution failures** — terminal command-buffer failures now surface backend, label, status, native code/domain, and localized description through `RendererError`.
- **Compiled Metal pass stream (#56 slice)** — deleted the Metal-only singleton schedule and fixed semantic rank table. Metal consumes the render compiler's canonical order with exact `PassId`, pass-local draw/UI submissions, repeated semantic categories, and deterministic traces.
- **Explicit object-ID pass** — GPU picking is a real render-graph pass on Metal and Vulkan instead of invisible geometry-pass work. Vulkan reuses the shared skinned/billboard draw path.
- **Render-graph pass culling (#34)** — exported resources and side-effect passes are liveness roots; true producer dependencies retain required predecessors; execution, parallel groups, material work, and submissions are live-only; loaded/blended targets declare read-before-write dependencies; diagnostics expose declared/live/culled state.
- **Repository hygiene** — temporary source-export and patch-materialization pull requests were closed without merging; product changes are rebuilt as clean commits directly above `main` before canonical CI.

## In Progress

- **Finish #56** by carrying graph-declared attachments, load/store/clear state, viewport/scissor, resource bindings, and generic executable payloads into backend execution records.
- Resolve Metal targets from graph resource handles instead of backend-owned editor fields.
- Remove temporary shadow/depth side-effect roots after those outputs become graph-owned.
- Complete custom graphics and backend-neutral compute execution contracts.

## Verified Baseline

- Canonical Linux graphics CI passes formatting, `cargo check`, graphics tests, and strict graphics Clippy.
- Canonical macOS 26/Metal CI passed the compiled Metal pass-stream merge.
- Isolated pass-culling validation passed graphics and application checks/tests, graphics strict Clippy, and focused application Clippy.
- `katla_app` library validation: 251 passed, 2 ignored.

## Known Follow-Up Work

### Metal headless band-collapse ROOT-CAUSED & FIXED (2026-08-26)

The scene collapsed to a top strip (~441px of 993) and spheres/sky vanished. Not a
Metal/GPU bug at all: katla_ui layout collapse from the `selectable()` wrapper added
around the viewport cell (9f0b714f). Chain measured via layout dump:
Selectable/ZStack resolved to height **0** (`Percent(1.0)` against auto-height parent),
then `Alignment::Center` centred the full-cell image at y=−248 → most of the quad sat
above screen top. What read as "sky gradient" was tonemapped ground plane squeezed
into the surviving sliver. Fixed by giving the zstack definite dimensions
(`flex_width/flex_height` from cell size) in
`katla_app/src/ui/editor_ui/declarative/viewport_grid.rs`; layout dump shows Image at
(0,0) filling the cell, headless render back to ~490KB with sphere grid visible.

Diagnostics worth keeping: `--dump-layout` (game binary) prints the laid-out widget
tree with bounds — pinpoints layout collapses without screenshots. Normal-colour
fs_main debug output distinguishes "which geometry survived". Run-to-run pixel diffs
(headless captures are byte-deterministic) rule out flakiness cheaply.

### UI alignment conventions (Aug 26 2026)

Two root causes found behind "misaligned text" reports; both fixed at source:

- Empty strings measured `(0,0)` height, so any widget centring text by
  `center().y - h/2` drew placeholders half a line low (hierarchy filter). Empty now
  measures width 0 × line height (`size * 1.2`, matching shape_text buffers).
- `Alignment` had no cross-axis-only variant. Default Leading leaves mixed-height
  children top-stuck (labels vs fields vs buttons); Center also horizontally packs
  content off the leading edge. Added `Alignment::Middle`: cross axis centred, main
  axis untouched. Use it for every toolbar/row mixing heights.

### Billboard plates root cause (fixed 2026-08-26, commit 48a66fe2)

Opaque squares behind editor billboard icons were not a blending or texture bug —
alpha, blending (SourceAlpha/OneMinus) and the icon rasteriser were all correct. The
billboard **depth prepass pipeline was vertex-only** (`["vs_main"]`, fragment None),
so its discard never ran: full quads wrote prepass depth, the main pass then
discarded transparent pixels, and everything behind each quad stayed occluded
(sky-grey plates). Fix: compile+attach the `billboard_depth.wgsl` fragment and bind
argument buffer + shared sampler for the billboard variant in `render_depth_prepass`.
Note for future pass pipelines: a depth-only prepass pipeline that must respect
alpha needs a real fragment stage — "depth written by rasterizer, no fragment" is
only valid for fully opaque geometry. Diagnostic pattern that nailed it: patch the
colour shader to *visualise the sampled value* (alpha as red) — distinguishes
"texture wrong" from "draw wrong" in one screenshot.

### Two render-graph contract tests born failing since 2026-07-29 (#82, open)

`copies_graph_resource_and_attachment_contracts` (execution_plan.rs) and
`diagnostics_expose_stable_physical_allocation_ids_and_memory_totals`
(diagnostics.rs) fail deterministically on every CI run since their
introducing/semantics-shifting PRs (#77 a932bc5a / #80 47336aa5). ReadWrite
access now copies into both reads+writes; physical_allocation_id comes back
None where Some(0) is asserted. Full forensics in #82. Files sit inside the
collaborator WIP area — coordinate before touching fixtures.

- Transient resource lifetime analysis and aliasing (#35).
- Real Metal frames in flight and synchronization cleanup (#36).
- Further deterministic graph diagnostics and capture tooling (#37).
- Complete graph-owned attachment execution and generic handlers before closing #56.

## Architecture Direction

- The application owns topology.
- The render graph owns validation, liveness, stable identity, ordering, and diagnostics.
- Backends own native realization and encoding.
- Editor conventions remain presets, never universal engine laws.
- Unsupported work must fail structurally before native command-buffer creation.

### Shadow pipeline root causes (Aug 28 2026, Metal + shared)

Shadows were dead on BOTH backends. Root causes found by shader-as-instrument
debugging (num_cascades → in_bounds → stored-depth visualisation, plus unit
probes with real captured view/proj matrices):

1. **Wrong inverse order** (`shadow/cascade.rs`): `view_proj_inv = P^-1 * V^-1`
   instead of `V^-1 * P^-1`. Frustum-slice un-projection collapsed every cascade
   AABB to ~0.2mm around origin -> degenerate ortho -> everything out of bounds
   -> sample_shadow returned 1.0 everywhere. ONE-LINE fix, fixed both backends.
2. **`apply_pancake` destroyed the z extent**: `pancake_offset = mins.z - 1` made
   the far plane `mins.z + 1` algebraically — only a 1-unit-deep slab survived;
   cascades 1-3 clipped all scene geometry (ndc_z up to 1.7). Fixed to near =
   mins.z - 1 (1 unit slack), far = maxs.z (real extent). Signature changed to
   take maxs; test updated.
3. **Metal atlas quadrant layout**: per-cascade viewport+scissor matching
   cascade_uv_offset_scale (row = 1 - i/2, Metal y-down agrees with shader).
4. **Metal cascade data hand-rolled**: rewired onto shared CascadeShadowMap
   (~400 lines deleted); splits raw view-space z, texel_size populated.
5. **Per-cascade render passes cleared the whole atlas**: each cascade began
   its own render pass with a full-attachment clear, wiping every previously
   rendered quadrant. All cascades now encode in ONE render pass, cycling
   viewport/scissor/push-constants per cascade (render_cascades).
6. **Depth convention mismatch**: mat4_ortho was GL [-1,1] NDC z while Metal/
   Vulkan clip and store [0,1] (near half of every cascade clipped). Now a
   zero-to-one ortho; light-view z runs positive toward the light, so depth 0
   is nearest the light, 1 farthest (matches LessEqual + front-face culling).
   Shader reference depth is raw proj.z (the old *0.5+0.5 remap was GL-only).
7. **Constant bias units**: depth_bias_constant was 1.5 raw depth units on a
   [0,1] range (compare always passed -> everything lit). Now converted to a
   depth fraction (texels / atlas size) at upload in gpu_data().

RESULT: cast shadows verified on Metal headless — sphere grid, boxes, cylinder,
fox all shadow the ground; PCF edges soft, no acne (pixel-probed + vision
confirmed). Vulkan inherits the same fixes via the shared code.

8. **Skinned shadow MSL collision (fixed, commit 24a872c2)**: shadow_depth_
   skinned.wgsl declares joint_matrices at group3:0, which the shared graphics
   binding map sends to buffer 3 — colliding with shadow_params (Vulkan works
   because it binds the skeleton descriptor set at set 3). Added
   ShaderProfile::ShadowSkinned with its own naga MSL binding map (joints to
   buffer 4, free in the depth-only shader). render_cascades now takes the
   skinned pipeline + skeleton storage, binds skeleton buffer 4 per skinned
   draw, and restores the regular pipeline per cascade. Note: the skinned
   pipeline previously overwrote the regular shadow pipeline slot; it now has
   its own slot (shadow_pipeline_skinned).

RESULT 2: fox casts a quadruped-shaped shadow on Metal (pixel-probed +
screenshot-verified). CI fully green (tests + lint + fmt both platforms).

9. **Batched Metal texture uploads (commit 89dc6bba)**: initial texture data no
   longer lives in shared textures written at creation time. TextureUploadQueue
   copies bytes into pooled shared staging buffers, one blit pass at frame start
   feeds all pending uploads, slots recycle after the consuming submission
   completes. copy_buffer_to_texture now derives bytesPerRow/BytesPerImage from
   the format (was hardcoded 0 — invalid for anything taller than one pixel).
   Uploads validate format/extents/pitch with typed errors naming the row pitch.
   COPY_DST now maps to MTLTextureUsage::ShaderWrite.

RESULT 3: headless render is pixel-identical on every static probe; the only
cross-binary diffs are the wall-clock fire flicker phase. Known follow-up:
flipping data textures to MTLStorageMode::Private changes rendered output
(midtones/penumbra shift) despite byte-identical texture content — needs a GPU
capture to root-cause before private storage lands.

10. Private-storage anomaly elimination sweep (f35f6971, 7fa79fbd)
   Extended probes to the bindless argument-buffer path:
   test_bindless_argument_buffer_storage_probe renders through the real
   MetalBindlessTextureManager arg buffer (slot 9, ShaderProfile::Graphics)
   for SHARED vs PRIVATE staged-blit textures — byte-identical AND
   non-vacuous (vacuity guard asserts real sampled content).
   Also declared bindless texture residency in the geometry pass (7fa79fbd)
   — correct Metal practice; zero pixel change (Apple Silicon implicit
   residency). Eliminated: content, timing, usage, residency, direct
   sampling, argument-buffer sampling, mipmaps (mipLevelCount 1). The
   darkening only manifests in the full app render — remaining suspects are
   app-scale (descriptor state, HDR targets, MSAA, tonemap chain). Needs an
   Xcode GPU capture; probe chain documented in the skill corpus.

11. #82 closed; #57 first slice (c2e2f7ab)
   #82 (red CI since Jul 29) was fixed by 6322f156's test-fixture updates;
   closed with evidence. #57: MetalSurface lost its blanket unsafe
   Send/Sync (AppKit-affine layer state; nothing moved it across threads);
   const compile-time guard fails the build if re-added. Other four blanket
   impls (context/command buffer/buffers/encoders) remain, each needing its
   own documented-invariant pass.

12. #53 core: persistent Metal pipeline archive (97c7480a)
   MetalPipelineArchive owns an MTLBinaryArchive + JSON metadata sidecar at
   ~/Library/Caches/dev.ravboet.katla/pipelines/ (KATLA_PIPELINE_CACHE_DIR
   overrides). Sidecar key = schema version, OS version, GPU registry ID,
   Apple7 family, engine version; any mismatch deletes and rebuilds. Corrupt
   archives rejected + rebuilt, never fatal. Render descriptors consult the
   archive (setBinaryArchives); all created render and compute pipelines
   register back. Atomic flush (temp + rename), no-op when empty. 4 tests
   (flush/corrupt/mismatch/device metadata), 494/494, clippy clean, headless
   8/8 probes, second run reuses archive. Remaining: explicit key
   layer, async warming, structured diagnostics, benchmark.
13. #53 fix: cache staleness now an explicit loaded_from_disk flag (3f6a678f)
   The mismatch test asserted rebuilt-bytes != original-bytes — false on a
   same-machine recompile (identical bytes) and runner-nondeterministic;
   CI on 97c7480a/439c0a73 was red, not green as first reported. The
   mismatch branch also fell into the Ok arm that assumed disk load. Fix:
   flag set from whether the open actually used the cached URL; tests
   assert the flag; open/flush logs name cache state. CI 33233307369
   (both jobs) green on 3f6a678f. Correction posted on #53.
14. #51: error-path test coverage landed (6df122af)
   validate_frame_submissions extracted as a pure fn (plan/pending/has_depth)
   from render_frame; 5 contract tests (unknown pass index, UI multi-list,
   single list accepted, depth required, UI-only exempt). render_frame drops
   the drawable on validation failure (no partial-frame present). Filtered
   staging on shared frame_render.rs (hunks 1/2/4 are collaborator's:
   MTLTexture import, CANVAS_CLEAR_COLOR, HDR viewport). 499/499, staged-tree
   clippy clean, CI headSha-verified green. Remaining on #51: explicit
   declared-graph-output DoD item is judgment-call territory (plan compiles
   from the graph itself); leave open or close with the evidence comment.
15. #52 slice 1: encoder execution diagnostics (e784eef3)
   GpuDiagnosticsMode (Release/Validation) → MTLCommandBufferDescriptor +
   EncoderExecutionStatus error option; mode from ValidationMode at init.
   Structured GpuCommandBufferDiagnostics (label/code/domain/description +
   per-encoder label/state/signposts from MTLCommandBufferEncoderInfoErrorKey),
   deterministic render(), first-faulted-encoder log. 6 tests incl. 2 GPU
   smoke (validation + release buffers complete on device). 505/505, 8/8
   probes vs pre-change baseline, CI SHA-verified. Still open: per-encoder
   labels (backend trait ripple into WIP files — separate slice), frame-
   indexed cmd-buffer labels, attach diagnostics to RendererError.
16. #52 slice 2: deterministic encoder labels (3d3139b9)
   RenderPassInfo.debug_label (const &'static str); all production render
   passes labeled (depth_prepass, shadow_cascade, canvas_clear, geometry,
   geometry_hdr, present, picking, picking_readback, outline). New trait
   methods begin_compute_pass_with_label / begin_blit_pass_with_label;
   migrated texture_upload (7 sites), light_culling, skinning, frame-prepass
   blit. Labeled-encoder smoke test; 506/506; 8/8 probes unchanged; CI
   SHA-verified. Remaining on #52: frame-indexed cmd-buffer labels, attach
   diagnostics to RendererError.
17. #52 CLOSED (b589b79b): diagnostics attached to RendererError +
   frame-indexed labels. GpuExecutionFailure.encoders
   (Vec<GpuEncoderDiagnostic>) populated in wait_for_frame from
   EncoderInfoErrorKey; Display lists encoders in order + signposts;
   is_faulted() predicate. Labels: render_graph_frame.<frame>,
   shadow_pass.<frame>, depth_prepass.<frame> (picking readback stays base —
   no frame index in free fn). 509/509 (+3); 8/8 probes; CI SHA-verified.
   NOTE: two silent lost-edit incidents this round (diagnostics.rs struct/
   impl derive ordering mangled, test append reverted) — always re-grep the
   FILE ON DISK after multi-edit scripts, don't trust script stdout.
18. #57 slice 2 (2a505a86 + 44f5f74e): deleted unsound/needless Send/
   Sync markers (LightCullingBuffers held Rc — markers were a lie;
   encoders honestly !Send now; MetalEvent empty), documented MetalContext +
   pipeline SAFETY invariants, removed orphan depth_stencil.rs (never a
   module), 14-assert const affinity contract in surface.rs tests. Both
   commits CI green (2a505a86 run 33237746522; 44f5f74e run was CANCELLED by
   concurrency group — gh run rerun 33237633701 then green). LESSON: check
   `git status` count after commit — blit_encoder.rs missed its `git add`
   and silently stayed unstaged. Remaining #57 DoD: executor/token model,
   completion-handler audit, stress test.
19. #57 slice 3 (8e7cec9e, CI green): completion-handler audit done — the
   only handler is the failure logger; captures nothing, logs only, no
   non-Send transfer; picking readback is synchronous. FIXED per-frame
   RcBlock leak: submit now registers ONE process-lifetime capture-free
   block via OnceLock<SharedBlock> (Send/Sync documented on Block ABI).
   Surface thread-affinity model documented in surface.rs module docs.
   Remaining #57 DoD: executor/token type, TSan stress test.
20. #53 observability slice (82375a02, CI green): ArchiveRejection enum
   (Absent/MetadataMismatch/Corrupt) + PipelineCacheStats snapshot
   (opened_from_disk, rejection, pipelines_registered, open/flush durations)
   in pipeline_archive.rs; structured log lines at context startup + renderer
   Drop flush; loaded_from_disk bool removed (tests migrated); +2 tests =
   511/511. Verified live: "opened_from_disk=true, rejection=None,
   open_ms=1". #53 remaining: async warming, off-thread hot reload,
   cold/warm/hot-reload benchmark. #55 is GATED on #54 (Metal 4 rewrite —
   needs Micke's sequencing call; objc2-metal 0.3.2 has MTL4 headers but
   device support unverified).
21. Headless artifact hunt (landed: 59cce2a3 billboard exclusion,
   5397ecf6 TAB_BAR_HEIGHT viewport fix; pixel-verified via later
   Metal/Vulkan capture passes):
   (a) shadow-map billboard slivers — root cause: encode_cascade_draws drew
   billboard gizmos flat via the non-billboard shadow VS; fix = is_billboard
   skip in metal/shadow.rs + exclude_billboards:DrawParams field (true in
   vulkan shadow_pass, false elsewhere). (b) pale band at viewport top —
   root cause: last_viewport_bounds includes the tab-bar strip while the UI
   image cell excludes it → texture squeezed, sun-washed sub-tab rows
   exposed; fix = layout.rs subtracts TAB_BAR_HEIGHT for viewport bounds.
   511/511, staged clippy clean. (The later blocker — collaborator tonemap
   WIP breaking rendering — was resolved; fixes pixel-verified in capture
   passes on 2026-09-05.)
   GOTCHA (resolved 2026-09-05): the ~/.cargo/bin/rustup shim that silently
   no-opped on Aug 29 works again (cargo 1.98.1); the direct toolchain PATH
   workaround is no longer needed but remains a fallback if it regresses.

STILL OPEN:
- (Pale strip at viewport top and scene/tests.rs approximate-constant
  clippy errors: both gone as of 2026-09-05 — resolved by later layout and
  fixture work; no repro in current headless captures or clippy runs.)
- Private storage for uploaded textures (see item 10; GPU capture needed).
- #57 remainder: documented invariants for remaining Metal types, executor
  model, TSan stress test.

## 2026-08-30 — Editor UI polish + layout-overlap widget fixes (93a80165)

- Fixed pre-existing co_creator panic on entity selection: all editor views
  share one positional StateArena slot counter; inspector's conditional
  section slots shifted later views. Slots now reserved unconditionally.
- Fixed chrome-overprint class bugs: Section, TabBar, DraggablePanel now
  reserve their header/strip in layout_style; Grid captures child count at
  construction (child_widgets drains before taffy styles).
- Docked panels use `panel_body()` (tab strip = header, no duplicate title);
  standalone Panel draws title + divider. Control primitives token-sized
  with hover + tooltips; console autoscroll via ScrollView `.auto_scroll` +
  new `post_layout` widget hook; asset browser up button; hierarchy
  fixed-height rows; status bar brand filler removed.
- Verified: `--headless --ui-test` 5 states judge-passed at 1280x720
  logical; katla_ui 640 + katla_app 252 tests green; clippy clean (my
  crates). Only my files committed at the time; the collaborator WIP
  (gfx bridge, RCP palette, layout.rs viewport hunk) has since landed.
- 2026-09-10: #100 done via PR #112 (squash 392d8075): compile_material takes
  typed PipelineDescriptor (pbr/ui/skinned/billboard/simple/compute ctors),
  Metal routes on layout identity, trait callers migrated, descriptor tests
  green; Linux+macOS CI passed before merge. Worktree ../Katla-100 removed.

## 2026-09-11 — Issue #92 core slice: backend-neutral public API (PR #120, squash 47a66ff5)

- katla_gfx root API is backend-neutral: implicit `pub use vulkan::…` re-exports
  removed; example-facing native types moved behind feature-gated
  `katla_gfx::vulkan_native` (with lifetime/safety doc); root `FrameGraph`
  alias (Vulkan-selecting, zero users) deleted; `MaterialOptions`/`VertexType`
  crate-internal, inherent `VulkanRenderer::compile_material(path, options)`
  demoted to `pub(crate)` — public path is `GpuRenderer::compile_material(&PipelineDescriptor)`.
- Re-homed: `RetirementSnapshot` → `renderer::retirement` (portable diagnostics);
  `CompositingDescriptorSet` → `vulkan::compositing` (was Vulkan-typed in the
  neutral render_graph tree, root-exported). `CompareOp`/`CullMode`/`FrontFace`/
  `DepthState` unconditionally public (PipelineDescriptor field types — previously
  nameable only under `validation`).
- App migrated: gizmo + billboard dropped their `AnyRenderer::Vulkan` vs `Metal`
  material branches (the Metal fallbacks compiled different effective materials —
  no HDR format / no depth-off). Builder UI/geometry materials on descriptors.
  UI/billboard semantics unchanged (compiler already normalized UI to depth-off;
  `PipelineDescriptor::ui/billboard` encode that state exactly). 9 device suites +
  6 validation examples migrated to the descriptor / vulkan_native paths.
- Validation: fmt/clippy clean, workspace tests green, 10 GPU device suites
  (incl. new `backend_neutral_api.rs` — representative frame through the portable
  surface only), 100-frame headless 0 ERROR / 1 WARN (= baseline), harness 8/8,
  metal/ hand-audited then confirmed by macOS CI.
- Gotchas hit: (1) `backend_neutral_api` SIGSEGV'd until `init_shadow_resources`
  was called before PBR material compile — PBR descriptor layouts need the shadow
  init like the other suites; (2) grepping test files with `| head -N` truncated
  the migration list — 5 more suites surfaced via the workspace build; grep
  without head when enumerating migration sites; (3) the `compute` module +
  `PoseCompute*` stay Vulkan-native (→ #32); app per-backend graph presets stay
  (→ #56; `AnyRenderer` is not a `RenderGraphBackend`). #92 remains open for both.

## 2026-09-11 — Issue #98 UI slice: dead UI command texture state (PR #121, squash 44c64a37)

- `UiDrawCommand.texture` deleted (write-only on both backends) plus the app-side
  `TextureHandle::from_raw(bindless_slot, 0)` fabrication in
  `katla_app/src/ui/renderer.rs`; `types.rs` also dropped its now-unused
  `TextureHandle` re-export. Metal `UiUniforms` struct replaced by a free
  `ui_uniforms(draw_list) -> [f32; 4]` (16 bytes preserved: old struct was
  8+4+4). `ui.wgsl` `UiUniforms` is one `params: vec4f`; `vs_main`/`vs_instanced`
  read `.xy`/`.z`. Vulkan's `update_ui_descriptor_set` already wrote
  `[w, h, 1.0, 0.0]` — unchanged.
- Validation: fmt/clippy clean, workspace 2122 passed, Vulkan device suites all
  pass (incl. headless_render + attachment_semantics, which compile the edited
  ui.wgsl and assert pixel-exact red/white sampling), 100-frame headless 0
  ERROR / 1 WARN (= baseline), interaction harness 8/8. The 4 validation-layer
  WRITE_AFTER_READ/WRITE_AFTER_WRITE hazards in interaction-test mode are
  byte-identical to an origin/main baseline run (verified via stash + rebuild).
- Gotchas: (1) `cargo test -p katla_gfx -- --ignored` fails the pre-existing
  `bda.rs` ` ```ignore ` doctest (doctests marked ignore get RUN under
  `--ignored` and that fragment cannot compile) — invocation artifact, not a
  regression; (2) macOS CI flaked on
  `pipeline_archive::test_pipeline_archive_rebuilds_on_metadata_mismatch`
  (assert `opened_from_disk` after register+flush+reopen) — green rerun on the
  identical commit; rerun before debugging, main has been red on this area once
  before; (3) the `game` binary is `cargo build -p game` (not katla_app).

## 2026-09-11 — Issue #88 closed: pipeline variants from a canonical key (PR #123, ded9c091)

- Design: `PipelineVariantKey::resolve(descriptor, requested)` — requested concrete
  format wins over the descriptor's declaration; `Auto` falls back to declared, then
  B8G8R8A8Srgb. `derive_depth_format` is ONE shared function both backends build from
  (unified Vulkan's `is_compositing || !depth_test` with Metal's `!test && !write`;
  divergence only for test=false/write=true, which no real descriptor uses).
  `MaterialAsset { descriptor, variants, textures }`; variant lookup is a plain map get.
- Pass-format plumbing (Vulkan): `execute_draw_list(cmd, list, format)` and
  `resolve_draw_commands(lists, frame, format)` take the pass's output_format; UI and
  compositing resolve per pass; all fallbacks are `unwrap_or(ImageFormat::Auto)` so a
  concrete-declared material keeps its declared variant when a pass declares no format.
- Metal: `collect_draw_lists` ensures variants per pending pass — draw lists ONLY for
  `PassKind::Geometry` (depth-prepass lists have R32Uint output_format; compiling for
  them would build float4-output pipelines against an integer target and fail under
  Metal validation), UI pass materials at drawable B8G8R8A8Srgb; unknown handles skip
  with a warn (`has_material_impl`) instead of failing the frame. `draw_objects` and
  the UI record resolve through the variant map. Materials register only after every
  pipeline (declared variant + UI instanced) builds.
- Deleted: `MaterialOptions`/`MaterialType`, `MaterialCompiler::compile_deferred_material`
  and `build_pipeline_from_modules`, write-only `material_descriptor_set/layout`,
  `RetiredDescriptorSetLayout` + `RetirementKind::DescriptorSetLayout` + snapshot field
  (destroy_material no longer had any layout to retire).
- Validation: new `tests/pipeline_variants.rs` (6 tests). Full workspace suite green
  (65 result lines). Headless 100-frame caught the one real bug: deferred variants
  built from the RAW descriptor (color = Auto → VK_FORMAT_UNDEFINED in
  VkPipelineRenderingCreateInfo, 21 VUID-08963/08910 errors on frame ~1); fix =
  compile_variant builds from `key.descriptor()`. The bug was invisible to the new
  device suite's GPU tests because validation is Disabled there (PBR-under-system-
  validation-layer segfaults the local Intel driver) — only the validation-Enabled UI
  regression test and the game run catch it.
- Readback gotcha: `queue_async_readback` copies the headless DRAWABLE (always BGRA),
  normalizing byte order — device tests cannot assert attachment byte order through
  it (same red lands in byte 2 regardless of the declared target format).
- CI gotchas: macOS clippy is lib-only `-D warnings` for katla_gfx AND katla_app —
  test-only pub methods on crate-reachable types die there (`material_variant_count`
  had to be used in prod logging); `&mut` extraction traps: splitting a `&mut self`
  method body into a `&self` helper silently breaks if the body mutates any field
  (bindless init E0596); clippy flags `let Some(ref x) = opt_ref` (double ref) and
  &expr` in log args. `cargo check --target aarch64-apple-darwin` originally failed on
  this Linux box because `objc2-exception-helper` shelled out to `cc` with `-arch`;
  overriding the C compiler fixes it (see the 2026-09-19 entry) — later sessions
  should use that instead of hand-auditing. Two macOS CI cycles were spent on
  E0596 + two needless-ref lints.
- Workflow note: origin/main moved mid-session (#122 from the live Katla-98
  worktree) — `git diff origin/main` then showed reverse-#122 hunks in MY tree;
  check `git merge-base HEAD origin/main` before interpreting diffs, commit then
  `rebase origin/main` (clean), push. A NEW worktree (Katla-37) appeared mid-session
  from another parallel session; never touched.
- `particle_stress_tests::test_frame_rate_stability` failed once in a full workspace
  run ("frame time degradation 547.9%") and passes in isolation — load-flaky timing
  test, rerun before debugging.

---

## 2026-09-19 — Issues #89, #101 closed; #37 slice 3 landed

Main went `57f3c3b9` → `4a784180`. Three PRs, all with Linux + macOS 26 CI green:

| PR | Issue | Squash | What |
|---|---|---|---|
| #126 | #37 (slice 3) | `b6f7b447` | Transient aliasing slots in diagnostics exports (schema v9→v10) |
| #127 | #89 (closed) | `0f442ca5` | Frame-scoped rendering API (`FrameToken`) |
| #128 | #101 (closed) | `4a784180` | Prepared frame draw data (`PreparedDraws`) |

### #89 — frame-scoped rendering API (closed)

`GpuRenderer::acquire_frame() -> Result<FrameAcquisition>`:

- `Ready(FrameToken)` — owns one reusable frame slot (and, when windowed, one
  surface image). **Acquisition waits for that slot's previous submission**, so
  frame-local CPU writes can never race a slot still in flight.
- `Unavailable` — surface produced no drawable this tick; nothing touched.
- `OutOfDate` — stale surface; recreate and re-acquire.

Every frame-local op takes the token and validates it against the renderer's
open frame: `set_frame_uniforms`, `execute_draw_calls`, `draw`, `upload_lights`,
`upload_shadow_cascades`, and `render`. `present(token)` consumes it with one
semantic on both backends (submit + present, returns after enqueue/commit).
`abort(token)` abandons it — nothing submitted or presented, slot reusable. A
failed `render` **poisons** the frame so `present` cannot submit half-encoded
work.

Deleted outright (no wrapper): `begin_frame`/`end_frame`, `wait_for_frame`, and
the untokened `set_frame_uniforms`/`execute_draw_calls`/`upload_lights`/
`upload_shadow_cascades`. `katla_app` renders through the token on both
platforms; Metal's `render` moved off implicit acquire/release onto the
compiled-pass path behind an open token.

Two things worth remembering:

- **`FrameToken` is `Copy`, so "dropping a token" cannot be the abandonment
  signal.** Documentation that claimed a destructor was wrong and was fixed.
  Abandonment is simply the next acquisition (which calls `frame_clear`).
- **A slot only advances on `present`, not on acquire.** Two successive
  acquisitions for an unfinished frame land on the SAME slot; ownership is per
  acquisition (generation counter), not per slot index. A test asserting
  "successive acquisitions own successive slots" was wrong — `present` is what
  rotates slots, so a slot cycles only across *finished* frames.

Real bug found and fixed mid-slice: the new Vulkan `render` initially gated both
surface-image transitions on `swapchain.is_some()`, dropping the headless
`COLOR_ATTACHMENT_OPTIMAL -> TRANSFER_SRC_OPTIMAL` transition that readback
depends on (`main` did both unconditionally). Now unconditional with the
headless case targeting `TRANSFER_SRC_OPTIMAL`.

Device suite `katla_gfx/tests/frame_scope.rs` (6 tests, all green on Intel
Vulkan): normal frame; frame-local calls with no open frame fail typed;
finished and superseded tokens rejected; abort leaves the slot reusable;
abandoned acquisition reuses the slot; slots cycle with work in flight and
readback matches the presenting frame. Surface-unavailable/out-of-date need a
real presentation surface, so they are covered by the app's
swapchain-recreation path instead.

### #101 — prepared frame draw data (closed)

`PreparedDraws<'a>` (`renderer/types.rs`) borrows one pass's submitted draw
lists from frame-owned storage; `iter()` walks submissions in submit order and
object slots stay exactly as `DrawList::push` assigned them.
`PreparedDrawCounts` gives per-pass draw/instance totals.
`Frame::submit`/`AnyFrame::submit` now take **`Rc<DrawList>`** — the list moves
into frame-owned storage once, so submitting one list to several passes costs a
refcount bump instead of a deep clone.

All consumers switched to the borrowed view: Vulkan geometry/draw calls,
parallel geometry, outline scissor; Metal geometry, depth-prepass, outline,
shadow, object-id. The per-pass `merge_draw_lists` rebuild and Metal's rebuilt
frame-level upload list are **deleted**; Metal uploads each unique submission
once (`Rc::as_ptr` identity), so shared lists are not re-uploaded while every
encoded slot is still initialized. Metal pass traces carry the prepared counts,
making prepared work attributable to a `PassId`.

Measured (`katla_gfx/benches/frame_preparation.rs`, CPU-only, counting global
allocator — measured, not estimated):

| scenario | legacy | prepared |
|---|---|---|
| 64 lists × 8 draws | 395 µs, 430 allocs/frame | 9.7 µs, 21 allocs/frame |
| 1 list × 2000 draws | 1.73 ms, 16 allocs/frame | 7.9 µs, 5 allocs/frame |

### #37 slice 3 — aliasing diagnostics (schema v9 → v10)

`transient_slots` entries now carry the slot's physical compatibility class
(kind/format/extent/swapchain tracking), the summed standalone bytes of its
members, and the estimated bytes aliasing saves; members render in first-use
order so each entry reads as the alias chain (predecessor → successor). DOT
renders each slot as a dashed `cylinder` physical-storage node wired to its
member resources with dotted `alias` edges, distinguishing physical allocations
from logical graph resources. The allocation plan retains chronological
membership, per-resource bytes, and slot start positions to feed those views.
Goldens re-blessed; two focused tests pin the JSON fields, the text line, and
the DOT nodes/edges. #37 remains open.

### Process findings (important)

- **macOS `cfg` code IS checkable on this Linux box.** The blocker was that
  `objc2-exception-helper` shells out to `cc` with `-arch`; overriding the C
  compiler makes the whole cross-check work:
  ```
  CC="clang --target=arm64-apple-macos11" CXX="clang++ --target=arm64-apple-macos11" \
    cargo check -p katla_gfx --tests --target aarch64-apple-darwin --locked
  ```
  This compiles the Metal module *and* the integration tests (`--tests`), and it
  caught three macOS-only errors that would each have cost a CI cycle:
  1. `MetalRenderer::frame_uniforms` dropped from the trait impl while moving
     lifecycle methods (E0046) — a `grep` for `fn frame_uniforms` across the file
     missed that the macOS-only impl had lost it.
  2. `GpuRenderer::execute_draw_calls(self, &upload_list)` left untokened in
     `metal/frame_render.rs` (E0061).
  3. `self.renderer.wait_for_device().expect(...)` in
     `tests/contract/harness.rs` — `wait_for_device` returns `()`, so the
     `expect` on `main` had been reachable only in macOS builds (E0599).
  Verify the check is real by injecting a deliberate type error and confirming it
  fails — it does. **`katla_app` still cannot be cross-checked** (mlua/ring C++
  build scripts need a real Apple SDK); its macOS path stays a line-by-line
  audit.

- **`git commit --amend` without `-a` silently drops unstaged edits.** Edited the
  file, ran `git commit -q --amend --no-edit` (no `-a`), so only the index was
  committed and CI tested the *old* tree — twice. Always `git add -A` (or
  `git commit -am`) before amending. `git show HEAD:<path>` confirms what was
  actually committed; `git status` after the amend shows the forgotten edit.

- **PR-body heredocs with backticks execute as shell.** `gh pr create --body
  "$(cat <<'EOF' ... EOF)"` still let backticked identifiers run as commands and
  truncated the body. Write the body to a file, then `gh pr edit <n>
  --body-file`.

- **Stacked PRs get no CI here.** CI only runs for PRs whose base is `main`, so a
  PR targeting another feature branch reports "no checks reported" forever, even
  after retargeting (retargeting does not re-trigger). The working sequence:
  land the base PR first (both its jobs green), rebase the child onto the new
  `main`, force-push, and its CI starts.

- Squash-merge + `--delete-branch=false` matches this repo's history; issues are
  closed with the DoD checklist in a comment.

### Validation commands used

- `cargo test -p katla_gfx --lib --locked` (455 → 459 tests as slices landed)
- `TMPDIR=$HOME/tmp cargo test -p katla_gfx --tests --locked -- --ignored --test-threads=1`
  (full device suite; serial because lavapipe races, but this box has real Intel
  Vulkan so no skips)
- `cargo fmt --all -- --check` and
  `cargo clippy -p katla_gfx -p katla_app --lib --locked -- -D warnings`
- `TMPDIR=$HOME/tmp cargo run -p game --release -- --headless --screenshot X.png -s`
  (100 frames; 0 errors, 1 pre-existing WARN for `fox1` normals)
- Pixel-diffing headless captures: both slices landed within the same-build
  run-to-run noise floor (measured 2.787% by rendering the *same* commit twice),
  so "differs from main by 2.7%" was equivalent output, not a regression.
  There is no PNG decoder in the toolchain — a ~40-line pure-Python
  zlib/struct PNG reader handled it.
- `game/benches/performance_benchmark.rs` fails to compile on `main` too
  (missing `rand` dev-dependency) — pre-existing, unrelated; ignore it.
