//! Layout system for UI positioning.
//!
//! Provides cursor-based layout, horizontal/vertical layout containers,
//! and automatic widget positioning.

use katla_math::{Rect2D, Vec2};

use super::UiContext;

/// Layout direction for container widgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutDirection {
    /// Horizontal layout (left to right).
    Horizontal,
    /// Vertical layout (top to bottom).
    Vertical,
}

/// Layout state for nested containers.
#[derive(Debug, Clone)]
pub struct LayoutState {
    /// Layout direction.
    pub direction: LayoutDirection,
    /// Starting position for this layout.
    pub start_pos: Vec2,
    /// Current position within the layout.
    pub cursor: Vec2,
    /// Maximum item size seen so far (for alignment).
    pub max_item_size: Vec2,
    /// Spacing between items.
    pub spacing: f32,
}

impl UiContext {
    // -------------------------------------------------------------------------
    // Basic Layout Helpers
    // -------------------------------------------------------------------------

    /// Set the cursor position for automatic layout.
    pub fn set_cursor(&mut self, pos: Vec2) {
        self.cursor = pos;
    }

    /// Get the current cursor position.
    pub fn cursor(&self) -> Vec2 {
        self.cursor
    }

    /// Move cursor to next line.
    pub fn newline(&mut self) {
        self.cursor = Vec2::new(
            0.0,
            self.cursor.y() + self.row_height + self.style.item_spacing,
        );
        self.row_height = 0.0;
    }

    /// Get bounds for the next item in a horizontal layout.
    pub fn next_item(&mut self, size: Vec2) -> Rect2D {
        let bounds = Rect2D::from_origin_size(self.cursor, size);
        self.cursor = Vec2::new(
            self.cursor.x() + size.x() + self.style.item_spacing,
            self.cursor.y(),
        );
        self.row_height = self.row_height.max(size.y());
        bounds
    }

    /// Begin a horizontal layout row.
    pub fn layout_row(&mut self, height: f32) {
        self.row_height = height;
    }

    /// Add a spacer of the given width (in horizontal layout) or height (in vertical layout).
    pub fn spacer(&mut self, size: f32) {
        self.cursor = Vec2::new(
            self.cursor.x() + size,
            self.cursor.y(),
        );
    }

    // -------------------------------------------------------------------------
    // Flex Layout System
    // -------------------------------------------------------------------------

    /// Begin a horizontal layout container.
    ///
    /// Widgets added inside the closure will be positioned left to right
    /// with automatic spacing.
    ///
    /// # Example
    /// ```ignore
    /// ui.horizontal(|ui| {
    ///     ui.button("btn1", "One", bounds1);
    ///     ui.button("btn2", "Two", bounds2);
    ///     ui.spacer(10.0);
    ///     ui.button("btn3", "Three", bounds3);
    /// });
    /// ```
    pub fn horizontal<F>(&mut self, f: F)
    where
        F: FnOnce(&mut Self),
    {
        let start_cursor = self.cursor;
        self.layout_stack.push(LayoutState {
            direction: LayoutDirection::Horizontal,
            start_pos: start_cursor,
            cursor: start_cursor,
            max_item_size: Vec2::new(0.0, 0.0),
            spacing: self.style.item_spacing,
        });

        f(self);

        // SAFETY: We just pushed to layout_stack, so pop must succeed
        let layout = self.layout_stack.pop().unwrap();
        self.cursor = Vec2::new(
            layout.start_pos.x(),
            layout.cursor.y() + layout.max_item_size.y(),
        );
        self.row_height = self.row_height.max(layout.max_item_size.y());
    }

    /// Begin a vertical layout container.
    ///
    /// Widgets added inside the closure will be positioned top to bottom
    /// with automatic spacing.
    ///
    /// # Example
    /// ```ignore
    /// ui.vertical(|ui| {
    ///     ui.label("Name:", bounds1);
    ///     ui.text_input("name", &mut name, bounds2);
    /// });
    /// ```
    pub fn vertical<F>(&mut self, f: F)
    where
        F: FnOnce(&mut Self),
    {
        let start_cursor = self.cursor;
        self.layout_stack.push(LayoutState {
            direction: LayoutDirection::Vertical,
            start_pos: start_cursor,
            cursor: start_cursor,
            max_item_size: Vec2::new(0.0, 0.0),
            spacing: self.style.item_spacing,
        });

        f(self);

        // SAFETY: We just pushed to layout_stack, so pop must succeed
        let layout = self.layout_stack.pop().unwrap();
        self.cursor = Vec2::new(
            layout.start_pos.x(),
            layout.cursor.y() + self.style.item_spacing,
        );
    }

    /// Begin a columned layout.
    ///
    /// Divides the available width into `count` columns and calls the closure
    /// for each column with the column index.
    ///
    /// # Example
    /// ```ignore
    /// ui.columns(3, |ui, col| {
    ///     match col {
    ///         0 => ui.label("Column 1", bounds),
    ///         1 => ui.label("Column 2", bounds),
    ///         2 => ui.label("Column 3", bounds),
    ///         _ => {}
    ///     }
    /// });
    /// ```
    pub fn columns<F>(&mut self, count: usize, mut f: F)
    where
        F: FnMut(&mut Self, usize),
    {
        let start_cursor = self.cursor;
        let available_width = self.screen_size.x() - start_cursor.x();
        let column_width = available_width / count as f32;

        for i in 0..count {
            self.cursor = Vec2::new(
                start_cursor.x() + i as f32 * column_width,
                start_cursor.y(),
            );
            f(self, i);
        }

        // Reset cursor to below all columns
        self.cursor = Vec2::new(start_cursor.x(), start_cursor.y() + self.row_height);
    }

    /// Get bounds for the next item in the current layout (if any).
    ///
    /// Returns bounds with automatic position based on layout direction.
    /// Falls back to current cursor if no layout is active.
    pub fn layout_item(&mut self, size: Vec2) -> Rect2D {
        if let Some(layout) = self.layout_stack.last_mut() {
            let bounds = Rect2D::from_origin_size(layout.cursor, size);

            match layout.direction {
                LayoutDirection::Horizontal => {
                    layout.cursor = Vec2::new(
                        layout.cursor.x() + size.x() + layout.spacing,
                        layout.cursor.y(),
                    );
                }
                LayoutDirection::Vertical => {
                    layout.cursor = Vec2::new(
                        layout.cursor.x(),
                        layout.cursor.y() + size.y() + layout.spacing,
                    );
                }
            }

            layout.max_item_size = Vec2::new(
                layout.max_item_size.x().max(size.x()),
                layout.max_item_size.y().max(size.y()),
            );

            bounds
        } else {
            self.next_item(size)
        }
    }

    /// Check if we're inside a layout container.
    pub fn in_layout(&self) -> bool {
        !self.layout_stack.is_empty()
    }

    /// Get the current layout direction (if any).
    pub fn layout_direction(&self) -> Option<LayoutDirection> {
        self.layout_stack.last().map(|l| l.direction)
    }
}
