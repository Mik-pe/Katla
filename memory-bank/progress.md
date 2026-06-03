# Progress

What's been done and what's next. **Update this file when completing or starting work. Remove completed items to keep this lean.**

## Completed Recently

- Contained 3D viewport within dock panel — viewport_panel_rect restricts Metal rasterization + blit to panel bounds, clears drawable to Catppuccin Mocha base color
- Fixed HiDPI text rendering scale factor bugs — `glyph.physical()` now uses scale_factor for crisp 2x rasterization; position calculation uses all-logical coordinates (no more `* scale` on already-logical cached offsets and run.line_y)

## In Progress

(Nothing in progress — add entries when work begins.)

## Upcoming / Blocked

<!-- Things planned but not started. Remove when started (move to In Progress). -->
