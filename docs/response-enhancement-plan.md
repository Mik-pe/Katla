# Phase 2: Response Enhancement Plan

> **Goal**: Enrich the `Response` type with more interaction information and convenience methods

## Current State

```rust
pub struct Response {
    pub clicked: bool,
    pub hovered: bool,
    pub active: bool,
    pub changed: bool,
    pub bounds: Rect2D,
}
```

**Issues:**
- No drag tracking (delta, total distance)
- No double/triple click detection
- No hover text/UI helpers
- No way to combine responses from multiple widgets
- Closure-based containers can't return both result AND response

---

## Proposed Enhancements

### 1. Drag Tracking

```rust
pub struct Response {
    // ... existing fields ...

    /// Drag distance this frame (None if not dragging)
    pub drag_delta: Option<Vec2>,
    /// Total drag distance from start position
    pub total_drag: Option<Vec2>,
    /// Starting position of drag (if dragging)
    pub drag_start: Option<Vec2>,
}

impl Response {
    /// Check if a drag started this frame
    pub fn drag_started(&self) -> bool { ... }

    /// Check if currently dragging
    pub fn is_dragging(&self) -> bool { ... }

    /// Check if drag ended this frame
    pub fn drag_released(&self) -> bool { ... }

    /// Get drag delta (zero if not dragging)
    pub fn drag_delta(&self) -> Vec2 { ... }
}
```

**Use cases:**
- Slider dragging
- Window moving
- Scroll area panning
- Color picker dragging

### 2. Multi-Click Detection

```rust
pub struct Response {
    // ... existing fields ...
    /// Click count (1 = single, 2 = double, 3 = triple)
    pub click_count: u8,
}

impl Response {
    /// Check for double-click
    pub fn double_clicked(&self) -> bool {
        self.clicked && self.click_count == 2
    }

    /// Check for triple-click
    pub fn triple_clicked(&self) -> bool {
        self.clicked && self.click_count == 3
    }
}
```

**Implementation:**
- Track last click time and position per widget ID
- Reset count if >500ms elapsed or mouse moved >5px

### 3. Hover Helpers

```rust
impl Response {
    /// Show tooltip on hover (convenience method)
    pub fn on_hover_text(self, ui: &mut UiContext, text: &str) -> Self {
        if self.hovered && !self.active {
            ui.tooltip(text);
        }
        self
    }

    /// Show custom UI on hover
    pub fn on_hover_ui<F>(self, ui: &mut UiContext, f: F) -> Self
    where
        F: FnOnce(&mut UiContext)
    {
        if self.hovered && !self.active {
            // Save state, draw hover UI, restore
            f(ui);
        }
        self
    }
}
```

**Usage:**
```rust
if ui.button_response("btn", "Delete", bounds)
    .on_hover_text(ui, "Delete the selected item")
    .clicked
{
    delete_item();
}
```

### 4. Response Combination

```rust
impl Response {
    /// Combine two responses (union of interactions)
    pub fn union(self, other: Self) -> Self {
        Response {
            clicked: self.clicked || other.clicked,
            hovered: self.hovered || other.hovered,
            active: self.active || other.active,
            changed: self.changed || other.changed,
            bounds: self.bounds.union(other.bounds),
            // ... merge other fields
        }
    }
}

impl BitOr for Response {
    type Output = Self;
    fn bitor(self, other: Self) -> Self {
        self.union(other)
    }
}
```

**Usage:**
```rust
let resp = ui.button("a", "A", bounds_a);
let resp = resp | ui.button("b", "B", bounds_b);  // Combined response
```

### 5. InnerResponse Pattern

For closure-based containers that need to return both a value and response:

```rust
pub struct InnerResponse<R> {
    /// Inner value from closure
    pub inner: R,
    /// Response for the whole container
    pub response: Response,
}

impl UiContext {
    pub fn horizontal<R, F>(&mut self, id: &str, f: F) -> InnerResponse<R>
    where
        F: FnOnce(&mut Self) -> R
    {
        let start_bounds = self.cursor;
        let inner = f(self);
        let end_bounds = self.cursor;

        InnerResponse {
            inner,
            response: Response {
                bounds: start_bounds.union(end_bounds),
                // ... detect hover/click for whole area
            },
        }
    }
}
```

**Usage:**
```rust
let result = ui.horizontal("row", |ui| {
    ui.button("one", "One", bounds1);
    ui.button("two", "Two", bounds2);
    "computed value"
});
// result.inner == "computed value"
// result.response == Response for whole horizontal area
```

---

## Implementation Order

1. **Add drag tracking fields** - Add fields, update constructors, add helper methods
2. **Add click count tracking** - Need storage for last click time/position per widget
3. **Add hover helpers** - `on_hover_text()` method
4. **Add union/BitOr** - Simple implementation
5. **Add InnerResponse** - New type, update horizontal/vertical layout helpers

---

## Storage Requirements

For click counting, need to track per-widget:
```rust
struct ClickState {
    last_click_time: f64,  // or Instant
    last_click_pos: Vec2,
    count: u8,
}
```

Add to `WidgetStorage` or separate `HashMap<WidgetId, ClickState>`.

---

## Files to Modify

- `katla_ui/src/lib.rs` - Response struct
- `katla_ui/src/context/mod.rs` - UiContext storage for click tracking
- `katla_ui/src/context/widgets/basic.rs` - Update button_behavior to return Response
- `katla_ui/src/context/widgets/selectable.rs` - Update selectable to return Response

---

## Open Questions

1. **Should widgets return Response by default?** Currently most return bool.
   - Option A: Keep bool, add `*_response()` variants
   - Option B: Migrate all to Response (breaking change)

2. **How to track click time?** Need access to timing:
   - Add `time: f64` to UiContext (updated each frame)
   - Use `std::time::Instant` internally

3. **InnerResponse for existing closures?** Horizontal/vertical don't currently track bounds.
   - Need to add bounds tracking to layout helpers
