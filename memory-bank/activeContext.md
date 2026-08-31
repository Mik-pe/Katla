# Active Context

## Current Focus

- **Editor UI/UX polish pass (2026-08-31, uncommitted)** — judged screenshot
  pass over 5 ui-test states drove these fixes (all widget-layer unless noted):
  (1) Dock tab strip now caps multi-tab leaves at `tokens::TAB_MAX_WIDTH`
  (160px) stacked from the left — draw and hit-testing share
  `dock_space::tab_hit_width` so visible tabs are exactly the clickable tabs;
  single-tab leaves still fill the strip as panel headers. (2) ScrollView
  finally scrolls: taffy's automatic minimum size made scroll views
  overflow their panels instead of shrinking (`min_size.height = 0` on
  ScrollView, `flex_min_height(0.0)` builder on stacks that wrap one, and
  `flex_shrink` builders on TextField/HStack/VStack for fixed-height rows);
  the visible scroll range and the new 3px scrollbar thumb (drawn in
  ScrollView::draw_after_children) read taffy `content_size`, now surfaced as
  `TaffyNodeMap::content_sizes` and passed to `draw_after_children`/`post_layout`
  as UNCLAMPED child bounds — before this, wheel offsets were always reset to
  0 (scrolling silently never worked). `apply_colors` derives
  `scrollbar_handle(_hovered)` from `text_muted` at 0.35/0.55 alpha because
  the color_scheme! macro defaulted them to opaque black (invisible).
  (3) UiStyle gained `accent` (from ColorScheme.accent); tool buttons'
  selected state = accent bg + black fg (was check_mark_color bg — illegible
  per-theme). `ToolLabelButton` deleted (unused after gizmo toolbar went
  icon-only: `tool_button` + tooltips "Move (W)/Rotate (E)/Scale (R)",
  overlaid below the viewport label in editor_root). (4) Inspector: redundant
  Type section removed; "+ Add Component" expander lists
  `available_components` and emits new `EditorAction::AddComponent` →
  `SceneToolExecutor::execute(SceneOp::AddComponent)` with
  `agent::check_protected_entity` guard (now pub(crate)), undo-group recorded.
  (5) Default bottom dock ratio 0.64 → 0.74 (viewport dominates).
  Verified: workspace tests/clippy/fmt green, judge-passed screenshots.

- **Headless render visual audit (2026-08-31, uncommitted)** — screenshot pass
  over default/playground/UI states surfaced and fixed three renderer bugs:
  (1) billboard shaders ignored the model-matrix scale, so every gizmo
  rendered as a fixed 1×1 world-unit quad (`compute_gizmo_scale` was dead) —
  billboards now scale from the matrix in `billboard.wgsl` and
  `billboard_depth.wgsl`; (2) ForkAwesome fire glyph overflowed the 64px icon
  canvas, bleeding a bright row onto billboard bottoms — the rasterizer now
  scales glyphs to fit (`billboard_icons.rs`, regression test
  `test_icons_fit_canvas_without_clipping`); (3) the long-standing "pale strip
  at viewport top" was the sky pass rendering Y-flipped relative to stored
  geometry (sky bypasses the projection, so the display flip that geometry
  incorporates hit only the sky; the below-horizon gradient landed at the
  viewport top) — fixed by flipping the sky's clip-space Y in `sky.wgsl`.
  Also: gizmo/debug-overlay draws now excluded from the shadow draw list
  (arrow-shaped shadow slivers appeared whenever a gizmo was visible);
  `compute_gizmo_scale`'s viewport_height input is the logical panel height
  and is CORRECT — don't re-investigate it. Verified via headless renders:
  uniform 40px gizmos, no band, clean shadows; workspace tests green.
- **Editor UI: preferences modal redesign (2026-08-31, uncommitted in tree)**
  — Preferences is a focused `Modal` with icon sidebar (Appearance / Viewport
  / Audio / AI; General removed) over a 0.6-alpha scrim; theme picker is a
  compact swatch list; UI scale is discrete 80–130% segments. Widget-layer
  additions: `Widget::press_action` (Enter/Space activates focused buttons —
  text inputs must keep returning None), `Modal::wants_global_input` +
  self-drawn title bar/scrim/shadow, `theme_swatch` replaces the deleted
  `theme_preview`, `LabeledSlider::value_display(multiplier, suffix)`, tokens
  `MODAL_TITLE_HEIGHT`/`RADIUS_WINDOW`/`MODAL_CLOSE_SIZE`; orphaned `TabBar`
  widget deleted. CRITICAL convention reaffirmed: every state slot used by any
  preferences category is allocated unconditionally in fixed order at the top
  of `PreferencesView::build` — conditional `ctx.state()` cross-assigns types
  between tabs. Hit-testing for the centered modal lives in
  `EditorUI::is_click_on_floating_panel` using `PREFERENCES_WIDTH/HEIGHT`
  (not `DraggablePanelState::bounds`). Verified: workspace tests, clippy,
  headless ui-test 5 states, judge-passed screenshots in
  `.zcode/ui-screenshots/`.
- **Particles render on Metal (landed 970e07d9, 2026-08-30)** — subsystem
  un-gated, compute dispatched inline pre-pass, `PassKind::Particles` record
  encoded onto hdr_color, emitter sync backend-agnostic via
  `ParticleEmitterDriver`. Known gaps: the editor `ResetParticleSystem` action
  and `game_state.rs`/editor entity-destruction cleanup still target the
  Vulkan system only (Metal emitters survive those actions until someone
  routes them through the driver trait); `MetalParticleSubsystem::reset_all`
  was deleted rather than wired — restore from git history if the editor
  action goes cross-backend.
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

- Docs: `docs/metal_backend.md` is now the Metal architecture reference
  (2026-08-30, commit 4bcb207a); the pre-implementation plan is archived with a
  superseded banner. Issues #51 and #57 closed after tip verification
  (CI 33239691228 at 59e0a50c); #53 and #58 carry state-of-play comments.
- PRE-EXISTING on main: clippy `too_many_arguments` in
  `katla_gfx/src/metal/shadow.rs` `encode_cascade_draws` (8 params).

## Temporary Boundaries

- **Metal frame-render bridge (committed 0fb87573, 2026-08-31):** the
  frame-slot refactor had dropped the per-slot uniform preparation from the
  graph-driven render_frame (Metal validation killed fs_main, "missing Buffer
  binding at index 0"). render_frame now merges all draw_lists and calls
  GpuRenderer::execute_draw_calls before encoding, and the fullscreen record
  re-binds slot 0. Proper owner: fold these into the slot-ownership design
  (#36) and replace the bridge with graph-declared buffer resources (#31).
- Billboard gizmos are excluded from shadow and outline passes on both
  backends (committed 59cce2a3); the app's shadow draw list additionally
  filters billboard/gizmo/physics-debug overlay materials (2026-08-31) because
  gizmos must not cast shadows and the shadow/outline vertex paths have no
  billboarding math.
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

## Editor UI Redesign (landed e4ff955a, 2026-08-30)

- Design tokens now live in `katla_ui::tokens` — ALL chrome heights (app bar
  38, tab bar 30, status 24, control 28, tree row 26, compact 24), 4px
  spacing scale, radii, divider/splitter metrics. UiStyle::default_dimensions
  and WidgetDefaults::DEFAULTS seed from it; do not reintroduce raw magic
  numbers for chrome geometry.
- Dock tab strip colors derive in UiStyle::apply_colors from scheme fields
  (tab_active_bg = background_light, tab_border = separator fallback
  panel_border) — theme authors get coherent tabs for free; per-theme tab
  overrides are unnecessary.
- New primitives: `tool_button`/`tool_label_button` (segmented editor tools,
  accent selected state). Inactive tool labels use text_secondary, never
  text_disabled — inactive must stay distinguishable from unavailable.
- `HierarchyAction::ToggleExpanded` exists; hierarchy click expands AND
  selects. Expand state remains `HierarchyState.expanded_entities`.
- StatusBarData dropped frame_count/selected_count (status bar no longer
  shows Frame counter / asset selection; ColorScheme label removed — it was
  a setting, not runtime status).
- Splitter visuals are 1px lines inside 6px hit targets (SplitInfo.line_rect);
  active-tab hairline gap painted by DockSpace::draw_after_children.
- Uncommitted collaborator WIP untouched: their style.rs (RCP palette) and
  layout.rs (viewport tab-bar offset) hunks remain in the working tree only.

## Editor UI Polish (landed 93a80165, 2026-08-30)

- Control primitives (Button/ImageButton/TextField/ToolButton) now size from
  `katla_ui::tokens` (28px), have hover feedback, and carry `.tooltip()` —
  `UiContext::defer_tooltip` renders them at frame end. Icon-only actions
  (play cluster, asset browser nav) must keep tooltips.
- `panel_body()` = docked panel surface: reserves the dock tab strip as header
  spacing, draws NO title (dock tab bar is the header). `panel()` draws its
  title + 1px divider for standalone use. Docked views must use panel_body.
- CRITICAL builder convention: ALL editor views share ONE positional
  StateArena slot counter (per node). Any conditional `ctx.state()` shifts
  every later view's slots frame-to-frame → type-confusion panics (was: crash
  on entity selection via inspector section slots). Reserve all slots
  unconditionally in a fixed order at the top of each Build::build.
- Chrome-reservation rule: a widget that draws chrome (Section header,
  TabBar strip, DraggablePanel title, Grid implied by child count) MUST
  reserve that space in `layout_style`. Grid must capture child count at
  construction (child_widgets drains before taffy styles the node).
- ScrollView autoscroll: `.auto_scroll(pin_id)` — pins to bottom as content
  grows; wheel up detaches, wheel down re-engages; `post_layout` widget hook
  (tree.rs `apply_post_layout`) clamps offsets after layout.
- Inspector shows entity name/type header; console follows logs; asset
  browser has an up button (`AssetBrowserState::can_go_up` guards empty
  parent at the resources root).
- Verified via `cargo run -p game -- --headless --ui-test <dir>` (5 states,
  judge-passed at 1280x720 logical). The formerly-uncommitted collaborator WIP
  (gfx frame-slot bridge, RCP palette, shadow/billboard fixes) landed
  2026-08-31 as commits 59cce2a3..4519a06d.
