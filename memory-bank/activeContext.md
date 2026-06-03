# Active Context

What is being worked on right now. **Update this file when starting or finishing a task. Remove entries when work are complete.**

## Current Work

- **Editor color scheme updated to design brief palette** — `ColorScheme::dark()` now uses neutral cool-dark palette (`#1E1E1E` canvas, `#2A2A2A` panels, `#F79545` orange accent, `#5AC8FA` cyan secondary). Metal viewport clear color changed from Catppuccin `#181825` to `#1E1E1E`.

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
