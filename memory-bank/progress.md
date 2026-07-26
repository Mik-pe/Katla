# Progress

What's been done and what's next. **Update this file when completing or starting work. Remove completed items to keep this lean.**

## Completed Recently

- Added permanent CI with one explicit current macOS environment: `macos-26` on Apple Silicon. Katla intentionally has no backwards-compatible macOS job; future runner upgrades replace the current label directly. Added focused Ubuntu 24.04 Vulkan/graphics-library validation, documented the policy, and removed mutable/deprecated runner choices.
- Fixed the Metal Objective-C exception path (#47). Bindless argument buffers are initialized lazily from device capabilities, Tier 2 uses direct resource IDs, supported Tier 1 devices use reflected layouts, unsupported Apple paravirtual devices fail before invalid selectors or GPU submission, and application initialization errors return a non-zero process status.
- Fixed the Metal editor viewport and hierarchy-selection crash (#44). The editor now tonemaps into graph-owned `viewport_0` in local texture coordinates before UI composition, so the 3D scene fills the complete viewport instead of presenting a stale drawable-relative quadrant. Selection gizmo/debug draws are prepared before object-uniform upload, and Metal rejects out-of-capacity instance indices before command encoding.
- Audited the render graph end-to-end across builder, compiler, frame submission, transient resources, Vulkan, Metal, and application graph construction. Merged the canonical dependency DAG (#21), platform isolation (#24), benchmark ownership fix (#26), fail-fast builder validation (#28), Metal semantic scheduling/depth-prepass integration (#39), deterministic graph diagnostics (#42), graph-owned editor viewport output (#44), and safe Metal capability handling (#47). Captured larger follow-up work as focused issues #30–#37 instead of a tracking epic.
- Added deterministic render-graph diagnostics with stable human-readable, JSON, and Graphviz DOT exports. Snapshots include pass/resource metadata, canonical order, parallel levels, resource lifetimes, and concrete RAW/WAR/WAW hazards without backend pointers or hash-order instability.
- Metal now validates a semantic frame schedule compiled from the canonical graph order, routes submissions by pass index and `PassKind`, keeps depth-prepass and geometry submissions separate, loads/stores shared depth correctly, and treats UI as a first-class semantic pass in both Metal and Vulkan dispatch.
- Fixed asset browser action safety: single clicks now select without immediately opening folders or previews, double-click activation is tracked per asset, context actions resolve the stored asset index, delete confirmation consumes the exact pending action, and deletion refuses empty paths plus the synthetic `..` entry. Removed the obsolete `AssetAction::Open` path and unused render-only asset path discovered by full Clippy validation.
- Restored complete editor docking interaction: DockSpace now owns tab activation, tab drag/drop, and splitter dragging through the declarative global-input pass; the duplicate manual editor input path was removed. Nested splitter ratios use local split bounds, and tab moves preserve the exact dragged tab. Added regression tests for nested ratios and non-first-tab moves.
- Wired the Console toolbar: level buttons toggle their filters and Clear empties the shared log buffer through typed declarative actions.
- Fixed declarative input state regressions: Hierarchy, Asset Browser, and Console now filter from their live text-field `StateId` values, and `UiContext` preserves input consumption across multiple passes within a frame. Added tests for consumption accumulation and frame reset.
- Fixed inactive dock tabs rendering over the active panel: `EditorOverlayView` still builds every docked panel in a stable order to preserve positional state slots, but now only mounts the active tab from each `DockTree` leaf into the ZStack. Added a regression test for active-tab collection.
- Fixed split viewport panel: 3D-scene render targets (HDR, depth, tonemap-output, picking) were sized to the swapchain/drawable extent, but the scene is composed for the panel's aspect ratio (camera uses `viewport_size()`). The scene was stretched across the full-screen target then the blit cropped the wrong sub-rect into the panel, producing a split/clipped panel. Now the scene targets are panel-sized: `Application::recreate_panel_rt_resources()` (renderer.rs) recreates depth/HDR/picking (`GpuRenderer::recreate_scene_render_targets`) + frame-graph transients + light-culling grid at the panel size each frame when it changes. Outline/picking pass viewports (frame_render.rs) and Forward+ tile uniform (renderer.rs) use the panel RT size. Picking coords (picking.rs) remap to panel-physical space. `attach_layer_to_nsview` (surface.rs) unconditionally sets `contentsScale`. Non-editor path unaffected (panel size == swapchain extent there).
- Code review pass on katla_gfx/metal, katla_ui, katla_app: fixed dead_code warning in console.rs (search_filter and filter_levels now actually filter log entries), removed spurious ensure_instance_buffer call in ui_renderer.rs, removed blanket #![allow(unused_imports)] in metal/context.rs, deduplicated resolve_wgsl_includes/find_and_read_shader across light_culling.rs/particle.rs/metal_renderer.rs (now all use shared read_shader), added missing Metal pipeline inits (ShadowSkinned, DepthPrepassSkinned, DepthPrepassBillboard) to init_metal(), added bounds checking in skeleton_api update_skeleton_impl and metal_renderer execute_draw_calls (prevents buffer overflows), fixed log levels (warn→debug for expected conditions, gated /tmp MSL dumps behind debug log), removed 18 pre-existing test compilation errors (stale DuplicateContext/unproject_to_ground_plane_impl references), cleaned up unused mut/drop(lit_ref)/unfulfilled lint expectations.
- Default scene physics entities + active on load: ground static box, 10 dynamic spheres (top 2 grid rows), dynamic cube, dynamic cyan sphere, dynamic magenta cylinder. PhysicsActive(true) at builder init for both headless and windowed.
- UI polish round 10: hierarchy text color (#8E8E93), row padding (29px height), vstack spacing
- UI polish round 11 (overnight R2): DraggablePanel and Modal — 10px rounded corners, zero borders, full-screen popup_shadow backdrop, removed crude drag handle lines from title bar. RCP selectable_selected brightened to #3A3A3C, hovered to #484848.
- UI polish round 12 (overnight R3 final): panel bg #252527→#2C2C2E (matches spec, better canvas/panel contrast), inspector padding 8→12px, inspector section headers 11→12px (FontSize::Medium), hierarchy panel padding 4→8px spacing 4→6px. Vision score: 7/10 — all spec compliance issues resolved. Remaining gaps are structural (empty states, panel dividers, asset browser sizing).
- UI polish round 13 (Droid duty): Modal backdrop alpha 0.5→0.7 for clear visibility. Theme grid now shows ✓ check mark on selected theme. Inspector sections (Transform, Type, Audio Source, Audio Listener) now collapsible with chevron indicators via section() widget. selectable_selected alpha 0.18→0.30.
- SceneSnapshot physics bug fixed: spawn_from_descriptor now restores RigidBody, ColliderShape, PhysicsMaterial, TriggerVolume, CollisionFilter — physics bodies survive play/stop cycles.
- Default scene path fix: added default_scene_path() resolving via CARGO_MANIFEST_DIR, eliminating test/runtime path drift. Both now target workspace-root assets/scenes/default.katla.
- Code health pass: cargo fmt/clippy/test all clean.

## In Progress

- PR #50 / #49 hardens Metal frame execution: requires the geometry/fullscreen/UI semantic pipeline, validates `hdr_color -> viewport_0 -> backbuffer` accesses before encoding, and propagates retained command-buffer GPU failures as typed renderer errors with native diagnostics.

## Upcoming / Blocked

- Typed image accesses and subresource ranges (#30).
- First-class buffer resources and dependencies (#31), followed by backend-neutral compute commands (#32) and one compiled synchronization plan (#33).
- Pass culling (#34), transient allocation/aliasing plus memoryless attachment selection (#35), real Metal frames in flight (#36), and synchronization/allocation/backend-trace diagnostics (#37).
- Remove the implicit Metal direct-to-drawable legacy path (#51) and add per-encoder validation diagnostics (#52).
- Add deterministic persistent pipeline caching and asynchronous warmup (#53).
- Replace the command path with Metal 4 after frame-slot/synchronization ownership is ready (#54), then move bindings and residency to argument tables/residency sets (#55).
- Execute Metal passes directly from the compiled graph plan instead of fixed semantic blocks (#56).
- Encode AppKit/CAMetalLayer thread affinity instead of blanket unsafe `Send`/`Sync` (#57).
- Replace synchronous shared-texture writes with format-aware staged uploads into private GPU storage (#58).
- Replace the stale Metal implementation plan with a verified current architecture reference (#59).
- Preferences theme grid: consider orange accent tint in addition to check mark for even more prominence.
- Asset browser sizing still needs work (structural gap from vision score 7/10).
- Empty states for panels (hierarchy, inspector with no selection).
- Panel dividers between docked panels.

<!-- Things planned but not started. Remove when started (move to In Progress). -->
