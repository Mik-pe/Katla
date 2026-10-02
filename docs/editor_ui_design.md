# Editor visual design

These are design targets for editor UI work, not a claim that every current screen meets them. Theme/widget source defines the rendered values. Read the [declarative architecture](declarative_ui_design.md) for implementation contracts.

## Core Principles

### 1. Restraint of Color
Two accent colors used in ~3% of pixel area. Most of the UI is neutral dark. Premium UIs are defined by what they DON'T use.

- **Base**: Neutral dark `#1E1E1E`–`#2A2A2A`, very slightly cool
- **Primary accent**: Orange `#F79545` — for add actions, active elements, CTAs
- **Secondary accent**: Cyan `#5AC8FA` — for 3D selection gizmos, live/active states
- **Text**: White `#FFFFFF` (values), `#8E8E93` (labels), `#6E6E72` (disabled)

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
A useful reference balance is hierarchy 16%, viewport 60–65%, inspector 22%, with
a bottom project/timeline area around 25% height. Treat these as visual reference
ratios, not mandatory panel geometry.

The reference uses SF Pro, green selection and blue secondary controls. Katla's
chosen identity uses Roboto, orange actions and cyan selection. Reference details
include 4pt spacing increments, 6px field gaps, 12px panel padding and 6–10px corner
radii. Borders should be subtle rather than form a grid around every control.
