# Popup System Refactoring Plan

> **Status: ✅ COMPLETE** (commit 4815923)

## Summary

Successfully consolidated 8 different popup patterns into ONE unified, closure-based API.

### Results

| Metric | Before | After |
|--------|--------|-------|
| Popup patterns | 8 | 1 |
| popup.rs lines | 1228 | 959 |
| Net change | - | -269 lines |

## Final Unified API

```rust
// Container popups (closure-based, auto-layout)
ui.context_menu("id", |ui| { ... })
ui.dropdown("id", trigger_bounds, |ui| { ... })
ui.modal("id", width, height, |ui| { ... })
ui.menu_bar_dropdown("id", "Label", bounds, |ui| { ... })

// Items inside popups (auto-positioning)
ui.menu_item_clicked("Label")
ui.menu_item_clicked_with_icon("Label", icon)
ui.menu_item_clicked_with_icon_and_shortcut("Label", icon, enabled, "Ctrl+S")
ui.toggle_menu_item_clicked("Label", checked)
ui.menu_separator()
```

## Core Types

```rust
pub struct Popup {
    id: String,
    position: PopupPosition,
    style: PopupStyle,
    close_behavior: CloseBehavior,
}

pub enum PopupPosition {
    AtCursor,
    AtPosition(Vec2),
    BelowButton(Rect2D),
    Fixed(Rect2D),
    Centered { width: f32, height: f32 },
}

pub enum PopupStyle {
    Menu,
    Modal,
    Tooltip,
}

pub enum CloseBehavior {
    ClickOutside,
    ExplicitOnly,
}
```

## Removed Code

- `DeferredDraw` enum + `defer_rect`/`defer_text` helpers
- `end_dropdown()` (dead code)
- `popup_auto()`, `begin_auto_popup()`, `end_auto_popup()`
- `popup_item()`, `popup_item_with_shortcut()`, `popup_separator()`
- `dropdown_deferred` field from UiContext
- All begin/end popup patterns replaced with closure-based API

## Files Modified

- `katla_ui/src/context/popup.rs` - Core popup system
- `katla_ui/src/context/mod.rs` - Removed DeferredDraw export
- `katla_app/src/ui/editor_ui.rs` - Migrated menu bar dropdowns
- `katla_app/src/ui/debug_overlay.rs` - Migrated context menu
- `katla_app/src/ui/asset_browser.rs` - Migrated modal and context menus
