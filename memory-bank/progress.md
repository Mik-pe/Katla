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
