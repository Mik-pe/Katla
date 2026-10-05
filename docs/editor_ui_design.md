# Editor visual design

These are design targets for editor UI work, not a claim that every current screen meets them. Theme/widget source defines the rendered values. Read the [declarative architecture](declarative_ui_design.md) for implementation contracts.

## Core Principles

### 1. Restraint of Color
Two accent colors used in ~3% of pixel area. Most of the UI is neutral dark. Premium UIs are defined by what they DON'T use.

- **Base**: Neutral dark `#1A1A1C`–`#303034`, very slightly cool
- **Primary accent**: Orange `#F79545` — for add actions, active elements, CTAs
- **Secondary accent**: Cyan `#5AC8FA` — for 3D selection gizmos, live/active states
- **Text**: `#F0F0F5` (values), `#A8A8B0` (labels), `#888890` (muted)

### 2. Generous Spacing
- Inspector rows: ~28-32px height
- Section spacing: 16px vertical air
- Panel padding: 12px
- Between fields: 6px
- The viewport occupies the central column above Assets — content over chrome

### 3. Depth Through Layers, Not Borders
- Panel backgrounds are 3-5% lighter than canvas — "rooms within rooms"
- No visible borders. Use tonal shifts instead of 1px lines
- Selection states use background fills, not outlines
- Subtle top-to-bottom gradient per panel (barely perceptible)

### 4. Visual Hierarchy
1. **Selected 3D object** (bright against dim viewport)
2. **Inspector properties** (bold values, muted labels)
3. **Scene hierarchy** (context)
4. **Toolbar** (slightly darker, less contrasted — doesn't compete)

### 5. Typography
- Font: Roboto (Katla's current font) at SF Pro quality
- Section headers: 12-13px, semibold, white, with disclosure chevron
- Field labels: 11px, regular, `#8E8E93` muted
- Field values: 11px, regular, white, tabular/monospace figures
- Unit suffixes: 10px, `#6E6E72`, dimmer than values

### 6. Micro-Details
- Corner radii: 6px (small controls), 8-10px (cards/panels)
- Borders: `rgba(255,255,255,0.06)` — almost invisible
- Icons: Line-art at 1.25px stroke, consistent weight, no fills
- One high-status CTA per region (Play button, Add Component, etc.)
- Collapsible sections with rotated chevron
- Numeric inputs: rounded rects with subtle underline fill, "carved in" feel

### 7. Viewport
- Dark background `#1C1C1E`
- Low-contrast perspective grid (6-8% opacity, not 20%)
- Grid visible when needed, invisible when not
- Floating translucent toolbar (blurred background)

## What Katla Does DIFFERENTLY
- Katla is a general-purpose game engine, not just visionOS
- Katla has its own panel layout (not necessarily 4-panel DCC)
- Katla uses Vulkan/Metal, not RealityKit
- Katla's own identity while sharing the quality language


## Inspiration and layout

Reality Composer Pro inspires restraint, spacing, layered surfaces and a dominant
viewport. Katla keeps its own general-purpose engine identity and Vulkan/Metal
rendering, with independently dockable panels rather than a fixed four-panel DCC.
The supplied Nova3D image refines this into full-height hierarchy and inspector
panels around a central column, with Assets below the viewport. Katla starts with
hierarchy 16.5%, inspector 22%, and Assets at 28% of the central column height.
Assets is the selected tab; Console and Mixer remain available beside it.
Docking still lets users change this arrangement.

The 38px top bar shows Katla, four compact menus, the scene title and play controls.
The title hides below 720px to preserve menu and play-control space. Menu painting
and click regions share measured glyph widths; separators occupy 8px and consume
clicks without passing them through to the scene. Icon actions use quiet surfaces
until hovered. Dark and Reality Composer Pro share Katla's neutral surface palette,
orange actions and cyan selection/focus. Fresh preferences select this palette and
hide viewport statistics. Inspector ordering puts Transform before Material, then
other components; material properties precede presets. Panel focus follows the
active dock leaf, including the full-height sidebars. Viewport light/emitter icons
use a restrained 24px screen-space size before user scaling.

The reference uses SF Pro, green selection and blue secondary controls. Katla's
chosen identity uses Roboto, orange actions and cyan selection. Reference details
include 4pt spacing increments, 6px field gaps, 12px panel padding and 6–10px corner
radii. Borders should be subtle rather than form a grid around every control.

## Editor interaction contract

The title displays the loaded scene name and an asterisk for unsaved authored
changes. Animation timing and derived clip durations do not mark the scene dirty.
Open and Save As show a path entry dialog for `.katla` files. Save uses the current
path and prompts when the scene is untitled. Save As asks before replacing an
existing file. New, Open and window close offer Save, Discard Changes and Cancel
when the document has edits. Errors appear in a modal and keep the current scene.
File operations require stopping play mode before changing the editor document.

Shortcuts use Command on macOS and Control elsewhere: S saves, Shift+S saves as,
O opens, N creates a scene, Z undoes, Shift+Z or Y redoes, and comma opens
preferences. They use the same deferred actions as the menus. Text entry and
scene modals capture keys; held-key repeats do not repeat document commands.
Within the viewport, F focuses selection, W/E/R choose transform mode and Escape
clears selection. Game controls also require viewport focus during play; losing
window focus releases held controls.

Hierarchy search includes matching children beneath collapsed ancestors.
Empty undo/redo menus are disabled. Camera speed, snap-to-grid and grid spacing
persist in preferences. Translation snapping applies only to manipulated axes.
Missing preference files are normal on first launch; invalid or nonfinite values
fall back to usable bounds.

Mesh selection exposes a Material section with an sRGB swatch, six named PBR
starting points, and live RGBA, metallic, roughness and occlusion sliders.
Each pointer gesture creates one editor undo step; presets create one step each.
The same validated factors are editable in batches through the `material` agent
tool. These per-object multipliers preserve model textures and pipeline handles
and persist in the scene document. See [agent authoring](agent-authoring.md) for
color semantics, search and room recipes.

`cargo run -- --interaction-test /tmp/katla-interactions` drives real UI
hit testing and native viewport picking. The walkthrough clicks presets, drags
material sliders outside their rows, uses Edit menu undo/redo, and collapses the
material section to add and remove a component. It writes screenshots and a
`receipt.json`, and exits with an error for failed or incomplete checks.
