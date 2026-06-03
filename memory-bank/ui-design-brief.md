# Katla UI Design Brief

> Not a copy of Reality Composer Pro — inspired by its quality bar and design language. Katla has its own layout and identity, but shares the same premium feel.

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

## The "Secret Sauce" Checklist
- [ ] Restraint of color (two accents, ~3% coverage)
- [ ] Generous vertical rhythm (32px rows)
- [ ] Grid stays out of the way (low contrast)
- [ ] Iconography discipline (consistent stroke weight, no fills)
- [ ] Single high-status CTA per region
- [ ] Depth through tonal layers, not borders
- [ ] Viewport dominates the window
