# UI Widgets Implementation Plan

## Overview

Extended widget set for katla_ui, providing menu systems, popups, and selection widgets.

## Status

- [x] Menu Item widget
- [x] Selectable widget
- [x] Popup container (begin_popup/end_popup)
- [x] Dropdown menu (begin_dropdown/end_dropdown)
- [x] Context menu (begin_context_menu/end_context_menu)
- [x] Combo box (begin_combo/end_combo)
- [x] Style properties for menus and popups

## API Reference

### Menu Item

Simple clickable item styled for menus.

```rust
pub fn menu_item(&mut self, id: &str, label: &str, bounds: Rect2D) -> bool
```

**Returns**: `true` if clicked this frame.

**Usage**:
```rust
if ui.menu_item("new", "New File", item_bounds) {
    // Handle New File action
}
```

### Selectable

Clickable item with selection state, useful for lists.

```rust
pub fn selectable(&mut self, id: &str, label: &str, selected: bool, bounds: Rect2D) -> bool
```

**Parameters**:
- `selected`: Whether the item is currently selected (affects styling)

**Returns**: `true` if clicked this frame.

**Usage**:
```rust
let mut selected_item = 0;
for (i, item) in items.iter().enumerate() {
    let is_selected = i == selected_item;
    if ui.selectable(&format!("item_{}", i), item, is_selected, bounds) {
        selected_item = i;
    }
}
```

### Popup Container

Base popup with clipping and click-outside-to-close.

```rust
pub fn begin_popup(&mut self, id: &str, bounds: Rect2D) -> bool
pub fn end_popup(&mut self)
```

**Returns**: `true` if the popup is open and contents should be drawn.

**Usage**:
```rust
if ui.begin_popup("my_popup", popup_bounds) {
    ui.menu_item("option1", "Option 1", item1_bounds);
    ui.menu_item("option2", "Option 2", item2_bounds);
    ui.end_popup();
}
```

### Dropdown Menu

Button that opens a popup menu below it.

```rust
pub fn begin_dropdown(&mut self, id: &str, label: &str, bounds: Rect2D) -> bool
pub fn end_dropdown(&mut self)
```

**Returns**: `true` if the dropdown is open and menu items should be drawn.

**Usage**:
```rust
if ui.begin_dropdown("file_menu", "File", button_bounds) {
    let item_height = ui.menu_item_height();
    // Draw menu items...
    ui.menu_item("new", "New", item_bounds);
    ui.menu_item("open", "Open", item_bounds);
    ui.end_dropdown();
}
```

### Context Menu

Right-click popup at mouse position.

```rust
pub fn open_context_menu(&mut self, id: &str) -> bool
pub fn begin_context_menu(&mut self, id: &str) -> bool
pub fn end_context_menu(&mut self)
pub fn is_context_menu_open(&mut self, id: &str) -> bool
```

**Usage**:
```rust
// In your widget area
if ui.open_context_menu("canvas_context") {
    // Context menu was just opened
}

if ui.begin_context_menu("canvas_context") {
    ui.menu_item("copy", "Copy", item_bounds);
    ui.menu_item("paste", "Paste", item_bounds);
    ui.end_context_menu();
}
```

### Combo Box

Dropdown with selectable items.

```rust
pub fn begin_combo(&mut self, id: &str, preview: &str, bounds: Rect2D) -> bool
pub fn end_combo(&mut self)
```

**Parameters**:
- `preview`: Text shown in the closed combo box

**Returns**: `true` if the combo is open and items should be drawn.

**Usage**:
```rust
let options = ["Option A", "Option B", "Option C"];
let mut selected = 0;

if ui.begin_combo("my_combo", options[selected], combo_bounds) {
    for (i, option) in options.iter().enumerate() {
        let item_bounds = Rect2D::from_origin_size(
            Vec2::new(x, y + i as f32 * ui.menu_item_height()),
            Vec2::new(width, ui.menu_item_height()),
        );
        if ui.selectable(&format!("opt_{}", i), option, i == selected, item_bounds) {
            selected = i;
            ui.close_current_popup();
        }
    }
    ui.end_combo();
}
```

### Utility Methods

```rust
/// Close the current popup/dropdown/context menu.
pub fn close_current_popup(&mut self)

/// Get the menu item height for layout.
pub fn menu_item_height(&self) -> f32
```

## Style Properties

Menu and popup styling is controlled through `UiStyle`:

```rust
pub struct UiStyle {
    // Menu styling
    pub menu_bg: Color,          // Background color for menus
    pub menu_hovered: Color,     // Menu item color when hovered
    pub menu_active: Color,      // Menu item color when pressed
    pub menu_border: Color,      // Menu border color
    pub menu_rounding: f32,      // Corner rounding radius
    pub menu_item_height: f32,   // Height of each menu item
    pub menu_padding: f32,       // Padding inside menus
    pub menu_min_width: f32,     // Minimum width for menus

    // Popup styling
    pub popup_bg: Color,         // Background color
    pub popup_border: Color,     // Border color
    pub popup_shadow: Color,     // Shadow color (drawn behind)
    pub popup_rounding: f32,     // Corner rounding radius

    // Selectable styling
    pub selectable_hovered: Color,   // Hover background
    pub selectable_selected: Color,  // Selected background

    // Combo box styling
    pub combo_bg: Color,         // Background color
    pub combo_border: Color,     // Border color
    pub combo_hovered: Color,    // Hover color
    pub combo_text: Color,       // Preview text color
}
```

## Implementation Notes

### State Management

The UI context tracks popup state across frames:
- `popup_id`: Currently open popup's widget ID
- `popup_bounds`: Popup bounds for click-outside detection
- `popup_opened_this_frame`: Prevents immediate close on open

### Click-Outside Detection

Popups automatically close when:
1. User clicks outside the popup bounds
2. `close_current_popup()` is called

### Layout Helper

Use `menu_item_height()` to properly layout menu items:

```rust
let item_height = ui.menu_item_height();
let item_bounds = Rect2D::from_origin_size(
    Vec2::new(popup_x, popup_y + index as f32 * item_height),
    Vec2::new(popup_width, item_height),
);
```
