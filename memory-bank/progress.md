# Progress

What's been done and what's next. **Update this file when completing or starting work. Remove completed items to keep this lean.**

## Completed Recently

- Audited the render graph end-to-end across builder, compiler, frame submission, transient resources, Vulkan, Metal, and application graph construction. Merged the canonical dependency DAG (#21), platform isolation (#24), benchmark ownership fix (#26), and fail-fast builder validation (#28). Captured larger follow-up work as focused issues #30–#38 and #41 instead of a tracking epic.
- Fixed asset browser action safety: single clicks now select without immediately opening folders or previews, double-click activation is tracked per asset, context actions resolve the stored asset index, delete confirmation consumes the exact pending action, and deletion refuses empty paths plus the synthetic `..` entry. Added regression tests for click activation, stored confirmation actions, context target resolution, and parent-entry protection.
- Restored complete editor docking interaction: DockSpace now owns tab activation, tab drag/drop, and splitter dragging through the declarative global-input pass; the duplicate manual editor input path was removed. Nested splitter ratios use local split bounds, and tab moves preserve the exact dragged tab. Added regression tests for nested ratios and non-first-tab moves.
- Wired the Console toolbar: level buttons toggle their filters and Clear empties the shared log buffer through typed declarative actions.
- Fixed declarative input state regressions: Hierarchy, Asset Browser, and Console now filter from their live text-field `StateId` values, and `UiContext` preserves input consumption across multiple passes within a frame. Added tests for consumption accumulation and frame reset.
- Fixed inactive dock tabs rendering over the active panel: `EditorOverlayView` still builds every docked panel in a stable order to preserve positional state slots, but now only mounts the active tab from each `DockTree` leaf into the ZStack. Added a regression test for active-tab collection.
- Fixed vertical duplication of the 3D scene in the editor viewport: the tonemap pass rendered the panel-sized HDR into a separate panel-sized intermediate (`viewport_0`) then blitted it into the drawable's panel rect. Rendering the fullscreen tonemap triangle into the panel-sized intermediate caused the scene to be duplicated vertically (uv.y wrapped with period = half the target height). Fix: the tonemap now renders **directly into the drawable**, constrained to the panel rect via `set_viewport`+`set_scissor` (Metal viewport originY converted from top-down panel coords), eliminating the viewport_0 intermediate and the blit entirely. The drawable is cleared to the panel background before the tonemap loads it. Verified with gemma-4-12b: no duplication, scene is a single continuous 3D perspective.
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
- Pushed 49 commits to origin/main.

## In Progress

- Metal semantic frame scheduling and depth-prepass integration (#39), including cleanup of its temporary macOS validation workflow before merge.
- Metal renderer tests still abort the macOS test process on Objective-C exceptions; root-cause work is scoped in #38.

## Upcoming / Blocked

- Typed image accesses and subresource ranges (#30).
- First-class buffer resources and dependencies (#31), followed by backend-neutral compute commands (#32) and one compiled synchronization plan (#33).
- Pass culling (#34), transient aliasing (#35), real Metal frames in flight (#36), deterministic graph diagnostics (#37), and one graph-owned Metal editor viewport output (#41).
- Preferences theme grid: consider orange accent tint in addition to check mark for even more prominence.
- Asset browser sizing still needs work (structural gap from vision score 7/10).
- Empty states for panels (hierarchy, inspector with no selection).
- Panel dividers between docked panels.

<!-- Things planned but not started. Remove when started (move to In Progress). -->
