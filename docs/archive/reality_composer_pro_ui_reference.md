# Reality Composer Pro — UI Design Reference

> Comprehensive design reference extracted from Apple's Reality Composer Pro (RCP),
> based on WWDC sessions (2023-2024), the Deconstructed reverse-engineering project
> by elkraneo, Apple HIG documentation, and screenshot analysis.

---

## 1. Overall Layout Structure

RCP uses a **single-window, multi-panel layout** following Apple's pro-app convention
(similar to Xcode, Final Cut Pro, Logic Pro).

```
┌─────────────────────────────────────────────────────────────────┐
│  Toolbar (top, ~38px)          [Window title + app controls]    │
├──────────┬──────────────────────────────────┬───────────────────┤
│          │                                  │                   │
│ Hierarchy│        Viewport                  │   Inspector       │
│  Panel   │     (3D Scene View)              │    Panel          │
│ (~220px) │                                  │  (~280-320px)     │
│          │                                  │                   │
│          │                                  │   [Properties]    │
│          │                                  │   [Components]    │
│          │                                  │   [Materials]     │
│          │                                  │                   │
├──────────┴──────────────────────────────────┴───────────────────┤
│  Optional: Timeline / Audio Mixer / Shader Graph / Statistics   │
│  (bottom panel, collapsible, ~200px)                             │
└─────────────────────────────────────────────────────────────────┘
```

### Panel Structure

| Panel | Position | Width | Description |
|-------|----------|-------|-------------|
| **Hierarchy** | Left sidebar | ~220px | Scene tree with USD prims |
| **Viewport** | Center | Flexible | 3D scene with RealityKit renderer |
| **Inspector** | Right sidebar | ~280-320px | Properties, components, materials |
| **Timeline** | Bottom (optional) | Full width | Animation timeline (added in WWDC24) |
| **Toolbar** | Top | Full width | Floating controls, camera modes, lighting |
| **Project Browser** | Left (tab) | ~220px | File/library browser |

### Key Layout Principles

- Panels can be **shown/hidden** via View menu or keyboard shortcuts
- **Split view** dividers are thin (~1px) subtle borders
- Viewport takes **all remaining space** between side panels
- Bottom panels (Timeline, Audio Mixer, Shader Graph) share space and are tabbed
- All panels follow **consistent internal padding**: 8-12px

---

## 2. Color Palette (Dark Mode)

RCP uses the standard macOS dark mode system colors with additional pro-app specific
tones. Based on macOS NSColor semantic values and screenshot analysis:

### Background Colors

| Element | Hex | NSColor Equivalent | Usage |
|---------|-----|-------------------|-------|
| **Window background** | `#323232` / `rgb(50,50,50)` | `windowBackgroundColor` | Main window chrome |
| **Control background** | `#1E1E1E` / `rgb(30,30,30)` | `controlBackgroundColor` | Panel backgrounds, text fields |
| **Under page background** | `#282828` / `rgb(40,40,40)` | `underPageBackgroundColor` | Deeper background layer |
| **Sidebar / Hierarchy** | `#2A2A2E` | Slightly lighter than control bg | Left panel background |
| **Inspector panel** | `#2A2A2E` | Same as sidebar | Right panel background |
| **Viewport chrome** | `#1E1E1E` to `#323232` | Blended | Viewport overlay controls |
| **Toolbar background** | `#323232` | Window bg | Top toolbar area |
| **Text field background** | `#1E1E1E` | Control bg | Input fields |
| **Selected row bg** | `#0058D0` / `rgb(0,88,208)` | `selectedContentBackgroundColor` | Selected item highlight |
| **Unemphasized selected** | `#464646` / `rgb(70,70,70)` | `unemphasizedSelectedContentBackgroundColor` | Secondary selection |

### Text Colors

| Element | Hex | NSColor Equivalent | Usage |
|---------|-----|-------------------|-------|
| **Primary label** | `rgba(255,255,255,0.85)` | `labelColor` | Main text, panel headers |
| **Secondary label** | `rgba(255,255,255,0.55)` | `secondaryLabelColor` | Descriptions, metadata |
| **Tertiary label** | `rgba(255,255,255,0.25)` | `tertiaryLabelColor` | Disabled/placeholder text |
| **Quaternary label** | `rgba(255,255,255,0.1)` | `quaternaryLabelColor` | Very faint text |
| **Placeholder text** | `rgba(255,255,255,0.2)` | `placeholderTextColor` | Empty field hints |
| **Disabled text** | `rgba(255,255,255,0.2)` | `disabledControlTextColor` | Disabled controls |

### Accent / System Colors (Dark Mode)

| Color | Hex | Usage |
|-------|-----|-------|
| **System Blue (accent)** | `#0A84FF` / `rgb(10,132,255)` | Active states, selections, links |
| **Alternate selection** | `#0058D0` / `rgb(0,88,208)` | Selected items in lists |
| **System Green** | `#30D158` / `rgb(48,209,88)` | Success, positive states |
| **System Red** | `#FF453A` / `rgb(255,69,58)` | Errors, destructive actions |
| **System Orange** | `#FF9F0A` / `rgb(255,159,10)` | Warnings |
| **System Yellow** | `#FFD60A` / `rgb(255,214,10)` | Caution, highlights |
| **System Purple** | `#BF5AF2` / `rgb(191,90,242)` | Secondary accent |
| **System Teal** | `#64D2FF` / `rgb(100,210,255)` | Info, links |

### Border / Separator Colors

| Element | Hex | Usage |
|---------|-----|-------|
| **Separator** | `rgba(255,255,255,0.1)` | Panel dividers, table separators |
| **Opaque separator** | `#38383A` / `rgb(56,56,58)` | Solid borders |
| **Grid lines** | `rgba(255,255,255,0.1)` | Inspector property grid |

### Gray Scale (Dark Mode)

| Shade | Hex | Usage |
|-------|-----|-------|
| **systemGray** | `#98989D` / `rgb(152,152,157)` | Neutral mid-gray |
| **systemGray2** | `#636366` / `rgb(99,99,102)` | Darker mid-gray |
| **systemGray3** | `#48484A` / `rgb(72,72,74)` | Panel borders, subtle elements |
| **systemGray4** | `#3A3A3C` / `rgb(58,58,60)` | Recessed areas |
| **systemGray5** | `#2C2C2E` / `rgb(44,44,46)` | Grouped background |
| **systemGray6** | `#1C1C1E` / `rgb(28,28,30)` | Deepest background |

---

## 3. Typography

RCP uses **SF Pro** (Apple's system font) throughout, following macOS typography conventions.

### Font Sizes and Weights

| Element | Font | Size | Weight | Color |
|---------|------|------|--------|-------|
| **Window title** | SF Pro Display | 13px | Medium (500) | Primary label |
| **Panel header** | SF Pro Text | 11px | Semibold (600) | Primary label, uppercase |
| **Section header** | SF Pro Text | 11px | Semibold (600) | Primary label |
| **Property label** | SF Pro Text | 11px | Regular (400) | Secondary label |
| **Property value** | SF Pro Text | 11px | Regular (400) | Primary label |
| **Hierarchy item** | SF Pro Text | 12px | Regular (400) | Primary label |
| **Toolbar label** | SF Pro Text | 11px | Regular (400) | Secondary label |
| **Monospace values** | SF Mono | 11px | Regular (400) | Primary label |
| **Disclosure triangle** | SF Pro Text | 10px | — | Tertiary label |
| **Tooltip** | SF Pro Text | 11px | Regular (400) | Primary label on bg |

### Typography Hierarchy

```
PANEL HEADER      (11px, Semibold, uppercase tracking) ─── INSPECTOR, HIERARCHY
  Section Title   (11px, Semibold) ─── Transform, Material, Components
    Property      (11px, Regular, secondary color) ─── Position, Rotation, Scale
      Value       (11px, Regular, primary color) ─── 0.0, 1.0, 0.5
    Sub-section   (11px, Medium) ─── Nested groups
```

---

## 4. Spacing and Padding Patterns

### General Spacing

| Context | Value |
|---------|-------|
| Panel internal padding | 12px |
| Section spacing | 16px |
| Property row height | ~22-24px |
| Section header top padding | 8px |
| Section header bottom padding | 4px |
| Between property groups | 8px |
| Inspector horizontal padding | 12-16px |
| Toolbar item spacing | 8px |
| Button padding (horizontal) | 8-12px |
| Button padding (vertical) | 4-6px |
| Icon-to-label gap | 4-6px |
| Disclosure indent per level | 16px |

### Panel Widths

| Panel | Min | Default | Max |
|-------|-----|---------|-----|
| Hierarchy | 180px | 220px | 400px |
| Inspector | 240px | 300px | 500px |
| Timeline | Full width | 200px height | — |

---

## 5. Button and Control Styles

### Standard Buttons

- **Height**: ~24-28px
- **Corner radius**: 4-6px
- **Background**: `#3A3A3C` (normal), `#0A84FF` (accent/primary)
- **Text color**: Primary label (normal), White (on accent)
- **Border**: 1px `rgba(255,255,255,0.1)` or none
- **Hover**: Background lightens slightly (+10-15% luminance)
- **Active/pressed**: Background darkens slightly

### Toggle Buttons / Segmented Controls

- **Height**: ~24px
- **Corner radius**: 4px for group, 2px for individual
- **Inactive bg**: `#2C2C2E`
- **Active bg**: `#48484A` with subtle highlight
- **Text**: 11px SF Pro Text, secondary label when inactive

### Popup/Dropdown Menus

- **Height**: ~22-24px
- **Corner radius**: 4px
- **Background**: `#3A3A3C`
- **Border**: 1px `#48484A`
- **Chevron indicator**: Small triangle, right-aligned
- **Menu bg**: `#2C2C2E` (popover)
- **Menu item height**: ~22px
- **Menu item hover**: `#0A84FF` (system blue)
- **Menu corner radius**: 6px (popover)

### Sliders

- **Track height**: 2-3px
- **Track bg**: `#48484A`
- **Track fill**: `#0A84FF` (active)
- **Thumb**: 14px circle, white with subtle shadow
- **Thumb hover**: Slight scale up

### Text Input Fields

- **Height**: ~22px
- **Corner radius**: 4px
- **Background**: `#1E1E1E` (slightly darker than panel)
- **Border**: 1px `#48484A`, `#0A84FF` on focus
- **Text**: 11px SF Pro Text, primary label
- **Number fields**: SF Mono for numeric values
- **Editable text**: Cursor with blue accent color

### Color Well / Color Picker

- **Size**: 22x22px square with 4px corner radius
- **Border**: 1px `#48484A`
- **Preview**: Filled with current color
- **Click**: Opens system color picker

### Checkbox / Toggle

- **Size**: 14x14px
- **Unchecked**: `#48484A` border, transparent fill
- **Checked**: `#0A84FF` fill, white checkmark
- **Corner radius**: 3px

---

## 6. Hierarchy / Scene Graph Panel

### Structure

The hierarchy panel displays the USD scene tree:

```
▼ Root
  ▼ Cube
    ▼ DefaultMaterial
      DefaultSurfaceShader
  ▶ Sphere
  ▶ Light
  ● Model_Sorting_Group
```

### Visual Design

- **Background**: `#2A2A2E` (sidebar standard)
- **Row height**: ~22-24px
- **Indent**: 16px per level
- **Disclosure triangle**: 10px, pointing right (collapsed) or down (expanded)
- **Icon**: Small entity type icon (16px), left of name
  - Xform: folder-like icon
  - Mesh: cube/geometry icon
  - Light: sun/bulb icon
  - Material: sphere icon
  - Component: gear/puzzle icon
- **Selected row**: `#0058D0` background with white text
- **Hover row**: `rgba(255,255,255,0.05)` background
- **Rename**: Inline text field with blue border
- **Context menu**: Standard macOS dark context menu

### Hierarchy Toolbar (Bottom)

Small toolbar at bottom of hierarchy panel:
- **Add button (+)**: Adds new entity/primitive
- **Delete button (-)**: Removes selected
- **Filter field**: Small search/filter input
- Buttons are ~20px icon-only buttons

### Insert Menu (Add Component)

Popover from the "+" button showing categorized components:
- **Categories**: General, Audio, Lighting, Physics
- **Row height**: ~24px
- **Icon + text** layout
- **Search field** at top
- **Background**: `#2C2C2E`

---

## 7. Viewport Chrome

### Floating Toolbar

A floating toolbar in the viewport provides camera and scene controls:

- **Position**: Top-center of viewport, with slight offset
- **Background**: Semi-transparent dark (`rgba(30,30,30,0.85)`) with blur
- **Corner radius**: 8px
- **Height**: ~32-36px
- **Content**: Icon buttons for:
  - Camera mode (Orbit, Fly, Pan)
  - Frame selected / Frame all
  - Grid toggle
  - Environment lighting popover
  - Debug view modes

### Environment Lighting Popover

Accessible from the floating toolbar:
- **Background presets**: Thumbnail grid of HDR environments
- **Rotation**: Slider (0-360°)
- **Exposure**: Slider (-3 to +3 EV)
- **Background toggle**: On/Off for skybox
- **Popover size**: ~280px wide

### Viewport Grid

- **Color**: Subtle gray, `rgba(255,255,255,0.08-0.12)`
- **Major lines**: Slightly brighter
- **Axis indicators**: Red (X), Green (Y), Blue (Z)
- **Origin indicator**: Small axis cross at 0,0,0

### Gizmo / Transform Handles

- **Translate**: Three arrows (RGB for XYZ)
- **Rotate**: Three circles
- **Scale**: Three bars with end caps
- **Colors**: Red (X), Green (Y), Blue (Z)
- **Hover highlight**: Brighter shade of axis color
- **Active axis**: Yellow/golden highlight

### Camera History

- Small forward/back buttons in viewport toolbar
- Shows preset camera positions
- Accessible from .rcuserdata metadata

---

## 8. Inspector Panel

### Layout

The inspector is a vertical scrollable panel with collapsible sections:

```
┌─────────────────────────┐
│ ENTITY NAME    [icon]    │  ← Header with entity name
├─────────────────────────┤
│ ▼ Transform             │  ← Collapsible section
│   Position  X  Y  Z     │
│   Rotation  X  Y  Z     │
│   Scale     X  Y  Z     │
├─────────────────────────┤
│ ▼ Material              │
│   [Material binding]    │
│   Surface shader        │
├─────────────────────────┤
│ ▼ Components            │
│   [+ Add Component]     │
│   ● Accessibility       │
│     isEnabled  [✓]      │
│     label  [________]   │
│     value  [________]   │
│   ● Billboard           │
│   ● Opacity             │
├─────────────────────────┤
│ ▼ Bindings              │
│ ▼ References            │
│ ▼ Variants              │
└─────────────────────────┘
```

### Inspector Sections

Each section has:
- **Header**: 11px semibold, with disclosure triangle
- **Content**: Indented 0px (flush left), with 12px horizontal padding
- **Separator**: 1px `rgba(255,255,255,0.06)` between sections
- **Collapse animation**: Smooth slide

### Property Rows

- **Layout**: Label (left) + Value (right) OR Label above Value
- **Label width**: ~40% of panel width
- **Value alignment**: Right-aligned
- **Number inputs**: Small text fields with drag-to-adjust behavior
- **XYZ triplets**: Three small fields side by side
- **Row spacing**: 2-4px between rows

### Add Component Button

- **Style**: Outlined button with "+" icon
- **Text**: "Add Component"
- **Action**: Opens categorized popover menu
- **Categories grouped by**:
  - General (Accessibility, Billboard, Opacity, etc.)
  - Audio (AudioLibrary, etc.)
  - Lighting (ImageBasedLight, etc.)
  - Physics (Collision shapes, etc.)

---

## 9. Timeline View (WWDC 2024 Addition)

Added in the WWDC24 version of RCP:

- **Position**: Bottom panel, collapsible
- **Background**: `#1E1E1E` (control bg)
- **Ruler/ticks**: Top ruler with time markings
- **Track rows**: ~24px height each
- **Keyframe diamonds**: Small diamond shapes, colored by type
- **Playhead**: Vertical line, red/orange accent
- **Scrubber**: Draggable playhead with time display
- **Track labels**: Left side, entity/property names

---

## 10. Design Principles

### Apple's Pro App UI Principles (observed in RCP)

1. **Deep Minimalism**
   - No unnecessary ornamentation
   - Content-first design; chrome recedes
   - Muted color palette lets 3D content stand out
   - Thin borders and separators, never heavy

2. **Semantic Color Usage**
   - Accent blue (`#0A84FF`) only for interactive/active states
   - Red only for destructive actions or errors
   - Green only for positive states
   - Gray tones carry 90% of the visual weight

3. **Vibrancy and Translucency** (subtle)
   - Toolbar uses semi-transparent background with blur
   - Popovers have subtle backdrop blur
   - Not as prominent as iOS vibrancy

4. **Consistent Hierarchy**
   - Three-level gray depth: window > panel > controls
   - Text hierarchy: primary > secondary > tertiary
   - Section headers are visually distinct but not dominant

5. **Functional Density**
   - Compact layout optimized for professional workflows
   - Dense but not cluttered; consistent spacing
   - Property editors pack lots of info into small space
   - Inspector scrolls vertically, never tabs (mostly)

6. **Responsive Feedback**
   - Hover states on all interactive elements
   - Clear selection states (blue highlight)
   - Smooth collapse/expand animations
   - Subtle transitions on state changes

7. **Platform Consistency**
   - Follows macOS dark mode conventions exactly
   - Uses NSColor semantic colors throughout
   - Standard macOS controls where possible
   - Keyboard shortcuts follow macOS conventions

8. **Progressive Disclosure**
   - Complex features hidden in collapsible sections
   - "Add Component" uses categorized popover
   - Debug views optional via toolbar
   - Timeline panel can be hidden

---

## 11. Additional UI Elements

### Status Bar / Statistics Panel

- **Position**: Bottom or floating panel
- **Content**: FPS, triangle count, draw calls, memory
- **Style**: Monospace font, small text, secondary label color
- **Background**: Same as panel bg

### Shader Graph

- **Position**: Opens in main view area (replaces or overlays viewport)
- **Node style**: Dark rounded rectangles with colored headers
- **Connection lines**: Curved bezier paths, colored by data type
- **Port indicators**: Small colored circles
- **Background**: Subtle dot grid pattern

### Audio Mixer

- **Position**: Bottom panel (tabbed with Timeline)
- **Channel strips**: Vertical sliders with level meters
- **Level meter colors**: Green → Yellow → Red gradient

### Warnings Panel

- **Position**: In inspector or as floating panel
- **Style**: Yellow/warning icon with descriptive text
- **Severity levels**: Warning (yellow), Error (red)

---

## 12. Window Title Bar

- Standard macOS title bar
- Traffic lights (close/minimize/zoom)
- Window title shows project name
- No tabs (unlike Xcode)
- Thin separator between title bar and toolbar

---

## Sources

- Apple WWDC23 "Meet Reality Composer Pro" (session 10083)
- Apple WWDC23 "Explore materials in Reality Composer Pro" (session 10202)
- Apple WWDC23 "Work with Reality Composer Pro content in Xcode" (session 10273)
- Apple WWDC24 "Compose interactive 3D content in Reality Composer Pro" (session 10102)
- Apple "Configuring the Reality Composer Pro project window" documentation
- elkraneo "Deconstructing Reality Composer Pro" series (2026)
  - Viewport, Document Type, Inspector Components, Inspector Bindings/References/Variants
- Apple Human Interface Guidelines: Color, Dark Mode, Layout
- macOS NSColor system color values (dark mode variants)
