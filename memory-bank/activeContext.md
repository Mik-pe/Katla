# Active Context

What is being worked on right now. **Update this file when starting or finishing a task. Remove entries when work are complete.**

## Current Work

- **Default scene has physics entities + active physics on load** — Ground is a static box collider (10x0.05x10 half-extents, friction 0.7, restitution 0.1). Top 2 rows of the 5x5 PBR grid are dynamic sphere colliders (10 spheres, radius 0.4, friction 0.5, restitution 0.3). CenterCube is dynamic with box collider [0.5,0.5,0.5] (lifted to y=2.0 so it falls). CyanSphere is dynamic sphere (radius 0.7) lifted to y=3.0. MagentaCylinder is dynamic capsule collider (half_height=0.375, radius=0.5) lifted to y=3.0. LimeTorus stays visual-only (torus trimesh collider is a separate TODO).
- **PhysicsActive(true) at builder init** — both headless (line ~920) and windowed (line ~1293) now default to active, so the scene plays its falling demo on load. User can still pause via play/pause toggle. Trade-off: SceneSnapshot::capture doesn't preserve physics components (separate bug in spawn_from_descriptor), so the play/stop cycle loses physics bodies on stop. Acceptable for now.

## Architecture Note

- Panel widget now reserves top padding via `header_height` (28px by default) so content renders below the DockSpace tab bar. The DockSpace draws tab bars as an overlay on top of panels, so panels must offset their content.
- `TAB_BAR_HEIGHT` constant (28.0) defined in `editor_root.rs`, matching `DockSpace::tab_bar_height`.
- DockSpace tab bar now uses `tab_text` (inactive, #8E8E93) and `tab_active_text` (active, #FFFFFF) from UiStyle instead of generic `text_color`.
- TabBar widget (preferences) uses the same proper theme colors.

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
- UI load op changed to LoadOp::Load to preserve 3D scene underneath
- Font atlas properly destroyed before recreation (prevents slot thrashing)
- Draw list now preserves submission order across instance/vertex batch types
- Metal instanced pipeline now uses bind_graphics_pipeline (not raw setRenderPipelineState)
- `STATUS_BAR_HEIGHT` defined once in `editor_root.rs`, re-exported from `declarative/mod.rs`
- Metal canvas clear color uses linear value (0.013) so it appears as #1E1E1E on sRGB framebuffer (BGRA8Unorm_sRGB interprets clears as linear)
- DockSpace tab bars use `tab_active_bg`/`tab_inactive_bg`/`tab_hover_bg` from UiStyle, not generic button/selection colors
- UI shader applies srgb_to_linear() in vertex shader — hex colors round-trip correctly through sRGB framebuffers
- `ToolbarDrawCtx` now carries `error` color for stop button — no hardcoded colors in toolbar
