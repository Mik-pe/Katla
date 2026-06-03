# Active Context

What is being worked on right now. **Update this file when starting or finishing a task. Remove entries when work are complete.**

## Current Work

- **UI polish round 3 complete** — fixed gizmo button clipping (positioned at viewport offset), auto-numbered duplicate entity names (Sphere.001), added per-type mesh icons, added hover state to selectable/radio widgets, added status bar separators, fixed viewport label alignment, improved inspector empty state text.

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
