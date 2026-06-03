# UI Design Reference: Reality Composer Pro

## Layout (4-panel DCC)
```
┌─────────────────────────────────────────────────────┐
│  ● ● ●   Title Bar / Global Toolbar                │
├──────────┬──────────────────────────┬───────────────┤
│  Scene   │                          │  Inspector    │
│  Hierarchy│     3D Viewport         │  (Properties) │
│  (~16%)  │       (~55%)             │    (~22%)     │
│          │  [viewport toolbar]      │               │
├──────────┴──────────────────────────┴───────────────┤
│  Bottom Panel: Project Browser / Timelines (~25%)   │
└─────────────────────────────────────────────────────┘
```

## Color Scheme (Dark Mode)
| Role | Color | Usage |
|------|-------|-------|
| App bg | `#1E1E1E` | Outer chrome |
| Panel bg | `#2A2A2A` | Sidebars, insets |
| Field/input bg | `#3A3A3A` | Text fields, dropdowns |
| Hover | `#48484A` | List row hover |
| Primary accent | `#F79545` (orange) | Active elements, play |
| Secondary accent | `#5BC55A` (green) | Selection |
| Tertiary accent | `#0A84FF` (blue) | Active tab |
| Text primary | `#FFFFFF` | Headers, values |
| Text secondary | `#8E8E93` | Labels |
| Text tertiary | `#6E6E72` | Disabled |
| Borders | `#1A1A1A` | Panel separators |

## Typography
- Font: SF Pro (system font) → Use Roboto as fallback
- Section headers: 12px, semibold, white, with disclosure chevron
- Field labels: 11px, regular, `#8E8E93`
- Field values: 11px, regular, white, tabular figures
- Tab labels: 12px, medium, white (active) / `#8E8E93` (inactive)

## Spacing (4pt grid)
- Panel interior padding: 12px
- Section header → first field: 8px
- Between fields: 6px vertical
- Between sections: 16px
- Tab bar height: ~36px
- Icon button padding: 8px

## Panel Details
- **Scene Hierarchy**: Expandable tree with icons, ~14px indentation per level, orange "Scene" pill header
- **Viewport**: Dark bg (`#1C1C1E`), perspective grid, floating translucent toolbar
- **Inspector**: Collapsible sections (⌄ disclosure), labeled rows with inline fields (X/Y/Z)
- **Bottom**: Tabbed panels (Project Browser, Shader Graph, Timelines, etc.)
