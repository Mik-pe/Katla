# Active Context

What is being worked on right now. **Update this file when starting or finishing a task. Remove entries when work are complete.**

## Current Work

- **Metal/render-graph hardening (#49 / PR #50)** — the Metal schedule now validates the required geometry/fullscreen/UI pipeline and the concrete `hdr_color -> viewport_0 -> backbuffer` resource contract before encoding. Retained command-buffer failures are returned as typed renderer errors with native Metal diagnostics. Remaining typed-access, buffer, synchronization, culling, aliasing, frame-lifetime, direct-path removal, diagnostics, pipeline-cache, Metal 4, residency, executable-plan, thread-affinity, upload, and documentation work is split into #30–#37 and #51–#59.
- **PhysicsActive(false) at builder init** — physics is now off in editing mode (the default). PlayStart action sets it to true, PlayStop sets it back to false. SceneSnapshot preserves physics components for restore on stop.
- **State slot stability** — ConsoleView and MixerView now always call `ctx.state()` unconditionally (even when their env is not set) to prevent slot shifts that corrupt DockSpace/Toolbar state IDs when tabs become active.
- **DockSpace global input** — DockSpace remains non-interactive for normal hit testing so panels underneath receive input, but owns tab and splitter interaction through the declarative global-input pass. There is no separate editor-side dock input path.
- **Selectable flex_grow opt-in** — Selectable widget defaults to `flex_grow: 0.0` (content-sized) instead of `1.0` (fill parent). Call `.flex_grow(1.0)` where fill behavior is needed.
- **Asset browser click semantics** — the first click selects an asset; a second click on the same asset within the UI double-click window activates it. Destructive context actions resolve the stored asset index and never fall back to deleting the current directory.

## Architecture Note

- Render-graph scheduling has one source of truth: declaration-order resource versions produce the canonical RAW/WAR/WAW DAG, stable topological order, cycle diagnostics, predecessor/successor metadata, and parallel levels.
- Graph construction is fail-fast. `backbuffer` is the only implicit resource; pass references must resolve to a declared transient resource or explicit import before compilation/allocation.
- Backend integration consumes compiled pass identity and access metadata. Core render-graph data must not contain Vulkan/Metal command types, and no backend should maintain an independent pass-name execution graph.
- Metal consumes a validated semantic schedule derived from the compiled graph. Missing, duplicate, compute, unsupported, out-of-order, and resource-contract-invalid semantic passes fail before command encoding.
- The Metal application contract is geometry writing `hdr_color`, optional outline reading/writing `hdr_color`, fullscreen reading `hdr_color` and writing `viewport_0`, and UI reading `viewport_0` and writing `backbuffer`.
- The Metal editor path tonemaps into graph-owned `viewport_0` using texture-local coordinates, then lets the UI composite that texture into `backbuffer`. The remaining implicit geometry-to-drawable legacy branch is not an accepted architecture and is tracked by #51.
- Retained Metal command buffers are checked after completion. `Error` status returns the native error code, domain, localized description, and command-buffer label; any non-`Completed` status after `waitUntilCompleted` is an invariant failure.
- Editor gizmo/debug draws are prepared before Metal object-uniform upload. The renderer validates the highest submitted instance index against object-buffer capacity before any encoder binds an offset.
- Metal bindless argument buffers are initialized from real device capabilities: Tier 2 uses direct `MTLResourceID` entries, supported Tier 1 devices use shader-reflected layouts, and unsupported virtual devices fail with a typed error before an invalid Objective-C call.
- Compiled graphs expose deterministic human-readable, JSON, and Graphviz DOT diagnostics containing stable pass/resource metadata, execution order, parallel levels, lifetimes, and RAW/WAR/WAW hazards. Backend pointers and unstable IDs are excluded.
- CI uses one explicit current macOS environment: `macos-26` on Apple Silicon. Katla does not run a backwards-compatible macOS matrix, and mutable `macos-latest` is not used.
- The Metal 4 migration is a clean cut after frame-slot and synchronization ownership are defined: replace the old command path (#54), then move binding/residency to argument tables and residency sets (#55). Do not retain parallel old/new production paths.
- Metal surface ownership must encode AppKit/CAMetalLayer thread affinity in types. Unconditional `unsafe impl Send/Sync` for surface/drawable state is tracked by #57 and is not accepted as a permanent invariant.
- Texture uploads should use format-aware staged copies into private GPU storage with completion-owned staging lifetime. Direct shared-texture writes and silent default-texture substitution are tracked by #58.
- `docs/metal_backend_implementation.md` is a stale migration plan, not a trustworthy current architecture reference. Its clean replacement/archival is tracked by #59.
- Panel widget now reserves top padding via `header_height` (28px by default) so content renders below the DockSpace tab bar. The DockSpace draws tab bars as an overlay on top of panels, so panels must offset their content.
- `TAB_BAR_HEIGHT` constant (28.0) defined once in `editor_root.rs`, matching `DockSpace::tab_bar_height`.
- DockSpace tab bar now uses `tab_text` (inactive, #8E8E93) and `tab_active_text` (active, #FFFFFF) from UiStyle instead of generic `text_color`.
- TabBar widget (preferences) uses the same proper theme colors.
- `EditorOverlayView` builds every docked panel in a stable order to preserve positional state slots, but only mounts the active tab from each `DockTree` leaf into the ZStack. Stale environment values for inactive tabs therefore cannot render over the active panel.
- Declarative text fields are read back from their `StateId` during the same build; environment search strings are initial values, not the live source after editing.
- Declarative input consumption accumulates across multiple input passes during a frame and is reset by `UiContext::begin()`.
- Splitter drag ratios are computed against the bounds of the split node being resized, including nested splits.
- Dock tab move actions carry the exact dragged tab; the editor preserves that identity when applying the tree mutation.
- Console level filters and Clear emit typed actions that are applied after the declarative frame.
- Asset browser confirmation dialogs store the pending `AssetAction`; confirmation consumes that exact action rather than reconstructing a path from UI data.

## UI Design Target

- **Reference**: Apple Reality Composer Pro — clean, modern, minimal chrome
- **Font**: Proper sizing with Retina scale support (scale_factor aware)
- **Layout**: Well-spaced panels with correct padding, margins, and alignment
- **No artifacts**: No visual glitches, no clipping issues, no half-rendered elements
- **Goal**: State-of-the-art game engine editor UI, not a prototype

## Vision Debugging Pipeline

1. `cargo run -- --headless -s --screenshot /tmp/katla.png` — headless render
2. Feed PNG to vision model for analysis
3. Fix issues, repeat until clean

## Recent Decisions

- Render-graph resource versions are defined by pass declaration order; a later writer cannot retroactively become the producer for an earlier read.
- Stable declaration order is the deterministic tie-breaker for otherwise independent passes.
- Metal pass routing uses compiled pass indices and semantic `PassKind`, never string-name dispatch. Depth-prepass and geometry submissions remain distinct, and geometry loads/stores depth when a prepass ran.
- Metal semantic validation includes the required graph resource accesses; a matching `PassKind` sequence alone is not sufficient.
- The application graph's `hdr_color -> viewport_0 -> backbuffer` chain is the editor render contract, not optional metadata alongside a separate renderer.
- A completed Metal command buffer must be inspected for terminal status; asynchronous logging alone is not sufficient error handling.
- Metal object-buffer overflow is a hard renderer error; invalid offsets are never submitted after a warning-and-continue fallback.
- macOS CI tracks one current explicit generation. A future upgrade replaces `macos-26` directly instead of retaining an older compatibility job.
- Metal 4 adoption removes the pre-Metal-4 production command path instead of adding a compatibility switch.
- AppKit/Metal surface thread affinity must be explicit; blanket unsafe sharing is not a scheduling strategy.
- Low-level GPU texture allocation/upload failures remain errors; asset placeholder substitution is an explicit higher-level policy.
- Architecture documentation must describe verified current code; superseded migration plans are archived or deleted rather than treated as agent instructions.
- Asset browser activation is derived from repeated `AssetClicked` actions tracked in `AssetBrowserState`; grid cells do not emit activation on every click.
- Asset deletion refuses empty paths and the synthetic `..` parent entry.
- Default theme is "rcp" (Reality Composer Pro): neutral dark #1E1E1E, muted orange #D97706 accent. "default" and "catppuccin" keys still map to RCP for backward compat. Preferences dropdown lists RCP first.
- RCP selection colors: primary #D97706 (amber), hover #E8913A (warm orange), highlight #B45309 (dark amber)
- Asset browser now uses `panel()` like other docked panels — provides background fill and tab bar padding
- Asset browser item_size increased from 64→80px, icons use FontSize::Huge (24px)
- Asset browser grid cells use `Alignment::Center` for centered icons
- ImageButton reverted to 28px button size and 14px icon font (toolbar-sized, not gigantic)
- `from_style()` and `default_dimensions()` use orange (#F79545) for accent/text_accent instead of blue
- DockSpace and TabBar widgets use tab_text/tab_active_text for proper inactive/active tab colors
- Headless mode uses scale_factor=2.0 and 2560x1440 offscreen texture (matches Retina)
- Headless mode uses the same `Application` code as windowed — no separate code paths
- Instance buffer binding uses byte offsets (not baseInstance) because Metal's instance_id ignores baseInstance
- UI clears the drawable when it composites graph-owned viewport output; direct-render fallbacks use Load to preserve prior scene color.
- Font atlas properly destroyed before recreation (prevents slot thrashing)
- Draw list now preserves submission order across instance/vertex batch types
- Metal instanced pipeline now uses bind_graphics_pipeline (not raw setRenderPipelineState)
- `STATUS_BAR_HEIGHT` defined once in `editor_root.rs`, re-exported from `declarative/mod.rs`
- Metal canvas clear color uses linear value (0.013) so it appears as #1E1E1E on sRGB framebuffer (BGRA8Unorm_sRGB interprets clears as linear)
- DockSpace tab bars use `tab_active_bg`/`tab_inactive_bg`/`tab_hover_bg` from UiStyle, not generic button/selection colors
- UI shader applies srgb_to_linear() in vertex shader — hex colors round-trip correctly through sRGB framebuffers
- `ToolbarDrawCtx` now carries `error` color for stop button — no hardcoded colors in toolbar
