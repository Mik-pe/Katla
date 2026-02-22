# UI System Improvements

> **LIVE DOCUMENT** - Research and roadmap for katla_ui evolution

## Overview

This document captures research findings from comparing katla_ui against established immediate mode UI libraries (egui, imgui) and outlines improvement opportunities.

## Current Assessment

| Category | Status | Notes |
|----------|--------|-------|
| Core Architecture | ✅ Good | Clean module separation, proper dependency boundaries |
| Text Rendering | ✅ Excellent | Subpixel positioning, gamma correction, kerning |
| Widget Variety | ⚠️ Basic | Button, checkbox, slider, text input, label |
| Container Widgets | ❌ Missing | No ScrollArea, no tables |
| Response Richness | ⚠️ Minimal | Basic clicked/hovered/active/changed |
| Code Organization | ⚠️ Needs Work | Large files need splitting |
| Drag-and-Drop | ❌ Missing | No built-in DnD support |

---

## Missing Features

### 1. ScrollArea Container (High Priority)

**Problem**: No scrollable containers. Critical for:
- Entity/asset browsers
- Log viewers
- Property panels with many items

**egui Reference** (`crates/egui/src/containers/scroll_area.rs`):
```rust
egui::ScrollArea::vertical().show(ui, |ui| {
    for i in 0..1000 {
        ui.label(format!("Item {}", i));
    }
});
```

**Key Features to Implement**:
- Persistent scroll offset state
- Automatic scrollbar visibility (show only when needed)
- Mouse wheel scrolling
- Drag-to-scroll (touch-friendly)
- `stick_to_bottom` for log viewers
- `scroll_to_cursor()` for programmatic scrolling
- Kinetic scrolling (momentum)

**State Structure**:
```rust
pub struct ScrollState {
    pub offset: Vec2,
    show_scroll: Vec2b,
    content_is_too_large: Vec2b,
    vel: Vec2,  // Kinetic scrolling velocity
    scroll_stuck_to_end: Vec2b,
}
```

---

### 2. Widget Trait (Medium Priority)

**Problem**: Widgets are methods on `UiContext` rather than implementing a trait. This prevents:
- Custom widgets from being used with `ui.add()`
- Composition patterns
- `impl Widget for FnOnce(&mut Ui) -> Response`

**egui Reference** (`crates/egui/src/widgets/mod.rs`):
```rust
pub trait Widget {
    fn ui(self, ui: &mut Ui) -> Response;
}

// Enables:
ui.add(Button::new("Click me"));
ui.add(my_custom_widget);

// Also enables closures as widgets:
ui.add(|ui: &mut Ui| {
    ui.label("Hello");
    Response::new()
});
```

**Migration Path**:
1. Define `Widget` trait
2. Create builder structs for existing widgets (`Button`, `Checkbox`, `Slider`)
3. Keep legacy methods on `UiContext` for backwards compatibility
4. Gradually migrate to builder pattern

---

### 3. Sense Type (Medium Priority)

**Problem**: Interaction intent is implicit per widget, not explicit.

**egui Reference** (`crates/egui/src/sense.rs`):
```rust
pub struct Sense(u8);

bitflags::bitflags! {
    impl Sense: u8 {
        const CLICK = 1<<0;
        const DRAG = 1<<1;
        const FOCUSABLE = 1<<2;
    }
}

impl Sense {
    pub fn click() -> Self { Self::CLICK | Self::FOCUSABLE }
    pub fn drag() -> Self { Self::DRAG | Self::FOCUSABLE }
    pub fn click_and_drag() -> Self { Self::CLICK | Self::DRAG | Self::FOCUSABLE }
}
```

**Use Cases**:
- Make labels clickable: `Label::new("...").sense(Sense::click())`
- Sliders that sense both click and drag
- Custom drag behaviors

---

### 4. Enhanced Response Type (Medium Priority)

**Current Response** (minimal):
```rust
pub struct Response {
    pub clicked: bool,
    pub hovered: bool,
    pub active: bool,
    pub changed: bool,
    pub bounds: Rect2D,
}
```

**egui Reference** (`crates/egui/src/response.rs`) - 500+ lines with:

| Method | Purpose |
|--------|---------|
| `drag_delta()` | How far dragged this frame |
| `total_drag_delta()` | Total drag distance from start |
| `double_clicked()`, `triple_clicked()` | Multi-click detection |
| `scroll_to_me()` | Scroll parent container to show widget |
| `on_hover_text()` | Show tooltip on hover |
| `on_hover_ui()` | Custom tooltip content |
| `context_menu()` | Right-click context menu |
| `has_focus()`, `gained_focus()`, `lost_focus()` | Focus tracking |
| `dnd_set_drag_payload()`, `dnd_hover_payload()` | Drag-and-drop |
| `interact(sense)` | Add additional sensing |
| `union(other)` | Combine responses (`|` operator) |

**InnerResponse Pattern**:
```rust
pub struct InnerResponse<R> {
    pub inner: R,        // Return value from closure
    pub response: Response,  // Interaction for whole container
}

// Usage:
let result = ui.horizontal(|ui| {
    ui.button("one");
    ui.button("two");
    "computed value"
});
// result.inner == "computed value"
// result.response == Response for horizontal area
```

---

### 5. Drag-and-Drop (Medium Priority)

**egui Reference** (`crates/egui/src/drag_and_drop.rs`):
```rust
// Source widget
if response.drag_started() {
    crate::DragAndDrop::set_payload(&ctx, my_data);
}

// Target widget
if let Some(payload) = response.dnd_hover_payload::<MyData>() {
    // Highlight drop target
}
if let Some(payload) = response.dnd_release_payload::<MyData>() {
    // Handle drop
}
```

**Use Cases**:
- Reordering list items
- Moving assets between folders
- Dropping materials onto meshes
- Dragging entities into groups

---

### 6. Tables/Grid (Low Priority)

**imgui Reference**:
```cpp
ImGui::BeginTable("table", 3);
ImGui::TableSetupColumn("Name");
ImGui::TableSetupColumn("Type");
ImGui::TableSetupColumn("Size");
ImGui::TableHeadersRow();

for (auto& item : items) {
    ImGui::TableNextRow();
    ImGui::TableSetColumnIndex(0);
    ImGui::Text("%s", item.name);
    // ...
}
ImGui::EndTable();
```

**Features**:
- Sortable columns
- Resizable columns
- Row selection
- Scrollable body with fixed header

---

## Code Organization Issues

### Large Files Need Splitting

| File | Lines | Status |
|------|-------|--------|
| `popup.rs` | ~959 | ✅ Refactored (was 1228) |
| `widgets.rs` | ~835 | ⚠️ Could split if needed |

### Code Duplication

~~**Popup Item Drawing**~~ - ✅ Fixed: Unified into `menu_item_clicked_*` methods
~~**Background Drawing**~~ - ✅ Fixed: Single `draw_popup_background()` helper

---

## Patterns to Adopt

### 1. UiBuilder Pattern

**egui** (`crates/egui/src/ui_builder.rs`):
```rust
let child_ui = ui.new_child(
    UiBuilder::new()
        .id_salt("my_area")
        .max_rect(rect)
        .layout(Layout::left_to_right())
        .sense(Sense::click())
);
```

**Benefit**: Consistent child UI creation with all options in one place.

### 2. Layer System with Order

**egui** (`crates/egui/src/layers.rs`):
```rust
pub enum Order {
    Background,  // Behind all floating windows
    Middle,      // Normal windows
    Foreground,  // Popups, menus
    Tooltip,     // Tooltips (no interaction)
    Debug,       // Debug overlay (always on top)
}
```

**Current katla**: Has `z_index` constants but no proper layer objects.

### 3. Persistent State with TypeId

**egui**:
```rust
ctx.data_mut(|d| d.get_persisted::<State>(id))
ctx.data_mut(|d| d.insert_persisted(id, state))
```

**Current katla**: Enum-based with manual accessors:
```rust
enum WidgetState { Checkbox(bool), Slider(f32), TextInput(String), ... }
```

**Benefit of egui approach**: No need to update enum for new state types.

---

## Implementation Roadmap

### Phase 1: Foundation
- [x] Unify popup API into closure-based pattern (commit 4815923)
- [x] Extract common helpers (`draw_popup_background`, `menu_item_clicked_*`)
- [ ] Split `widgets.rs` into widget-specific files (optional)

### Phase 2: Response Enhancement
- [ ] Add `drag_delta()` and `total_drag_delta()`
- [ ] Add `on_hover_text()` and `on_hover_ui()`
- [ ] Add `double_clicked()` detection
- [ ] Add `union()` / `|` operator
- [ ] Add `InnerResponse<R>` type

### Phase 3: Container Widgets
- [ ] Implement `ScrollArea` with:
  - [ ] Vertical/horizontal scrolling
  - [ ] Mouse wheel support
  - [ ] Auto-hiding scrollbars
  - [ ] `stick_to_bottom` option
  - [ ] `scroll_to_cursor()` method

### Phase 4: Widget Trait System
- [ ] Define `Widget` trait
- [ ] Create builder structs for existing widgets
- [ ] Implement `Sense` type
- [ ] Add `ui.add()` method

### Phase 5: Advanced Features
- [ ] Implement drag-and-drop
- [ ] Implement tables (optional)
- [ ] Keyboard navigation (optional)

---

## References

### Codebases Studied
- [egui](https://github.com/emilk/egui) - Rust immediate mode UI
- [imgui](https://github.com/ocornut/imgui) - C++ immediate mode UI

### Key Files
- `egui/crates/egui/src/response.rs` - Rich response pattern
- `egui/crates/egui/src/sense.rs` - Sense type
- `egui/crates/egui/src/containers/scroll_area.rs` - ScrollArea implementation
- `egui/crates/egui/src/layers.rs` - Layer system
- `egui/crates/egui/src/widgets/mod.rs` - Widget trait
- `egui/crates/egui/src/ui_builder.rs` - UiBuilder pattern
