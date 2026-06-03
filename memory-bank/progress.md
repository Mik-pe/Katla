# Progress

What's been done and what's next. **Update this file when completing or starting work. Remove completed items to keep this lean.**

## Completed Recently

- Fixed Panel content overlapping DockSpace tab bars — Panel now uses `header_height` (28px) as top padding in layout style, pushing content below tab bars
- Fixed gizmo buttons (Move/Rotate/Scale) clipped behind viewport tab bar — offset now includes `TAB_BAR_HEIGHT + 8px`
- Fixed inspector empty state hidden behind tab bar — automatically fixed by Panel top padding
- Fixed first hierarchy item clipped behind tab bar — automatically fixed by Panel top padding
- Fixed gizmo radio button text clipping — increased vertical padding from `text_size.y() + 10` to `+ 16`
- Improved per-type hierarchy icons — sphere→CIRCLE, cube→SQUARE, plane→SQUARE_OUTLINE, torus→CIRCLE_OUTLINE, cylinder→CUBE, default mesh→SQUARE
- Precached shape and scene object icons (CIRCLE, SQUARE, LIGHTBULB, SUN, FIRE, VOLUME_UP, etc.) in ForkAwesome font
- Fixed selection highlighting — changed selectable_selected from near-invisible `#3A3A3C` to accent color at 18% opacity
- Used default icon size (Medium) for hierarchy entity icons instead of Small

## In Progress

(Nothing in progress — add entries when work begins.)

## Upcoming / Blocked

<!-- Things planned but not started. Remove when started (move to In Progress). -->
