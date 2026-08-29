# Progress

## Completed Recently

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

STILL OPEN:
- Pale strip at viewport top y~125-158 (UI-side, unchanged by this work).
- katla_app scene/tests.rs has pre-existing clippy approximate-constant errors
  on main (untouched by this work).
- Private storage for uploaded textures (see item 10; GPU capture needed).
- #57 remainder: documented invariants for remaining Metal types, executor
  model, TSan stress test.
