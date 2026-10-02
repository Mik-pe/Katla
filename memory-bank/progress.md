# Progress

## Current Delivery

- The direct-main ten-issue batch contains #138, #31, #32, #33, #35, #36, #53, #54, #55 and #58. ECS is delivered and closed after green Linux/macOS/Miri CI. The nine graphics implementations are complete, with native Vulkan and Metal acceptance, full editor interaction QA and source-hashed scene/cache measurements. CI enforces the supported macos-26 and Ubuntu 24.04 contracts.
- Graph buffers and neutral compute commands now declare actual animation, skeleton, light and particle dependencies on both backends. One compiled synchronization plan handles subresources, native buffer ranges, role changes, transfers, alias visibility and output contracts. Vulkan speculative layouts roll back on abort.
- Transient storage uses compiled lifetimes, per-slot native allocations, Metal placement heaps and Vulkan alias-compatible memory. Memoryless/lazy storage requires whole-lifetime tile-local discard operations. Allocation-contract changes reject before encoding and require safe cleanup/reinitialization.
- Metal uses one Metal 4 command/compiler/archive/binding/residency stack, three bounded frame owners, immutable publication and completion-owned upload retirement. Native private-texture uploads have format/subresource validation, bounds and metrics. Shader replacement and pipeline warmup run off frame encoding.
- Focused Metal validation proves direct/indirect compute, small-pool particle results, queued animation matrices, independent UI uploads, imported/output attachment contracts, texture sampling/readback, immutable replacements and resize/streaming stress. Whole-editor static output matches the previous renderer in checked regions. All eight editor interaction checks pass, including exact object selection. Picking preserves global IDs for instanced draws and retains the last committed graph source across abort, resize and slot reuse. The final workspace library suite passes 2,178 tests (15 ignored), native Metal contracts pass 13/13, Linux graphics passes 560 tests plus seven contract and two aliasing scenarios, and strict Rust 1.99 production Clippy passes. Validation and measurement provenance are recorded in docs/metal4_validation.md.

## Completed ECS Delivery

- **Issue #138: typed, safe parallel ECS (2026-10-02)** — sealed parameters derive access claims and prepare independently borrowed data; workers never receive World or a registry. Commands apply FIFO in registration order at batch boundaries. Typed queries cache membership and dense offsets, while sealed structural filters invalidate on insertion/removal, dense relocation, entity recycling, and clear/respawn. Four camera/animation systems are typed; scripts, physics and hierarchy remain explicitly exclusive. Generations preserve stale-ID rejection and exhausted slots retire. One sparse store remains after source-hashed sparse/archetype benchmarks. Fresh local validation: 237 all-feature ECS unit tests, integration tests and doctests, strict ECS all-target/application/script Clippy, 43 focused Miri tests, and the pre-graphics-migration workspace suite (2,079 passed, 11 ignored). The existing 13-scenario Metal contract suite and 100-frame headless/windowed runs passed under native API validation; the baseline scene capture was reviewed. Benchmarks were refreshed after filter sealing; concurrent graphics compilation caused visible timing spread, so no causal speedup is claimed. Linux/macOS ECS and pinned Miri checks are in CI. Remaining lifecycle cleanup and extended sanitizer/soak work are distinct roadmap tasks.

## Earlier Deliveries

- **Application-owned frame graphs (#60, #61, #62)** — applications can provide a one-shot graph factory and runtime policy; Katla's editor renderer is an explicit preset rather than an engine invariant. Empty, UI-only, geometry-only, and custom graphs are supported.
- **Optional Metal topology (#49)** — removed the requirement that every Metal frame contain scene depth, geometry, tonemapping, and UI. Absent passes no longer encode hidden work or wait on unsignaled fences.
- **Structured Metal execution failures** — terminal command-buffer failures now surface backend, label, status, native code/domain, and localized description through `RendererError`.
- **Compiled Metal pass stream (#56 slice)** — deleted the Metal-only singleton schedule and fixed semantic rank table. Metal consumes the render compiler's canonical order with exact `PassId`, pass-local draw/UI submissions, repeated semantic categories, and deterministic traces.
- **Explicit object-ID pass** — GPU picking is a real render-graph pass on Metal and Vulkan instead of invisible geometry-pass work. Vulkan reuses the shared skinned/billboard draw path.
- **Render-graph pass culling (#34)** — exported resources and side-effect passes are liveness roots; true producer dependencies retain required predecessors; execution, parallel groups, material work, and submissions are live-only; loaded/blended targets declare read-before-write dependencies; diagnostics expose declared/live/culled state.
- **Repository hygiene** — temporary source-export and patch-materialization pull requests were closed without merging; product changes are rebuilt as clean commits directly above `main` before canonical CI.

- **Native compiled attachments (#56)** — graph declarations own color/depth/stencil targets and operations. Helpers draw into supplied encoders; hidden target fallbacks and implicit scene depth are removed. Native traces compare emitted work to the compiled contract. Arbitrary imported images and compute migration have since been integrated into the current batch.
- **Prepared draw data and frame-scoped API (#89, #101)** — submissions reuse immutable draw lists; explicit acquisition tokens guard mutable writes and preserve slot ownership.
- **Portable graph capture (#37, partial)** — deterministic JSON/text/DOT and failure artifacts exist. Schema 12 includes compiled boundaries, buffer ranges, persistence and native per-slot texture allocation records. Native Metal submission diagnostics expose reflected layouts, residency and feedback. Full portable capture integration and finer queue/submission identities remain open.
- **Canonical pipeline variants (#88)** — render/material identity includes shader, attachment and raster state; variants are prepared and reused from canonical keys.
- **Shader and editor correctness** — billboard alpha depth, shadow sampling, sky/grid state, viewport sizing and layout overlap were corrected. Public backend-neutral rendering APIs no longer expose raw Vulkan handles.
- **Optional MCP dependency security** — rmcp resolves to 2.2.0; the feature-gated server path was validated and Dependabot alerts cleared in the September 27 delivery.

## Follow-Up Work

- #37 portable capture integration and #93 wider ECS roadmap remain outside the ten-issue batch.
- Backend-neutral texture-view cleanup, compositing/stencil handlers and broader ECS sanitizer/soak coverage remain separate tasks in TODO.md.
- Default application features are validated. Optional all-feature MCP editor polling still has a double mutable borrow and an unused router field; repairing that integration is outside this batch.
- Physical local Metal evidence uses Apple M5/macOS 27. CI supports exactly macos-26 and Ubuntu 24.04; do not add older macOS jobs or mutable runner aliases.
