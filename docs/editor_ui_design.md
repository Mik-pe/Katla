# Editor visual design

These are design targets for editor UI work, not a claim that every current screen meets them. Theme/widget source defines the rendered values. Read the [declarative architecture](declarative_ui_design.md) for implementation contracts.

## Core Principles

### 1. Restraint of Color
Two accent colors used in ~3% of pixel area. Most of the UI is neutral dark. Premium UIs are defined by what they DON'T use.

- **Base**: Neutral dark `#1E1E1E`–`#2A2A2A`, very slightly cool
- **Primary accent**: Orange `#F79545` — for add actions, active elements, CTAs
- **Secondary accent**: Cyan `#5AC8FA` — for 3D selection gizmos, live/active states
- **Text**: White `#FFFFFF` (values), `#A8A8B0` (labels), `#6E6E72` (disabled)

### 2. Generous Spacing
- Inspector rows: ~28-32px height
- Section spacing: 16px vertical air
- Panel padding: 12px
- Between fields: 6px
- The viewport dominates (~60-65% of window) — content over chrome

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
- Field labels: 11px, regular, `#A8A8B0` muted
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
A useful reference balance is hierarchy 16%, viewport 60–65%, inspector 22%, with
a bottom project/timeline area around 25% height. Treat these as visual reference
ratios, not mandatory panel geometry.

The reference uses SF Pro, green selection and blue secondary controls. Katla's
chosen identity uses Roboto, orange actions and cyan selection. Reference details
include 4pt spacing increments, 6px field gaps, 12px panel padding and 6–10px corner
radii. Borders should be subtle rather than form a grid around every control.

## Workspace chrome

The default workspace opens Assets in the bottom dock, with Console and Mixer
available as sibling tabs. The bottom dock occupies 26% of the central stack;
the viewport receives 74%. Existing saved dock layouts retain their chosen tabs
and split ratios. Hierarchy and Inspector remain independent side docks.

File, Edit and View use quiet text controls in a compact top toolbar. Undo/Redo
and Play/Pause/Stop use the existing ForkAwesome icon catalogue and its native
font, with descriptive hover tips and retained keyboard focus. File/Edit menus
show the actual platform shortcuts. Menu items use 28px rows, tonal hover fills
and separators; long menus scroll inside the window. The top toolbar hides
history controls in narrow windows, where Undo/Redo remain in Edit.

Standard buttons, quiet toolbar controls and the region's primary action have
distinct semantic styles. Hierarchy and asset selection use restrained cyan
fills. Single-panel dock headers stay plain; multi-tab docks indicate their
active tab with a small orange underline. Splitter gaps use the canvas tone.
Scrollbars appear only when content extends past the actual viewport.

The default appearance is the neutral `rcp` theme, with frame statistics off;
existing preferences retain their chosen theme and diagnostics. The `dark`
preset uses the same neutral editor palette. Primary text is `#F0F0F5`, secondary
text `#A8A8B0`; panel, control and hover surfaces are `#252528`, `#303034` and
`#3D3D41`. Orange identifies actions, cyan identifies scene selection.

Inspector starts with the selected object's name, then collapsible named
component sections. Transform groups position and scale into X/Y/Z rows;
rotation preserves explicit X/Y/Z/W quaternion semantics. Numeric values align
opposite their labels and keep captured drags and shared Undo/Redo. Component
removal uses a labelled icon control. Empty selection provides a short
instruction rather than a raw technical message.

## Responsive panel layout

Preferences stays within the window with an 8px margin and scrolls its content.
Theme choices reflow to one column when width or interface scale requires it.
Material presets share equal cell widths and become one column in a narrow
inspector; channel labels move above sliders there. Mixer channel strips reflow
and scroll within their dock panel. The scene title truncates with a tooltip,
and hides when menus leave insufficient space.
Console messages wrap within the dock and its toolbar reflows in narrow panels.
Asset breadcrumbs occupy a separate row and compact long paths. Scene dialogs
fit within the window, with full-width path entry and scrollable long messages.
Text fields clip their content and reveal the trailing input while focused.

## Editor interaction contract

The title displays the loaded scene name and an asterisk for unsaved authored
changes. Animation timing and derived clip durations do not mark the scene dirty.
Open and Save As show a path entry dialog for `.katla` files. Save uses the current
path and prompts when the scene is untitled. Save As asks before replacing an
existing file. New, Open and window close offer Save, Discard Changes and Cancel
when the document has edits. Errors appear in a modal and keep the current scene.
File operations require stopping play mode before changing the editor document.
Visible modals capture pointer input across both the dialog and its scrim;
clicks and wheel input cannot select objects or move the camera behind them.

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

Texture images and sampling form a separate collapsible section. Five effective
image previews select albedo, normal, metallic/roughness, occlusion or emission.
Drag an image from the asset browser onto a role, or use Browser image or an
explicit Resource/Scene/File source. Neutral clears just that image; Original
restores its imported binding. Image edits preserve factors and sampling.
The selected role exposes available UV sets, offset, rotation in radians,
scale, independent min/mag/mip filtering, wrap U/V and anisotropy. Missing UV
sets are disabled. Each slider gesture remains one scoped undo step.
Source choices and asset buttons stack in narrow panels. Save material captures
the effective surface to a project `.katmat`; Apply material copies the entire
surface as one undo step. Double-clicking a `.katmat` in the browser applies it
to the selected mesh. Invalid edits show the existing error dialog.

`odin run tools/build -- run -- --interaction-test /tmp/katla-interactions` drives real UI
hit testing and native viewport picking. The walkthrough clicks presets, drags
material sliders outside their rows, uses Edit menu undo/redo, and collapses the
material section to add and remove a component. It then drags an image onto a
role, restores neutral/original images, edits filtering and UVs after scrolling,
and saves/applies a reusable material. It writes screenshots and a
`receipt.json`, and exits with an error for failed or incomplete checks.
The [historical Rust material inspector evidence](material-inspector-study/README.md)
records the accepted walkthrough and its validation scope.
