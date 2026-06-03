# Active Context

What is being worked on right now. **Update this file when starting or finishing a task. Remove entries when work are complete.**

## Current Work

- **UI polish round 5 complete** — fixed Panel content overlapping DockSpace tab bars (Panel now uses `header_height` as top padding in Taffy layout, default changed from 24→28 to match `tab_bar_height`), fixed gizmo buttons clipped behind viewport tab bar (offset now includes `TAB_BAR_HEIGHT + 8px`), fixed inspector empty state hidden behind tab bar, fixed first hierarchy item clipped behind tab bar.

## Architecture Note

- Panel widget now reserves top padding via `header_height` (28px by default) so content renders below the DockSpace tab bar. The DockSpace draws tab bars as an overlay on top of panels, so panels must offset their content.
- `TAB_BAR_HEIGHT` constant (28.0) defined in `editor_root.rs`, matching `DockSpace::tab_bar_height`.

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

- Headless mode uses scale_factor=2.0 and 2560x1440 offscreen texture (matches Retina)
- Headless mode uses the same `Application` code as windowed — no separate code paths
- Instance buffer binding uses byte offsets (not baseInstance) because Metal's instance_id ignores baseInstance
- UI load op changed to LoadOp::Load to preserve 3D scene underneath
- Font atlas properly destroyed before recreation (prevents slot thrashing)
- Draw list now preserves submission order across instance/vertex batch types
- Metal instanced pipeline now uses bind_graphics_pipeline (not raw setRenderPipelineState)
- `STATUS_BAR_HEIGHT` defined once in `editor_root.rs`, re-exported from `declarative/mod.rs`
- Metal canvas clear color extracted to `CANVAS_CLEAR_COLOR` constant (#1E1E1E)
- `ToolbarDrawCtx` now carries `error` color for stop button — no hardcoded colors in toolbar
