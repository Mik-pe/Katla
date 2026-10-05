# Rust editor target

Build a coherent modern Katla editor using the supplied Nova3D reference and
Reality Composer Pro as the visual direction. Keep Katla branding and fewer
controls. Preserve Odin on its separate branch and publish verified Rust work
to main.

Acceptance requires a compact app bar, aligned full-height side panels, a
dominant central viewport, Assets selected on arrival, consistent crisp icons
and type, smooth physical-pixel edges, restrained corners and surfaces, and
responsive layout without clipped controls. Material cards and the inspector
must generate visual previews from authored factors and imported PBR maps.
Transform, light and camera numeric inspector fields must accept mouse
scrubbing and keyboard edits, preserve selection, update the scene, support undo/redo, and
reject invalid values. Selection changes and undo must refresh every displayed
value. Native Metal readback and interaction checks must prove the complete
authoring path, alongside Rust tests, Clippy and format checks.

Compare rendered captures against the reference at the same logical size before
publication. This document records the Rust editor's visual direction and acceptance criteria.

The native walkthrough covers 30 behavioral checks, including material/prefab
workflows, grouped numeric undo/redo, degree/radian authoring, invalid-entry
recovery, keyboard focus, a narrower layout, and Mixer volume retention and
recovery. The pixel validator checks the six material cards, factor edits, history and imported-map cache invalidation
against real Metal readback. Run the commands in [visual design](editor_ui_design.md)
with Metal API validation enabled. Inspect the default, selected material,
preferences, console, numeric error and narrow-layout captures together at the
reference's logical scale. A bounded floor-silhouette regression checks the scene
AA path; it is not a universal rendering-quality score.
