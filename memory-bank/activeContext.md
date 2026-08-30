# Active Context

## Current Focus

- **Complete render-graph execution plans (#56)** — application-owned topology, exact pass identity, pass-local submissions, explicit object-ID work, and dead-pass culling are complete. The remaining work moves graph-declared attachments, load/store/clear state, viewport/scissor, and generic executable payloads into backend records.
- **Preserve the engine/application boundary** — Katla's editor pipeline is an explicit preset. The engine must continue to support empty, UI-only, geometry-only, reordered, repeated, and fully custom pass graphs without hidden scene work.
- **Keep Metal honest** — Metal consumes the compiler's canonical live pass order. It may reject unsupported executable payloads before command-buffer creation, but it must not invent topology or silently add editor passes.

## Verified Render-Graph State

- `ApplicationBuilder` accepts a one-shot frame-graph factory and explicit runtime policy.
- Pass and resource capability bindings are optional. Missing capabilities disable dependent app behavior instead of using sentinel IDs.
- The compiler retains exact `PassId` identity and produces deterministic execution and parallel groups.
- Exported resources and explicit side-effect passes are liveness roots.
- Dead branches are removed from execution, material work, submissions, and diagnostics.
- Loaded or blended attachments declare read-before-write dependencies.
- Submissions to culled passes fail with a structured render-graph error.
- Text, JSON, and DOT diagnostics distinguish declared, live, and culled passes deterministically.
- Metal executes ordered pass records and consumes only submissions addressed to each record.
- Object-ID/picking is an explicit graph pass on Metal and Vulkan.

## Current Focus: Shadow Pipeline (complete)

Cast shadows render on Metal for all geometry — regular meshes (commit
6322f156) and skinned meshes (commit 24a872c2). Verified headless with pixel
probes: soft PCF edges, no acne, fox casts a quadruped-shaped shadow.
Both backends share the corrected cascade code (inverse-order view_proj_inv,
zero-to-one NDC ortho, real-extent z-pancake, raw splits, texel-size bias
units, single-pass atlas encoding). Skinned shadows use a dedicated
ShaderProfile::ShadowSkinned MSL binding map (joints to buffer 4, avoiding
the buffer-3 collision with shadow_params). CI fully green.

## Open Items
## Open Items

- Docs: `docs/metal_backend.md` is now the Metal architecture reference
  (2026-08-30, commit 4bcb207a); the pre-implementation plan is archived with a
  superseded banner. Issues #51 and #57 closed after tip verification
  (CI 33239691228 at 59e0a50c); #53 and #58 carry state-of-play comments.
- Pale strip at viewport top (y≈125–158 in 2560×1440 screenshots) — UI-side,
  not a renderer artifact (present in pre-shadow screenshots; no code
  constant matches its colours; clears are black). Investigated 2026-08-29:
  see skill references/shadow-debugging.md "Pale strip investigation".
  Overlaps the uncommitted collaborator WIP (Panel RT debug, viewport fix).
  Ruled out: gizmo-row container (hstack hugs children, ~350px — band is
  1461px); theme constants; canvas/HDR clears (both dark).

## Temporary Boundaries

- Shadow and depth-prepass remain explicit side-effect roots in the editor preset while their native targets are still backend-owned.
- Some Metal semantic handlers still resolve backend-owned textures rather than graph resource handles.
- Generic/custom executable payloads and backend-neutral compute commands are not complete.
- Transient resource allocation is not yet live-range aliased.

## Validation Baseline

- Linux graphics checks, tests, and strict Clippy pass for the compiled pass stream and pass-culling work.
- `katla_app` library tests pass with 251 tests successful and 2 ignored in the isolated validation flow.
- Canonical Linux and macOS 26/Metal CI are required before every merge.

## Working Rules

- The application owns graph topology and presets.
- The render-graph core owns validation, dependency analysis, liveness, stable identity, ordering, and diagnostics.
- Backends own native resource realization and command encoding only.
- No backend may infer a universal editor pipeline from semantic categories.
- No hidden work may be attached to another pass for convenience.
- Prefer structured errors before native command-buffer creation over silent fallback behavior.

## Next Actions

0. Texture uploads: staged blit queue landed (89dc6bba, shared storage).
   Private storage blocked on the sampling anomaly — needs an Xcode GPU
   capture; probe test `storage_mode_sampling_probe` is the starting point. All in-process paths (direct sampling,
argument-buffer sampling, residency) are exonerated; only an Xcode GPU capture
of the full app render remains. #82 closed (fixed by 6322f156); #57 first
slice landed (MetalSurface Send/Sync removed, guard in surface.rs); #53 core
landed (MTLBinaryArchive + sidecar, atomic flush, registration on both
pipeline paths).
1. Carry graph-declared color/depth attachments and load/store/clear metadata into executable records.
2. Resolve Metal render targets through graph resource handles per frame.
3. Carry viewport/scissor policy into the plan instead of deriving editor-specific rectangles in the backend.
4. Define a generic executable payload/handler contract for custom graphics and compute passes.
5. Remove temporary shadow/depth side-effect roots once their outputs are graph-owned.
6. Close #56 only when the backend no longer reconstructs pass semantics outside the compiled plan.
