# Declarative UI Migration Plan

## 1. Current State Inventory

### 1.1 Architecture Overview

The editor UI (`katla_app/src/ui/editor_ui/`) is built entirely on the immediate mode system in `katla_ui`. Every frame, `EditorUI::render()` calls `ui.begin()`, `self.build()` walks all panels producing draw calls, and `ui.end()` returns the `DrawList`.

**Frame loop** (`katla_app/src/ui/editor_ui/mod.rs`):
1. `EditorUI::render()` → `ui.begin(screen_size, scale_factor)`
2. `EditorUI::build()` — renders floating panels, then toolbar, hierarchy, inspector, viewport grid, asset browser, status bar
3. `ui.end()` → `DrawList` → GPU

**Entry point** (`EditorUI` struct in `mod.rs`):
- Holds all panel state, theme, selected entity, pending actions, viewport info
- `render()` applies theme, calls `build()`, returns `DrawList`
- `take_actions()` drains `pending_actions: Vec<EditorAction>`

### 1.2 Panel Inventory

#### Toolbar (`toolbar.rs`)
- **What it does**: Menu bar with File/Edit/View/Create dropdowns, centered title
- **State**: `ToolbarState` (menu open flags, `pending_actions`, undo/redo counts)
- **Widgets**: `MenuBar`, `menu_bar_dropdown`, `menu_item_clicked`, `toggle_menu_item_clicked`
- **Interactions**: Menu open/close, menu item clicks → `EditorAction` pushes
- **Dependencies**: `Preferences` (grid/stats visibility), `SpawnableModel::all()`

#### Status Bar (`status_bar.rs`)
- **What it does**: Bottom bar showing FPS, frame count, entity count, asset count, mode (PLAYING/EDITING), theme name, save confirmation
- **State**: All passed as `StatusBarConfig` params (no persistent state)
- **Widgets**: `StatusBar` widget, `status_label`, `status_separator`, `draw_text`
- **Interactions**: None (read-only display)
- **Dependencies**: `ColorScheme`

#### Hierarchy (`hierarchy.rs`)
- **What it does**: Left panel showing entity tree with expand/collapse, search filter, context menu (Duplicate/Rename/Delete), entity type icons and badges
- **State**: `HierarchyState` (`scroll_state: ScrollAreaState`, `expanded_entities: HashSet<EntityId>`, `context_menu_open: bool`, `context_entity: Option<EntityId>`)
- **Search filter**: `hierarchy_search_filter: String` (stored on `EditorUI`)
- **Widgets**: `Panel`, `TextInput` (search), `TreeView`, `context_menu`
- **Interactions**: Click to select entity, double-click to expand/collapse, right-click context menu, keyboard navigation (Arrow keys, Delete, Escape)
- **Dependencies**: `&mut selected_entity`, `&[EntityInfo]`, `&mut pending_actions`, `&ColorScheme`
- **Key functions**: `is_entity_visible_fast()` — O(D) visibility check using parent map

#### Inspector (`inspector.rs`)
- **What it does**: Right panel showing properties of selected entity: name, transform (Vec3 sliders), point light (color picker, intensity, range), particle emitter (emit rate, velocity, lifetime, gravity, scale), info section, delete button
- **State**: `InspectorEditState` (`pos`, `rot`, `scale`, `light_color`, `light_intensity`, `light_range`, `emit_rate`, `velocity`, `lifetime`, `gravity`, `particle_scale`, `light_color_picker: ColorPickerState`) — stored on `EditorUI`
- **Scroll**: `inspector_scroll_state: ScrollAreaState` — stored on `EditorUI`
- **Sync**: `EditorUI::sync_inspector_edit_state()` copies from `EntityInfo` when selected entity changes
- **Widgets**: `Panel`, `ScrollArea`, `Vec3Slider`, `LabeledSlider`, `ColorPickerButton`, `Button`
- **Interactions**: Slider drag → updates `InspectorEditState` fields; color picker; delete button → `EditorAction::DeleteEntity`
- **Dependencies**: `&mut selected_entity`, `&[EntityInfo]`, `&mut pending_actions`, `&mut InspectorEditState`, `&mut ScrollAreaState`

#### Viewport Grid (`viewport_grid.rs`)
- **What it does**: Renders 1-4 viewports in configurable grid (1x1, 1x2, 2x1, 2x2) with texture images, borders, labels
- **State**: `ViewportGridState` (from `resources/viewport_state.rs` — layout enum, active viewport)
- **Texture IDs**: `viewport_texture_ids: [Option<TextureId>; 4]` — stored on `EditorUI`
- **Widgets**: `ui.image()`, `draw_selection_border`, `draw_text`, `draw_rect`
- **Interactions**: Hover detection, active viewport highlighting, gizmo mode buttons (RadioButtons)
- **Dependencies**: `&ViewportGridState`, `&[Option<TextureId>; 4]`, `&ColorScheme`

#### Asset Browser (`asset_browser/mod.rs`, `state.rs`, `types.rs`)
- **What it does**: Bottom panel with breadcrumb navigation, grid of asset thumbnails, folder navigation, search, context menu, drag-and-drop to viewport, marquee selection, rename mode, confirmation dialog
- **State**: `AssetBrowserState` (~25 fields: path, assets, selection, scroll, drag state, rename, context menu, confirm dialog, nav history, search)
- **Widgets**: `ImageButton` (nav, collapse, refresh), `TextInput` (search, rename), `ScrollArea`, `context_menu`, `modal` (confirm dialog), `image()` (thumbnails)
- **Interactions**: Click/double-click (navigate folders, preview models), right-click context menu, breadcrumb navigation, back/forward/refresh buttons, drag to viewport, marquee selection, rename, keyboard nav, file system operations
- **Dependencies**: `&mut BackgroundLoader`, `&HashMap<PathBuf, TextureHandle>`, `&ColorScheme`

#### Preferences (`preferences.rs`)
- **What it does**: Floating draggable panel with tabs (General, Viewport, AI). General: theme grid, font scale slider. Viewport: grid/stats toggles, grid size buttons, snap toggle, camera speed slider. AI: provider selection, API key, model, base URL, temperature, max tokens.
- **State**: `PreferencesPanelState` (panel: `DraggablePanelState`, `current_tab`, `scroll_state`, `llm_config` snapshot)
- **Widgets**: `DraggablePanel`, `Button` (tabs, theme grid, size grid, provider grid), `ToggleButton`, `Slider`, `TextInput` (API key, model, base URL), `ScrollArea`
- **Interactions**: Tab switching, theme selection, slider changes, toggle clicks, text input → `PreferencesAction` → `EditorAction`
- **Dependencies**: `&Preferences`, `&EditorSettings`, `&ColorScheme`, `&LlmConfig`

#### Co-Creator (`co_creator.rs`)
- **What it does**: Floating draggable chat panel with message history, markdown rendering, text input, send/undo buttons
- **State**: `CoCreatorState` (panel: `DraggablePanelState`, `input_text`, `messages`, `processing`, `status_message`, `scroll_state`)
- **Widgets**: `DraggablePanel`, `ScrollArea`, `TextInput`, `Button`, `ImageButton`, markdown rendering
- **Interactions**: Text input, send message, undo agent action, close panel
- **Dependencies**: `&ColorScheme`, `agent_undo_count`

#### Particle Inspector (`particle_inspector.rs`, referenced via `../particle_inspector.rs`)
- **What it does**: Floating draggable panel showing particle emitter properties
- **State**: `ParticleInspectorState` (panel: `DraggablePanelState`), `selected_particle_emitter`
- **Interactions**: Toggle emitter, reset system, close
- **Dependencies**: `&ParticleInspectorData`, `&ColorScheme`

#### Gizmo Mode Buttons (in `layout.rs`, inside viewport section)
- **What it does**: Three radio buttons (W:Move, E:Rotate, R:Scale) inside viewport area
- **State**: `gizmo_mode: u8` on `EditorUI`
- **Widgets**: `RadioButton`
- **Interactions**: Click → `EditorAction::SetGizmoMode`

#### Resize Handles (in `layout.rs`)
- **What it does**: Three resize handles for left panel width, right panel width, asset browser height
- **State**: `left_panel_width`, `right_panel_width`, `asset_browser.panel_height` on `EditorUI`
- **Widgets**: `ResizeHandle::horizontal()`, `ResizeHandle::vertical()`
- **Interactions**: Drag to resize panels

### 1.3 Shared State Between Panels

| State | Owner | Used By |
|-------|-------|---------|
| `selected_entity: Option<EntityId>` | `EditorUI` | Hierarchy, Inspector, keyboard nav, layout (delete key) |
| `inspector_edit: InspectorEditState` | `EditorUI` | Inspector (synced from `EntityInfo`) |
| `inspector_edit_entity: Option<EntityId>` | `EditorUI` | Inspector sync logic |
| `inspector_scroll_state: ScrollAreaState` | `EditorUI` | Inspector |
| `hierarchy_search_filter: String` | `EditorUI` | Hierarchy |
| `hierarchy_state: HierarchyState` | `EditorUI` | Hierarchy |
| `theme: ColorScheme` | `EditorUI` | All panels |
| `font_scale: f32` | `EditorUI` | All panels |
| `pending_actions: Vec<EditorAction>` | `EditorUI` | All panels (written), layout (read, drained) |
| `focused_panel: FocusedPanel` | `EditorUI` | Layout, Asset Browser (keyboard focus) |
| `left_panel_width: f32` | `EditorUI` | Layout |
| `right_panel_width: f32` | `EditorUI` | Layout |
| `last_viewport_bounds: Rect2D` | `EditorUI` | Layout, focus detection |
| `last_viewport_size: (u32, u32)` | `EditorUI` | Application (resize) |
| `is_playing: bool` | `EditorUI` | Status bar |
| `show_grid: bool` | `EditorUI` | Toolbar/Preferences |
| `show_stats: bool` | `EditorUI` | Toolbar/Preferences |
| `gizmo_mode: u8` | `EditorUI` | Gizmo buttons |
| `viewport_texture_ids: [Option<TextureId>; 4]` | `EditorUI` | Viewport Grid |
| `viewport_grid_state: ViewportGridState` | `EditorUI` | Viewport Grid |
| `save_confirmation_timer: f32` | `EditorUI` | Status bar |

### 1.4 Action Flow

1. Panel widgets detect interactions (clicks, slider drags, text input)
2. Panel pushes `EditorAction` or panel-specific action to `pending_actions`
3. `EditorUI::build()` collects actions from panels
4. After `ui.end()`, application calls `editor.take_actions()`
5. Application processes actions in `process_editor_actions()` (step 8 in frame order)

This deferred pattern already matches the declarative `ActionStream` model.

---

## 2. Migration Order

### Difficulty Ranking (Easiest → Hardest)

| # | Panel | Difficulty | Justification |
|---|-------|-----------|---------------|
| 1 | Status Bar | **Small** | Read-only display, no state, no interaction. Pure text rendering. |
| 2 | Toolbar | **Small** | Menu bar with dropdowns. State is simple (open flags). Uses `menu_bar_dropdown` which needs a declarative equivalent. |
| 3 | Gizmo Mode Buttons | **Small** | Three radio buttons, minimal state. Good warm-up for interactive widgets. |
| 4 | Viewport Grid | **Medium** | Image rendering with borders and labels. Minimal interaction (hover). No complex widgets. |
| 5 | Inspector | **Medium-Large** | Most complex widget set (Vec3Slider, ColorPicker, ScrollArea) but well-scoped state. Validates slider/color interactions. |
| 6 | Hierarchy | **Large** | TreeView with custom rendering, context menu, search filter, keyboard nav. Complex interaction. |
| 7 | Preferences | **Large** | Tabbed floating panel with many widgets (theme grid, sliders, text inputs, toggles). Large widget surface area. |
| 8 | Asset Browser | **Very Large** | Most complex panel: grid layout, thumbnails, drag-and-drop, marquee selection, context menu, rename mode, confirm dialog, breadcrumb nav, file system operations. |
| 9 | Co-Creator | **Large** | Markdown rendering, chat history, text input. Floating panel. |
| 10 | Particle Inspector | **Medium** | Floating panel with simple controls. Similar pattern to Inspector subset. |

### Dependencies

- **Status Bar** and **Toolbar** can be migrated independently — no shared state beyond theme.
- **Gizmo Buttons** depend on the viewport area being available but are otherwise independent.
- **Inspector** depends on `selected_entity`, `InspectorEditState`, `InspectorAction` pattern — validates the Binding/StateArena/ActionStream model.
- **Hierarchy** shares `selected_entity` and `expanded_entities` with Inspector — migrate after Inspector validates the pattern.
- **Viewport Grid** is standalone — can migrate at any point.
- **Preferences** uses many widget types — benefits from Inspector being done first (slider/toggle/text input patterns established).
- **Asset Browser** uses every pattern — definitely last.
- **Co-Creator** and **Particle Inspector** are floating panels with text input — can be done in parallel with Preferences.

### Independent Migration Groups

```
Group A (simple, no dependencies):
  Status Bar → Toolbar → Gizmo Buttons

Group B (core interactive pattern):
  Inspector → Hierarchy

Group C (standalone):
  Viewport Grid

Group D (complex, needs A+B patterns):
  Preferences → Particle Inspector → Co-Creator

Group E (hardest, needs all patterns):
  Asset Browser
```

---

## 3. Per-Panel Migration Guide

### 3.1 Status Bar

**Current implementation**: `status_bar.rs` — `StatusBar` struct implements `Widget`. Renders FPS, frame count, entity count, selection count, mode, theme name, save confirmation.

**State**: None (all data passed via `StatusBarConfig`).

**Widgets used**: `StatusBar` widget, `status_label`, `status_separator`, `draw_text`.

**Declarative equivalent**:
```rust
struct StatusBarView {
    fps: f32,
    frame_count: usize,
    entity_count: usize,
    selected_count: usize,
    total_assets: usize,
    is_playing: bool,
    theme: ColorScheme,
    save_confirmation_timer: f32,
    screen_width: f32,
    height: f32,
}

impl Build for StatusBarView {
    fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor {
        let fps_color = if self.fps >= 55.0 { self.theme.success }
            else if self.fps >= 30.0 { self.theme.warning }
            else { self.theme.error };

        ViewDescriptor::HStack(Box::new(StackDescriptor {
            children: vec![
                ViewDescriptor::Text {
                    content: format!("FPS: {:.0}", self.fps),
                    color: Some(fps_color),
                    font_size: None,
                },
                ViewDescriptor::Text {
                    content: format!("Frame: {}", self.frame_count),
                    color: Some(self.theme.text_secondary),
                    font_size: None,
                },
                // ... more labels
            ],
            spacing: 0.0,
            padding: Padding::all(4.0),
            alignment: Alignment::Leading,
        }))
    }
}
```

**State migration**: No state to migrate. All data flows in via `Binding<T>` or direct fields.

**Effort**: **Small** — 1-2 hours.

**Risks**: The `StatusBar` widget from `katla_ui` uses `status_label` helper which positions text at a running cursor. The declarative version relies on Taffy's HStack layout. Need to verify that the status bar renders correctly with flex layout instead of manual cursor advancement.

**Gotchas**: The save confirmation text has alpha fade-out (`save_confirmation_timer < 0.5`). The declarative version can use `AnimatedProperty::Opacity` for this, or simply set the color alpha in the descriptor.

---

### 3.2 Toolbar

**Current implementation**: `toolbar.rs` — `Toolbar` struct implements `Widget`. Menu bar with 4 dropdowns (File/Edit/View/Create) plus centered title.

**State**: `ToolbarState` — 4 menu open flags, `pending_actions: Vec<EditorAction>`, undo/redo counts.

**Widgets used**: `MenuBar`, `menu_bar_dropdown`, `menu_item_clicked`, `menu_item_clicked_with_icon_and_shortcut`, `toggle_menu_item_clicked`, `menu_separator`.

**Declarative equivalent**:
```rust
struct ToolbarView {
    state: ToolbarState,  // or Binding<ToolbarState>
    theme: ColorScheme,
    screen_width: f32,
    height: f32,
}

impl Build for ToolbarView {
    fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor {
        let menu_open_ids: Vec<StateId> = (0..4)
            .map(|i| ctx.state(false))
            .collect();

        // Toolbar as HStack of menu buttons + centered title
        ViewDescriptor::ZStack(Box::new(ZStackDescriptor {
            children: vec![
                (Alignment::Leading, ViewDescriptor::HStack(Box::new(StackDescriptor {
                    children: vec![
                        self.file_menu(ctx, menu_open_ids[0]),
                        self.edit_menu(ctx, menu_open_ids[1]),
                        self.view_menu(ctx, menu_open_ids[2]),
                        self.create_menu(ctx, menu_open_ids[3]),
                    ],
                    spacing: 0.0,
                    padding: Padding::zero(),
                    alignment: Alignment::Leading,
                }))),
                (Alignment::Center, ViewDescriptor::Text {
                    content: "Katla Engine".into(),
                    color: Some(self.theme.text_muted),
                    font_size: Some(FontSize::Medium),
                }),
            ],
            padding: Padding::zero(),
        }))
    }
}
```

**State migration**: Menu open flags become `StateArena` state (`ctx.state(false)` for each). Undo/redo counts come from `Environment` or `Binding`.

**Action handling**: Menu item clicks use `ctx.on_click(|| ctx.emit(EditorAction::SaveScene))`.

**Effort**: **Small** — 3-4 hours. The dropdown menus need an `Overlay` variant for popup positioning.

**Risks**: Dropdown menus require overlay positioning. The declarative `Overlay` variant handles this, but menu item hover highlighting and keyboard navigation need testing.

**Gotchas**: The toolbar temporarily overrides `ui.style().button_normal` to `Color::TRANSPARENT`. The declarative version should set `fill_color: Some(Color::TRANSPARENT)` on each button descriptor.

---

### 3.3 Gizmo Mode Buttons

**Current implementation**: Inline in `layout.rs` inside the viewport section. Three `RadioButton` widgets.

**State**: `gizmo_mode: u8` on `EditorUI`.

**Declarative equivalent**:
```rust
struct GizmoButtonsView {
    gizmo_mode: u8,
}

impl Build for GizmoButtonsView {
    fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor {
        let modes = [("W:Move", 0u8), ("E:Rotate", 1), ("R:Scale", 2)];
        let selected_id = ctx.state(self.gizmo_mode);

        ViewDescriptor::HStack(Box::new(StackDescriptor {
            children: modes.iter().map(|(label, mode)| {
                let is_selected = self.gizmo_mode == *mode;
                ViewDescriptor::Button {
                    label: label.to_string(),
                    fill_color: Some(if is_selected { theme.selection } else { theme.button_bg }),
                    hover_color: Some(theme.button_hover),
                    border_color: None,
                    on_click: Some(ctx.on_click(|| {
                        ctx.emit(EditorAction::SetGizmoMode(*mode));
                    })),
                }
            }).collect(),
            spacing: 2.0,
            padding: Padding::all(10.0),
            alignment: Alignment::TopLeading,
        }))
    }
}
```

**Effort**: **Small** — 1 hour. Simple button group.

---

### 3.4 Viewport Grid

**Current implementation**: `viewport_grid.rs` — `ViewportGrid` struct implements `Widget`. Renders 1-4 viewport images in a grid with borders and labels.

**State**: `ViewportGridState` (layout enum, active viewport) — read-only reference.

**Widgets used**: `ui.image()`, `draw_selection_border`, `draw_text`, `draw_rect`.

**Declarative equivalent**:
```rust
struct ViewportGridView {
    state: ViewportGridState,
    texture_ids: [Option<TextureId>; 4],
    theme: ColorScheme,
}

impl Build for ViewportGridView {
    fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor {
        let (rows, cols) = self.state.layout.grid_dimensions();
        let mut children = Vec::new();
        for row in 0..rows {
            let mut row_children = Vec::new();
            for col in 0..cols {
                if let Some(slot) = self.state.layout.slot_index(row, col) {
                    let texture = self.texture_ids[slot].unwrap_or(TextureId::NONE);
                    let label = match self.state.layout { /* ... */ };
                    row_children.push(ViewDescriptor::ZStack(Box::new(ZStackDescriptor {
                        children: vec![
                            (Alignment::TopLeading, ViewDescriptor::Text {
                                content: label.to_string(),
                                color: Some(Color::WHITE.with_alpha(0.8)),
                                font_size: Some(FontSize::Small),
                            }),
                        ],
                        padding: Padding::zero(),
                    })));
                    // Note: Image needs to be the base layer
                }
            }
            children.push(ViewDescriptor::HStack(Box::new(StackDescriptor {
                children: row_children,
                spacing: 0.0,
                padding: Padding::zero(),
                alignment: Alignment::Leading,
            })));
        }
        ViewDescriptor::VStack(Box::new(StackDescriptor {
            children,
            spacing: 0.0,
            padding: Padding::zero(),
            alignment: Alignment::Leading,
        }))
    }
}
```

**Effort**: **Medium** — 4-6 hours. The `Image` variant in `ViewDescriptor` needs to work with bindless textures (high bit set).

**Risks**: The viewport texture uses bindless texture IDs (`TextureId` with high bit 63 set). The `ViewDescriptor::Image` variant stores a `TextureId` directly, so this should work. Need to verify that the declarative draw pass handles the border/label overlay correctly.

---

### 3.5 Inspector

**Current implementation**: `inspector.rs` — `Inspector` struct implements `Widget`. Shows selected entity properties inside a `Panel` with `ScrollArea`.

**State**:
- `InspectorEditState` on `EditorUI`: `pos`, `rot`, `scale` (Vec3), `light_color`, `light_intensity`, `light_range`, `emit_rate`, `velocity`, `lifetime`, `gravity`, `particle_scale` (f32 scalars), `light_color_picker: ColorPickerState`
- `inspector_scroll_state: ScrollAreaState` on `EditorUI`
- `inspector_edit_entity: Option<EntityId>` — tracks which entity the edit state is for

**Sync logic**: `EditorUI::sync_inspector_edit_state()` copies values from `EntityInfo` to `InspectorEditState` when selected entity changes. Sliders write directly to `InspectorEditState` fields. The application reads these values back via `process_editor_actions()` (not shown — presumably the app reads `inspector_edit` after UI frame).

**Widgets used**: `Panel`, `ScrollArea`, `Vec3Slider`, `LabeledSlider`, `ColorPickerButton`, `Button` (delete), `draw_text`, `draw_line` (section headers).

**Declarative equivalent**:
```rust
struct InspectorView {
    selected_entity: Binding<Option<EntityId>>,
    entities: Vec<EntityInfo>,
}

struct InspectorEditState {
    pos: [f32; 3],
    rot: [f32; 3],
    scale: [f32; 3],
    light_color: [f32; 3],
    light_intensity: f32,
    light_range: f32,
    emit_rate: f32,
    velocity: f32,
    lifetime: f32,
    gravity: f32,
    particle_scale: f32,
}

impl Build for InspectorView {
    fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor {
        let entity_id = self.selected_entity.get();
        let edit_id = ctx.state(InspectorEditState {
            pos: [0.0; 3],
            rot: [0.0; 3],
            scale: [1.0, 1.0, 1.0],
            light_color: [1.0; 3],
            light_intensity: 1.0,
            light_range: 10.0,
            emit_rate: 10.0,
            velocity: 2.0,
            lifetime: 2.0,
            gravity: -9.81,
            particle_scale: 0.1,
        });

        let Some(entity_id) = entity_id else {
            return ViewDescriptor::Text {
                content: "No entity selected".into(),
                color: None,
                font_size: None,
            };
        };

        let Some(entity) = self.entities.iter().find(|e| e.id == entity_id) else {
            return ViewDescriptor::Empty;
        };

        // Build sections
        let mut children = vec![
            ViewDescriptor::Text {
                content: entity.name.clone(),
                color: None,
                font_size: Some(FontSize::Medium),
            },
            section_header("Transform", ctx.env::<ColorScheme>().unwrap()),
            // Vec3 sliders for position, rotation, scale
            // Each needs 3 Slider descriptors with StateIds
        ];

        if entity.point_light.is_some() {
            children.push(section_header("Point Light", ctx.env::<ColorScheme>().unwrap()));
            // ColorPicker, Intensity slider, Range slider
        }

        if entity.particle_emitter.is_some() {
            children.push(section_header("Particle Emitter", ctx.env::<ColorScheme>().unwrap()));
            // Emit rate, velocity, lifetime, gravity, scale sliders
        }

        children.push(section_header("Info", ctx.env::<ColorScheme>().unwrap()));
        // Type text, component list

        children.push(delete_button(ctx, || {
            ctx.emit(EditorAction::DeleteEntity(entity_id));
        }));

        ViewDescriptor::Panel(Box::new(PanelDescriptor {
            title: "Inspector".into(),
            content: Box::new(ViewDescriptor::ScrollView(Box::new(ScrollDescriptor {
                content: Box::new(ViewDescriptor::VStack(Box::new(StackDescriptor {
                    children,
                    spacing: 4.0,
                    padding: Padding::all(12.0),
                    alignment: Alignment::Leading,
                }))),
                scroll_state_id: ctx.state(0.0f32),
            }))),
            header_height: 24.0,
        }))
    }
}
```

**State migration**:
- `InspectorEditState` → `StateArena` state cell (one `StateId` per value group)
- `ScrollAreaState` → `ScrollView`'s `scroll_state_id` in arena
- `ColorPickerState` → `ColorPicker`'s `value_id` in arena
- Entity sync → happens in `build()` by checking if `selected_entity` binding changed

**Key issue**: The current `InspectorEditState` is mutable — sliders write to it directly and the application reads it back. In the declarative model, state lives in `StateArena`. The application needs to read slider values from `StateArena` after the frame, or use `ActionStream` to propagate changes.

**Resolution**: Use `Binding<InspectorEditState>` — the `get` closure reads from `StateArena`, the `set` closure writes to `StateArena`. Or better: keep `InspectorEditState` on `EditorUI` and use `Binding::from_ref(&mut self.inspector_edit)` for the frame's duration.

**Effort**: **Medium-Large** — 1-2 days. This is the first complex panel and validates the slider/toggle/color picker interaction patterns.

**Risks**:
- The `Vec3Slider` widget renders 3 sliders with axis colors. The declarative `Slider` variant is single-value. Need either a `Vec3Slider` helper that produces 3 `Slider` descriptors, or a `Custom` draw function.
- The `ColorPickerButton` uses an overlay picker — the declarative `ColorPicker` variant needs the full SV+Hue picker accessible.
- `ScrollArea` in immediate mode uses `scroll_area()` which returns content height. The declarative `ScrollView` uses `scroll_state_id` in arena. Need to verify scrolling works correctly.

**Gotchas**: The inspector's edit state is synced only when the entity changes (`inspector_edit_entity != selected_entity`). This guard must be replicated — `build()` should only overwrite arena state when the binding value changes, not every frame.

---

### 3.6 Hierarchy

**Current implementation**: `hierarchy.rs` — `Hierarchy` struct implements `Widget`. Search filter, `TreeView` with custom per-item rendering, context menu.

**State**: `HierarchyState` (`scroll_state`, `expanded_entities: HashSet<EntityId>`, `context_menu_open`, `context_entity`).

**Widgets used**: `Panel`, `TextInput` (search), `TreeView` with `render_item` callback, `context_menu`.

**Declarative equivalent**: The `TreeView` is a complex custom widget. In the declarative system, it maps to a recursive `VStack` with toggle buttons for expand/collapse and click handlers for selection. The per-item custom rendering (icons, badges, depth lines) needs either:
- A `Custom` draw function per item
- New `ViewDescriptor` variants for tree items

```rust
struct HierarchyView {
    entities: Vec<EntityInfo>,
    selected_entity: Binding<Option<EntityId>>,
    expanded_entities: Binding<HashSet<EntityId>>,
    theme: ColorScheme,
}

impl Build for HierarchyView {
    fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor {
        let search_id = ctx.state(String::new());
        let search_text: String = ctx.state_arena().get(search_id);

        // Filter entities
        let filtered: Vec<&EntityInfo> = /* ... */;

        // Build tree items as VStack of custom-drawn rows
        let items: Vec<ViewDescriptor> = filtered.iter()
            .filter(|e| is_visible(e, &expanded))
            .map(|entity| {
                ViewDescriptor::HStack(Box::new(StackDescriptor {
                    children: vec![
                        // Expand arrow button (if has_children)
                        // Entity icon (Custom draw)
                        // Entity name text
                        // Entity type badge text
                    ],
                    spacing: 4.0,
                    padding: Padding::horizontal(8.0),
                    alignment: Alignment::Leading,
                }))
            })
            .collect();

        ViewDescriptor::Panel(Box::new(PanelDescriptor {
            title: format!("Hierarchy ({} entities)", filtered.len()),
            content: Box::new(ViewDescriptor::VStack(Box::new(StackDescriptor {
                children: vec![
                    ViewDescriptor::TextField {
                        placeholder: "Filter entities...".into(),
                        value_id: search_id,
                        on_submit: None,
                    },
                    ViewDescriptor::ScrollView(Box::new(ScrollDescriptor {
                        content: Box::new(ViewDescriptor::VStack(Box::new(StackDescriptor {
                            children: items,
                            spacing: 0.0,
                            padding: Padding::zero(),
                            alignment: Alignment::Leading,
                        }))),
                        scroll_state_id: ctx.state(0.0f32),
                    })),
                ],
                spacing: 2.0,
                padding: Padding::all(4.0),
                alignment: Alignment::Leading,
            }))),
            header_height: 24.0,
        }))
    }
}
```

**State migration**:
- `expanded_entities` → `StateArena` state or `Binding<HashSet<EntityId>>`
- `selected_entity` → `Binding<Option<EntityId>>`
- `scroll_state` → `ScrollView`'s `scroll_state_id`
- `context_menu_open` → `StateArena` state
- `search_filter` → `TextField`'s `value_id`

**Effort**: **Large** — 2-3 days. The tree rendering with icons, badges, and depth lines is heavily customized.

**Risks**: The `TreeView` widget handles virtual scrolling (only renders visible items). The declarative `ScrollView` needs similar virtualization for large entity counts. Without it, building `ViewDescriptor` for 1000+ entities every frame is wasteful.

**Gotchas**: The hierarchy uses `EntityId` (from `katla_ecs`) as tree item IDs. The declarative tree needs a way to map `EntityId` to `ViewId` for diffing. The `Custom` draw function approach loses diffing granularity — adding/removing entities replaces the entire list.

---

### 3.7 Preferences

**Current implementation**: `preferences.rs` — `PreferencesPanel` struct implements `Widget`. Floating draggable panel with 3 tabs. Each tab has section headers, button grids, sliders, toggles, text inputs.

**State**: `PreferencesPanelState` (panel: `DraggablePanelState`, `current_tab`, `scroll_state`, `llm_config`).

**Widgets used**: `DraggablePanel`, `Button`, `ToggleButton`, `Slider`, `TextInput`, `ScrollArea`, `begin_grid/end_grid`.

**Declarative equivalent**: The `DraggablePanel` is an immediate mode concept. In the declarative system, it becomes a `Panel` inside an `Overlay` with drag handling. Alternatively, keep `DraggablePanel` as a `Custom` draw function during migration.

**State migration**:
- `current_tab` → `StateArena` state
- `scroll_state` → `ScrollView`'s `scroll_state_id`
- `llm_config` → `Environment` or `Binding<LlmConfig>`
- Panel position → `StateArena` state or `DraggablePanelState` binding

**Effort**: **Large** — 2-3 days. Many widgets to map, grid layout to replicate.

**Risks**: The `DraggablePanel` has its own position tracking and dragging behavior. The declarative `Overlay` doesn't inherently support dragging. Either extend `Overlay` with drag support, use `Custom` for the draggable panel shell, or add a `DraggablePanel` variant to `ViewDescriptor`.

---

### 3.8 Asset Browser

**Current implementation**: `asset_browser/mod.rs` (~500 lines). Grid of thumbnails, breadcrumb nav, search, context menu, drag-and-drop, marquee selection, rename mode, confirm dialog.

**State**: `AssetBrowserState` (~25 fields including drag state, selection state, rename state, nav history, confirm dialog).

**This is the hardest panel to migrate.** It uses:
- Custom grid layout with manual item positioning (not Taffy)
- Drag-and-drop to viewport (cross-panel interaction)
- Marquee selection (custom mouse tracking)
- File system operations (create folder, delete, rename)
- Thumbnail loading (async)
- Confirmation modal dialog
- Breadcrumb navigation with click regions
- Rename inline text input

**Declarative equivalent**: This panel benefits least from the declarative system. Consider keeping it in immediate mode permanently, or migrating only the non-interactive parts (header, breadcrumbs, grid) while keeping drag-and-drop and marquee in `Custom` functions.

**Effort**: **Very Large** — 4-5 days.

**Risks**: The asset browser's grid layout bypasses Taffy entirely — items are positioned manually based on column count and item size. Taffy's flexbox would need explicit sizing constraints for each cell to replicate this. The drag-and-drop and marquee selection are highly interactive patterns that don't map cleanly to the declarative model's callback-based approach.

**Recommendation**: Keep asset browser in immediate mode or use `Custom` draw functions within the declarative tree.

---

### 3.9 Co-Creator

**Current implementation**: `co_creator.rs` — Floating chat panel with markdown rendering.

**State**: `CoCreatorState` (panel, input_text, messages, processing, scroll_state).

**Key challenge**: Markdown rendering uses `parse_markdown_line` and `draw_markdown_segments` — custom text rendering that doesn't map to `ViewDescriptor::Text`. Use `Custom` draw function for the message area.

**Effort**: **Large** — 2-3 days. Markdown rendering is custom.

---

### 3.10 Particle Inspector

**Current implementation**: Floating panel with particle emitter controls (sliders, buttons).

**State**: `ParticleInspectorState` (panel, selected_emitter).

**Effort**: **Medium** — 1-2 days. Similar pattern to Inspector subset.

---

## 4. Shared Infrastructure Migration

### 4.1 EditorUI Struct Changes

The `EditorUI` struct (`katla_app/src/ui/editor_ui/mod.rs`) currently holds all panel state and orchestrates the build. During migration, it gains a `ViewTree`:

```rust
pub struct EditorUI {
    // Existing fields (unchanged during migration)
    pub selected_entity: Option<EntityId>,
    pub theme: ColorScheme,
    pub pending_actions: Vec<EditorAction>,
    // ... all current fields ...

    // NEW: Declarative view tree
    view_tree: katla_ui::declarative::ViewTree,
}
```

During migration, `EditorUI::build()` calls both declarative and immediate mode code:

```rust
fn build(&mut self, ui: &mut UiContext, params: &mut EditorRenderParams) {
    // 1. Set up environment
    self.view_tree.env_mut().set(self.theme.clone());

    // 2. Build declarative panels (migrated ones)
    let input_consumed = self.view_tree.frame(ui, &self.declarative_root(), ui.screen_size());

    // 3. Process declarative actions
    for action in self.view_tree.actions_mut().drain::<EditorAction>() {
        self.pending_actions.push(action);
    }

    // 4. Build immediate mode panels (not yet migrated)
    // toolbar, hierarchy, inspector, etc. — only unmigrated ones
}
```

After full migration, `EditorUI::build()` becomes a thin wrapper:

```rust
fn build(&mut self, ui: &mut UiContext, params: &mut EditorRenderParams) {
    self.sync_view_tree_state(params);
    self.view_tree.env_mut().set(self.theme.clone());
    self.view_tree.frame(ui, &EditorRootView { /* ... */ }, ui.screen_size());

    for action in self.view_tree.actions_mut().drain::<EditorAction>() {
        self.pending_actions.push(action);
    }
}
```

### 4.2 Frame Loop Changes

**Current** (in `katla_app`):
```rust
fn generate_ui_draw_list(&mut self) {
    let draw_list = self.editor.render(&mut self.ui, &mut params);
    self.ui_draw_list = draw_list;
}
```

**During migration** — no change to this call. `EditorUI::render()` internally delegates to `build()` which calls both systems.

**After migration** — same call, but `build()` only uses `ViewTree::frame()`.

### 4.3 Input Routing During Coexistence

Both systems share the same `UiInputState` on `UiContext`. The declarative input pass runs first (inside `ViewTree::frame()`):

1. `ViewTree::frame()` calls `input::process_input()` which hit-tests against the declarative tree
2. If input is consumed, `frame()` returns `true` (input_consumed)
3. The immediate mode code checks `ui.input_consumed_by_declarative()` (not yet implemented) to skip input handling
4. If not consumed, immediate mode widgets process input normally

**Required change to `UiContext`**: Add `declarative_input_consumed: bool` flag that `ViewTree::frame()` sets before returning.

### 4.4 Action Handling

Both systems push to `EditorUI::pending_actions`. The declarative system uses `ActionStream::drain::<EditorAction>()` which is collected in `build()` and appended. No change to `take_actions()` on `EditorUI`.

### 4.5 Theme/Environment

The `ColorScheme` is set via `Environment::set()` on the `ViewTree`. Each `build()` call reads it via `ctx.env::<ColorScheme>()`. During coexistence, the theme is also applied to `UiContext::style_mut()` for immediate mode code.

### 4.6 Panel Resize Handles

The resize handles (`ResizeHandle::horizontal()` / `ResizeHandle::vertical()`) in `layout.rs` adjust `left_panel_width`, `right_panel_width`, and `asset_browser.panel_height`. These values feed into panel bounds calculations.

During migration, resize handles stay in immediate mode (they're simple drag interactions that adjust a float). After all panels are migrated, the resize handles can become `Custom` draw functions in the declarative tree, or the bounds can be `Binding<f32>`.

---

## 5. Step-by-Step Migration Sequence

### Step 0: Infrastructure Setup

**Goal**: Add `ViewTree` to `EditorUI` alongside existing code. Zero visual change.

**Changes**:
1. Add `view_tree: ViewTree` field to `EditorUI` in `mod.rs`
2. Add `declarative_input_consumed: bool` flag to `UiContext` (in `katla_ui`)
3. In `EditorUI::build()`, call `self.view_tree.frame()` with an empty root, before immediate mode code
4. Verify: existing tests pass, no visual change

**Testing**: `cargo test --workspace`, run editor, verify identical appearance.

---

### Step 1: Migrate Status Bar

**Goal**: Status bar renders via declarative tree.

**Changes**:
1. Create `katla_app/src/ui/editor_ui/declarative/status_bar.rs`
2. Implement `StatusBarView` with `Build` trait
3. In `EditorUI::build()`, remove immediate mode `StatusBar` add, add `StatusBarView` to declarative root
4. Remove `status_bar` import from `layout.rs`

**Testing**: Verify FPS counter, entity count, mode indicator, save confirmation all render correctly. Compare side-by-side screenshot.

---

### Step 2: Migrate Toolbar

**Goal**: Menu bar renders via declarative tree.

**Changes**:
1. Create `katla_app/src/ui/editor_ui/declarative/toolbar.rs`
2. Implement `ToolbarView` with `Build` trait
3. Menu dropdowns use `ViewDescriptor::Overlay` for popup positioning
4. Each menu item click emits `EditorAction`
5. Wire into `EditorUI::build()`, remove immediate mode toolbar

**Testing**: Verify all 4 menu dropdowns open/close correctly. Verify all menu items emit correct actions. Verify "Katla Engine" title centered.

---

### Step 3: Migrate Gizmo Mode Buttons

**Goal**: Gizmo buttons render via declarative tree inside viewport area.

**Changes**:
1. Create `katla_app/src/ui/editor_ui/declarative/gizmo.rs`
2. Implement `GizmoButtonsView` with `Build` trait
3. Position as `Overlay` anchored to viewport top-left
4. Wire into `EditorUI::build()`, remove inline gizmo code from `layout.rs`

**Testing**: Verify gizmo mode switches on click. Verify selected button has highlight. Verify W/E/R keyboard shortcuts still work.

---

### Step 4: Migrate Inspector

**Goal**: Inspector panel renders via declarative tree. This is the critical validation step.

**Changes**:
1. Create `katla_app/src/ui/editor_ui/declarative/inspector.rs`
2. Implement `InspectorView` with `Build` trait
3. Map all slider values to `StateArena` state
4. Handle `Vec3Slider` as 3 `Slider` descriptors (or `Custom` wrapper)
5. Map `ColorPickerButton` to `ViewDescriptor::ColorPicker`
6. Wire delete button via `ctx.emit(EditorAction::DeleteEntity)`
7. Handle entity sync logic in `build()`
8. Wire into `EditorUI::build()`, remove immediate mode inspector

**Testing**:
- Select entity → inspector shows correct values
- Drag position slider → value updates smoothly
- Toggle between entities → edit state syncs correctly
- Delete entity button works
- Point light section shows/hides based on entity components
- Particle emitter section shows/hides
- Scroll works for tall inspectors
- Color picker opens and changes light color

---

### Step 5: Migrate Viewport Grid

**Goal**: Viewport grid renders via declarative tree.

**Changes**:
1. Create `katla_app/src/ui/editor_ui/declarative/viewport_grid.rs`
2. Implement `ViewportGridView` with `Build` trait
3. Map grid layout to nested `HStack`/`VStack`
4. Each viewport is `ViewDescriptor::Image` + `ZStack` for label overlay
5. Wire into `EditorUI::build()`

**Testing**: Verify 1x1, 1x2, 2x1, 2x2 layouts render correctly. Verify viewport textures display. Verify active viewport border highlight.

---

### Step 6: Migrate Hierarchy

**Goal**: Hierarchy panel renders via declarative tree.

**Changes**:
1. Create `katla_app/src/ui/editor_ui/declarative/hierarchy.rs`
2. Implement `HierarchyView` with `Build` trait
3. Map `TreeView` items to recursive `VStack` with `Custom` draw per item
4. Search filter → `TextField`'s `value_id`
5. Expand/collapse → click handlers that toggle `expanded_entities` binding
6. Context menu → `Overlay` + `VStack` of buttons
7. Wire into `EditorUI::build()`

**Testing**:
- Entity list displays correctly
- Click entity → selection updates
- Double-click expands/collapses
- Search filter works
- Arrow key navigation works
- Delete key deletes entity
- Context menu shows on right-click
- Context menu actions work

---

### Step 7: Migrate Preferences

**Goal**: Preferences panel renders via declarative tree.

**Changes**:
1. Create `katla_app/src/ui/editor_ui/declarative/preferences.rs`
2. Implement `PreferencesView` with `Build` trait
3. Map tabs to button group + conditional content
4. Theme grid → `HStack`/`VStack` of themed buttons
5. Sliders, toggles, text inputs → corresponding `ViewDescriptor` variants
6. Floating panel → `Overlay` or `Custom` for draggable behavior
7. Wire into `EditorUI::build()`

**Testing**:
- All 3 tabs switch correctly
- Theme selection works
- Font scale slider works
- Grid/stats toggles work
- Camera speed slider works
- AI provider selection works
- API key text input works
- LLM config saves correctly

---

### Step 8: Migrate Particle Inspector and Co-Creator

**Goal**: Floating panels render via declarative tree.

**Changes**:
1. Create declarative versions of particle inspector and co-creator
2. Use `Custom` draw for markdown rendering in co-creator
3. Use `Overlay` for floating panel positioning
4. Wire into `EditorUI::build()`

**Testing**: Verify particle inspector controls work. Verify chat messages render. Verify text input works.

---

### Step 9: Handle Asset Browser

**Decision point**: The asset browser is extremely complex and benefits least from the declarative model.

**Option A**: Keep in immediate mode permanently — use `ViewDescriptor::Custom` to embed it in the declarative tree.

**Option B**: Migrate incrementally — header/breadcrumbs first, then grid, then interactions.

**Recommendation**: Option A. Wrap the existing `build_asset_browser()` call in a `Custom` descriptor:

```rust
ViewDescriptor::Custom(|ui, bounds| {
    build_asset_browser(&mut state, ui, theme, bounds, focused, loader, thumbnails);
})
```

---

### Step 10: Remove Old Immediate Mode Code

**Goal**: Clean up dead immediate mode code for migrated panels.

**Changes**:
1. Remove `toolbar.rs` (replaced by declarative)
2. Remove `status_bar.rs` (replaced by declarative)
3. Remove `inspector.rs` (replaced by declarative)
4. Remove `hierarchy.rs` (replaced by declarative)
5. Remove `preferences.rs` (replaced by declarative)
6. Remove `viewport_grid.rs` (replaced by declarative)
7. Remove `co_creator.rs` (replaced by declarative)
8. Simplify `layout.rs` — remove all panel building, keep resize handles and panel bounds calculation
9. Simplify `EditorUI::build()` to only call `view_tree.frame()`

**What to keep**:
- `mod.rs` — `EditorUI` struct (simplified, state moves to arena/bindings)
- `types.rs` — `EditorAction`, `EntityInfo`, `FocusedPanel` (these are app-level types)
- `asset_browser/` — stays in immediate mode
- `layout.rs` — resize handles and bounds calculation
- `tests.rs` — update to test declarative panels

**Testing**: Full editor smoke test. All panels render and interact correctly.

---

## 6. Testing Strategy Per Step

### 6.1 General Testing Approach

For each migration step:

1. **Compile check**: `cargo check` passes
2. **Test suite**: `cargo test --workspace` passes
3. **Visual comparison**: Run editor before and after migration, compare screenshots
4. **Interaction test**: Click/drag/type in migrated panel, verify behavior matches

### 6.2 Per-Step Verification

| Step | Visual Check | Interaction Check |
|------|-------------|-------------------|
| 0 | No change | No change |
| 1 | Status bar shows FPS, entities, mode | Save confirmation fades correctly |
| 2 | Menu bar renders, title centered | Each dropdown opens, items clickable, actions fire |
| 3 | Gizmo buttons in viewport top-left | Click changes mode, highlight updates |
| 4 | Inspector shows entity properties | All sliders drag, color picker works, delete works, scroll works |
| 5 | Viewport grid renders textures | Layout switch works, hover highlight, active border |
| 6 | Entity tree renders with icons | Click selects, expand/collapse, search, keyboard nav, context menu |
| 7 | Preferences panel renders tabs | Tab switch, theme grid, sliders, toggles, text inputs |
| 8 | Floating panels render | Particle controls, chat messages, text input |
| 9 | Asset browser unchanged | Asset browser unchanged |
| 10 | Editor looks and works the same | Full regression: every panel, every interaction |

### 6.3 Automated Test Strategy

Existing tests in `tests.rs` test immediate mode panels directly. As panels migrate:

1. Keep existing tests during coexistence (they test the immediate mode version)
2. After a panel is fully migrated, update the test to use the declarative version
3. The test creates a `ViewTree`, sets up state, calls `frame()`, checks draw output

Pattern for declarative panel tests:
```rust
#[test]
fn test_declarative_status_bar() {
    let mut ui = UiContext::new();
    ui.begin(Vec2::new(800.0, 600.0), 1.0);

    let mut tree = ViewTree::new();
    tree.env_mut().set(ColorScheme::default_theme());

    let view = StatusBarView { fps: 60.0, /* ... */ };
    tree.frame(&mut ui, &view, Vec2::new(800.0, 600.0));

    let draw_list = ui.end();
    // Verify draw list contains expected text elements
}
```

---

## 7. Cleanup Plan

### 7.1 Immediate Mode Code to Remove After Full Migration

| File | Status |
|------|--------|
| `katla_app/src/ui/editor_ui/toolbar.rs` | Remove |
| `katla_app/src/ui/editor_ui/status_bar.rs` | Remove |
| `katla_app/src/ui/editor_ui/inspector.rs` | Remove |
| `katla_app/src/ui/editor_ui/hierarchy.rs` | Remove |
| `katla_app/src/ui/editor_ui/preferences.rs` | Remove |
| `katla_app/src/ui/editor_ui/viewport_grid.rs` | Remove |
| `katla_app/src/ui/editor_ui/co_creator.rs` | Remove |

### 7.2 Code to Keep

| File | Reason |
|------|--------|
| `katla_app/src/ui/editor_ui/mod.rs` | `EditorUI` struct (simplified), `render()`, `take_actions()` |
| `katla_app/src/ui/editor_ui/layout.rs` | Resize handles, panel bounds calculation |
| `katla_app/src/ui/editor_ui/types.rs` | `EditorAction`, `EntityInfo`, `FocusedPanel`, `EditorPanel` |
| `katla_app/src/ui/editor_ui/asset_browser/` | Stays in immediate mode (wrapped in `Custom`) |
| `katla_app/src/ui/editor_ui/tests.rs` | Updated for declarative panels |

### 7.3 Immediate Mode API to Keep

The immediate mode `Widget` trait, `UiContext` drawing methods, and widget implementations stay available for:
- Debug overlays (FPS graph, render stats)
- Game HUD (health bars, minimap, score)
- Asset browser (complex custom interactions)
- Any future ad-hoc UI needs

The `katla_ui` crate's immediate mode API is never removed — it's the foundation the declarative layer builds on.

### 7.4 New Declarative Panel Files

| New File | Panel |
|----------|-------|
| `katla_app/src/ui/editor_ui/declarative/mod.rs` | Module root |
| `katla_app/src/ui/editor_ui/declarative/status_bar.rs` | Status bar |
| `katla_app/src/ui/editor_ui/declarative/toolbar.rs` | Toolbar |
| `katla_app/src/ui/editor_ui/declarative/gizmo.rs` | Gizmo buttons |
| `katla_app/src/ui/editor_ui/declarative/inspector.rs` | Inspector |
| `katla_app/src/ui/editor_ui/declarative/viewport_grid.rs` | Viewport grid |
| `katla_app/src/ui/editor_ui/declarative/hierarchy.rs` | Hierarchy |
| `katla_app/src/ui/editor_ui/declarative/preferences.rs` | Preferences |
| `katla_app/src/ui/editor_ui/declarative/co_creator.rs` | Co-creator |
| `katla_app/src/ui/editor_ui/declarative/particle_inspector.rs` | Particle inspector |
| `katla_app/src/ui/editor_ui/declarative/editor_root.rs` | Root view that composes all panels |

### 7.5 `EditorUI` Simplification After Migration

```rust
pub struct EditorUI {
    // Declarative view tree (owns state internally)
    view_tree: ViewTree,

    // Application-level state (not owned by any single panel)
    pub selected_entity: Option<EntityId>,
    pub theme: ColorScheme,
    pub pending_actions: Vec<EditorAction>,
    pub focused_panel: FocusedPanel,

    // Panel geometry (computed from resize handles)
    pub left_panel_width: f32,
    pub right_panel_width: f32,

    // Asset browser (stays immediate mode)
    pub asset_browser: AssetBrowserState,

    // Viewport
    pub viewport_grid_state: ViewportGridState,
    pub viewport_texture_ids: [Option<TextureId>; 4],

    // Flags
    pub is_playing: bool,
    pub show_grid: bool,
    pub show_stats: bool,
    pub font_scale: f32,
    pub gizmo_mode: u8,
    pub save_confirmation_timer: f32,
    pub last_viewport_bounds: Rect2D,
    pub last_viewport_size: (u32, u32),
    pub last_screen_size: Vec2,
}
```

Most panel-specific state (scroll positions, edit buffers, menu open flags) moves into the `ViewTree`'s `StateArena` and is no longer visible on `EditorUI`.

---

## 8. Open Issues and Risks

### 8.1 Missing Declarative Widgets

The following immediate mode widgets have no direct declarative equivalent yet:

| Immediate Mode Widget | Migration Strategy |
|----------------------|-------------------|
| `DraggablePanel` | `Overlay` + drag state in `StateArena`, or `Custom` |
| `TreeView` | Recursive `VStack` + `Custom` per-item rendering |
| `MenuBar` / `menu_bar_dropdown` | `HStack` of buttons + `Overlay` dropdown |
| `ResizeHandle` | Keep in immediate mode, or `Custom` draw function |
| `RadioButton` | `HStack` of `Button` with selection state |
| `ColorPickerButton` (full SV+Hue) | `ViewDescriptor::ColorPicker` (already exists, needs full picker) |
| `ImageButton` | `Button` with icon rendering, or `Custom` |
| `DockArea` / `DockLayout` | Keep for now (skeleton, not active) |
| `context_menu` | `Overlay` + `VStack` of buttons |
| `modal` | `Overlay` centered + `ZStack` for dimming |

### 8.2 Performance Concerns

- **Hierarchy with 1000+ entities**: Building `ViewDescriptor` for every entity every frame is O(N). The immediate mode `TreeView` does virtual scrolling. Need to add virtualization to the declarative `ScrollView`.
- **Inspector slider drag**: Currently writes directly to `&mut f32`. In the declarative model, it writes to `StateArena` which involves `Box<dyn Any>` downcasting. Should be negligible overhead but worth profiling.
- **Full tree rebuild every frame**: The design doc says full rebuild is < 0.1ms for 50-200 nodes. For the full editor with 500+ nodes, profile and optimize if needed.

### 8.3 Gaps in the Declarative System

- **No `Vec3Slider` equivalent**: Need a helper that produces 3 `Slider` descriptors or a `Custom` function
- **No grid layout primitive**: The preferences theme grid uses `begin_grid/end_grid` which is immediate mode cursor-based. Need to replicate with `HStack`/`VStack` nesting or a `Grid` variant
- **No `DraggablePanel` equivalent**: Need overlay + drag handling
- **ScrollView virtualization**: The `ScrollView` descriptor doesn't support virtual rendering — it builds the full content tree even when most is off-screen

---

## 9. Estimated Timeline

| Step | Panel(s) | Effort | Cumulative |
|------|----------|--------|------------|
| 0 | Infrastructure setup | 0.5 day | 0.5 day |
| 1 | Status Bar | 0.5 day | 1 day |
| 2 | Toolbar | 0.5 day | 1.5 days |
| 3 | Gizmo Buttons | 0.25 day | 1.75 days |
| 4 | Inspector | 2 days | 3.75 days |
| 5 | Viewport Grid | 1 day | 4.75 days |
| 6 | Hierarchy | 2 days | 6.75 days |
| 7 | Preferences | 2 days | 8.75 days |
| 8 | Particle Inspector + Co-Creator | 2 days | 10.75 days |
| 9 | Asset Browser (Custom wrapper) | 0.5 day | 11.25 days |
| 10 | Cleanup + testing | 1 day | 12.25 days |

**Total estimate**: ~12 days of focused work.

Each step is independently testable and the immediate mode code remains as fallback. The migration can be paused at any step without leaving the editor in a broken state.
