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
    /// Number of columns for grid layouts (0 for non-grid layouts).
    pub grid_columns: usize,
}

impl UiContext {
    // -------------------------------------------------------------------------
    // Basic Layout Helpers
    // -------------------------------------------------------------------------

    /// Set the cursor position for automatic layout.
    ///
    /// When inside a layout container, updates the layout cursor.
    /// Otherwise updates the main cursor.
    pub fn set_cursor(&mut self, pos: Vec2) {
        if let Some(layout) = self.layout_stack.last_mut() {
            layout.cursor = pos;
        } else {
            self.cursor = pos;
        }
    }

    /// Get the current cursor position.
    ///
    /// If inside a layout container (horizontal, vertical, grid, etc.),
    /// returns the layout's cursor position. Otherwise returns the main cursor.
    pub fn cursor(&self) -> Vec2 {
        self.layout_stack
            .last()
            .map(|l| l.cursor)
            .unwrap_or(self.cursor)
    }

    /// Move cursor to next line.
    pub(crate) fn newline(&mut self) {
        self.cursor = Vec2::new(
            0.0,
            self.cursor.y() + self.row_height + self.style.item_spacing,
        );
        self.row_height = 0.0;
    }

    /// Get bounds for the next item in a horizontal layout.
    pub(crate) fn next_item(&mut self, size: Vec2) -> Rect2D {
        let bounds = Rect2D::from_origin_size(self.cursor, size);
        self.cursor = Vec2::new(
            self.cursor.x() + size.x() + self.style.item_spacing,
            self.cursor.y(),
        );
        self.row_height = self.row_height.max(size.y());
        bounds
    }

    /// Begin a horizontal layout row.
    pub(crate) fn layout_row(&mut self, height: f32) {
        self.row_height = height;
    }

    /// Add a spacer of the given width (in horizontal layout) or height (in vertical layout).
    pub fn spacer(&mut self, size: f32) {
        if let Some(layout) = self.layout_stack.last_mut() {
            layout.cursor = Vec2::new(layout.cursor.x() + size, layout.cursor.y());
        } else {
            self.cursor = Vec2::new(self.cursor.x() + size, self.cursor.y());
        }
    }

    /// Advance the cursor after placing a widget.
    ///
    /// Call this after positioning a widget to move the cursor for the next item.
    /// In horizontal layouts, advances horizontally. In vertical layouts, advances vertically.
    ///
    /// # Example
    /// ```ignore
    /// let bounds = Rect2D::from_origin_size(ui.cursor(), Vec2::new(100.0, 28.0));
    /// ui.add(Button::new("Click").bounds(bounds));
    /// ui.advance_cursor(Vec2::new(100.0, 28.0)); // Move to next position
    /// ```
    pub fn advance_cursor(&mut self, size: Vec2) {
        if let Some(layout) = self.layout_stack.last_mut() {
            match layout.direction {
                LayoutDirection::Horizontal => {
                    layout.cursor = Vec2::new(
                        layout.cursor.x() + size.x() + layout.spacing,
                        layout.cursor.y(),
                    );
                    layout.max_item_size = Vec2::new(
                        layout.max_item_size.x().max(size.x()),
                        layout.max_item_size.y().max(size.y()),
                    );
                }
                LayoutDirection::Vertical => {
                    layout.cursor = Vec2::new(
                        layout.cursor.x(),
                        layout.cursor.y() + size.y() + layout.spacing,
                    );
                    layout.max_item_size = Vec2::new(
                        layout.max_item_size.x().max(size.x()),
                        layout.max_item_size.y().max(size.y()),
                    );
                }
            }
        } else {
            // Default: advance vertically
            self.cursor = Vec2::new(
                self.cursor.x(),
                self.cursor.y() + size.y() + self.style.item_spacing,
            );
            self.row_height = self.row_height.max(size.y());
        }
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
            grid_columns: 0,
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
            grid_columns: 0,
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
    pub(crate) fn columns<F>(&mut self, count: usize, mut f: F)
    where
        F: FnMut(&mut Self, usize),
    {
        let start_cursor = self.cursor;
        let available_width = self.screen_size.x() - start_cursor.x();
        let column_width = available_width / count as f32;

        for i in 0..count {
            self.cursor = Vec2::new(start_cursor.x() + i as f32 * column_width, start_cursor.y());
            f(self, i);
        }

        // Reset cursor to below all columns
        self.cursor = Vec2::new(start_cursor.x(), start_cursor.y() + self.row_height);
    }

    /// Get bounds for the next item in the current layout (if any).
    ///
    /// Returns bounds with automatic position based on layout direction.
    /// Falls back to current cursor if no layout is active.
    pub(crate) fn layout_item(&mut self, size: Vec2) -> Rect2D {
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
    pub(crate) fn in_layout(&self) -> bool {
        !self.layout_stack.is_empty()
    }

    /// Get the current layout direction (if any).
    pub(crate) fn layout_direction(&self) -> Option<LayoutDirection> {
        self.layout_stack.last().map(|l| l.direction)
    }

    // -------------------------------------------------------------------------
    // Convenience Layout Helpers
    // -------------------------------------------------------------------------

    /// Move cursor back to continue on the same line.
    ///
    /// After calling `newline()` or adding vertical widgets, this moves
    /// the cursor back up to continue adding widgets horizontally.
    ///
    /// # Example
    /// ```ignore
    /// ui.label("Name:", bounds1);
    /// ui.same_line();
    /// ui.text_input("name", &mut name, bounds2);
    /// ```
    pub(crate) fn same_line(&mut self) {
        if self.row_height > 0.0 {
            // Move cursor back up to the current row
            self.cursor = Vec2::new(
                self.cursor.x(),
                self.cursor.y() - self.row_height - self.style.item_spacing,
            );
        }
    }

    /// Add spacing in the current layout direction.
    ///
    /// In horizontal layouts, adds horizontal spacing.
    /// In vertical layouts (or no layout), adds vertical spacing.
    ///
    /// # Example
    /// ```ignore
    /// ui.button("btn1", bounds1);
    /// ui.spacing(10.0);
    /// ui.button("btn2", bounds2);
    /// ```
    pub fn spacing(&mut self, amount: f32) {
        if let Some(layout) = self.layout_stack.last_mut() {
            match layout.direction {
                LayoutDirection::Horizontal => {
                    layout.cursor = Vec2::new(layout.cursor.x() + amount, layout.cursor.y());
                }
                LayoutDirection::Vertical => {
                    layout.cursor = Vec2::new(layout.cursor.x(), layout.cursor.y() + amount);
                }
            }
        } else {
            // Default to vertical spacing when no layout is active
            self.cursor = Vec2::new(self.cursor.x(), self.cursor.y() + amount);
        }
    }

    /// Indent the cursor by a given amount (horizontal offset).
    ///
    /// Useful for creating hierarchical UIs or nested content.
    ///
    /// # Example
    /// ```ignore
    /// ui.indent(20.0);
    /// ui.label("Nested content", bounds);
    /// ```
    pub(crate) fn indent(&mut self, amount: f32) {
        if let Some(layout) = self.layout_stack.last_mut() {
            layout.cursor = Vec2::new(layout.cursor.x() + amount, layout.cursor.y());
        } else {
            self.cursor = Vec2::new(self.cursor.x() + amount, self.cursor.y());
        }
    }

    /// Unindent the cursor by a given amount (horizontal offset).
    ///
    /// Moves the cursor left by the specified amount.
    pub(crate) fn unindent(&mut self, amount: f32) {
        if let Some(layout) = self.layout_stack.last_mut() {
            layout.cursor = Vec2::new(layout.cursor.x() - amount, layout.cursor.y());
        } else {
            self.cursor = Vec2::new(self.cursor.x() - amount, self.cursor.y());
        }
    }

    /// Execute a closure with an indented cursor, automatically restoring after.
    ///
    /// This is an RAII-style helper that ensures the cursor is restored
    /// even if the closure panics.
    ///
    /// # Example
    /// ```ignore
    /// ui.with_indent(20.0, |ui| {
    ///     ui.label("Nested item 1", bounds1);
    ///     ui.label("Nested item 2", bounds2);
    /// }); // cursor automatically restored
    /// ```
    pub(crate) fn with_indent<F, R>(&mut self, amount: f32, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        let original_x = self.cursor().x();
        self.indent(amount);
        let result = f(self);
        // Restore: move back by amount from current position
        if let Some(layout) = self.layout_stack.last_mut() {
            layout.cursor = Vec2::new(original_x, layout.cursor.y());
        } else {
            self.cursor = Vec2::new(original_x, self.cursor.y());
        }
        result
    }

    /// Begin a horizontal row layout from the current cursor position.
    ///
    /// This is a simpler alternative to `horizontal()` that doesn't use
    /// a closure. Remember to call `end_row()` when done.
    ///
    /// # Example
    /// ```ignore
    /// ui.begin_row();
    /// ui.button("One", bounds1);
    /// ui.button("Two", bounds2);
    /// ui.button("Three", bounds3);
    /// ui.end_row();
    /// ```
    pub fn begin_row(&mut self) {
        let start_cursor = self.cursor;
        self.layout_stack.push(LayoutState {
            direction: LayoutDirection::Horizontal,
            start_pos: start_cursor,
            cursor: start_cursor,
            max_item_size: Vec2::new(0.0, 0.0),
            spacing: self.style.item_spacing,
            grid_columns: 0,
        });
    }

    /// End a horizontal row layout.
    ///
    /// Advances the cursor below the row. Must be paired with `begin_row()`.
    pub fn end_row(&mut self) {
        if let Some(layout) = self.layout_stack.pop() {
            self.cursor = Vec2::new(
                layout.start_pos.x(),
                layout.cursor.y() + layout.max_item_size.y(),
            );
            self.row_height = self.row_height.max(layout.max_item_size.y());
        }
    }

    /// Begin a vertical column layout from the current cursor position.
    ///
    /// This is a simpler alternative to `vertical()` that doesn't use
    /// a closure. Remember to call `end_column()` when done.
    ///
    /// # Example
    /// ```ignore
    /// ui.begin_column();
    /// ui.label("Item 1", bounds1);
    /// ui.label("Item 2", bounds2);
    /// ui.label("Item 3", bounds3);
    /// ui.end_column();
    /// ```
    pub fn begin_column(&mut self) {
        let start_cursor = self.cursor;
        self.layout_stack.push(LayoutState {
            direction: LayoutDirection::Vertical,
            start_pos: start_cursor,
            cursor: start_cursor,
            max_item_size: Vec2::new(0.0, 0.0),
            spacing: self.style.item_spacing,
            grid_columns: 0,
        });
    }

    /// End a vertical column layout.
    ///
    /// Advances the cursor below the column. Must be paired with `begin_column()`.
    pub fn end_column(&mut self) {
        if let Some(layout) = self.layout_stack.pop() {
            self.cursor = Vec2::new(layout.start_pos.x(), layout.cursor.y() + layout.spacing);
        }
    }

    // -------------------------------------------------------------------------
    // Grid Layout Helper
    /// Begin a grid layout from the current cursor position.
    ///
    /// A grid divides available width into equal columns. Items are added
    /// left-to-right, top-to-bottom with automatic row wrapping.
    ///
    /// Remember to call `end_grid()` when done with the grid.
    ///
    /// # Example
    /// ```ignore
    /// ui.begin_grid(3, 200.0, 24.0, 8.0);
    /// for item in items {
    ///     let bounds = ui.grid_item(item.size());
    ///     ui.add(Button::new(item.name).bounds(bounds));
    /// }
    /// ui.end_grid();
    /// ```
    ///
    /// # Arguments
    /// * `columns` - Number of columns in the grid
    /// * `item_width` - Width of each grid item
    /// * `item_height` - Height of each grid item (used for row wrapping)
    /// * `spacing` - Spacing between items
    pub fn begin_grid(&mut self, columns: usize, item_width: f32, item_height: f32, spacing: f32) {
        let start_cursor = self.cursor;
        self.layout_stack.push(LayoutState {
            direction: LayoutDirection::Horizontal,
            start_pos: start_cursor,
            cursor: start_cursor,
            max_item_size: Vec2::new(item_width, item_height),
            spacing,
            grid_columns: columns,
        });
    }

    /// Get bounds for the next item in a grid layout with automatic row wrapping.
    ///
    /// This method is specifically designed for grid layouts created with `begin_grid()`.
    /// It positions items left-to-right and wraps to a new row when the current row is full.
    ///
    /// # Arguments
    /// * `size` - Size of the grid item (width should match begin_grid's item_width)
    ///
    /// # Returns
    /// Bounds for the next grid item position
    ///
    /// # Example
    /// ```ignore
    /// ui.begin_grid(4, 100.0, 24.0, 8.0);
    /// for item in items {
    ///     let bounds = ui.grid_item(Vec2::new(100.0, 24.0));
    ///     ui.add(Button::new(item.name).bounds(bounds));
    /// }
    /// ui.end_grid();
    /// ```
    pub fn grid_item(&mut self, size: Vec2) -> Rect2D {
        if let Some(layout) = self.layout_stack.last_mut() {
            let columns = layout.grid_columns;
            let item_width = layout.max_item_size.x();
            let item_height = layout.max_item_size.y();

            // Calculate current column (0-indexed)
            let current_x = layout.cursor.x();
            let start_x = layout.start_pos.x();
            let item_with_spacing = item_width + layout.spacing;
            let column = ((current_x - start_x) / item_with_spacing) as usize;

            // Check if we need to wrap to next row
            if column >= columns {
                // Move to start of next row
                layout.cursor =
                    Vec2::new(start_x, layout.cursor.y() + item_height + layout.spacing);
            }

            let bounds = Rect2D::from_origin_size(layout.cursor, size);

            // Advance cursor for next item
            layout.cursor = Vec2::new(
                layout.cursor.x() + item_width + layout.spacing,
                layout.cursor.y(),
            );

            // Track max height for end_grid
            layout.max_item_size = Vec2::new(
                layout.max_item_size.x().max(size.x()),
                layout.max_item_size.y().max(size.y()),
            );

            bounds
        } else {
            // Not in a grid, fall back to layout_item
            self.layout_item(size)
        }
    }

    /// End a grid layout.
    ///
    /// Calculates final grid height and advances cursor below all items.
    /// Must be paired with `begin_grid()`.
    pub fn end_grid(&mut self) {
        if let Some(layout) = self.layout_stack.pop() {
            let item_height = layout.max_item_size.y();

            // Move cursor below the grid content
            self.cursor = Vec2::new(
                layout.start_pos.x(),
                layout.cursor.y() + item_height + layout.spacing,
            );
        }
    }

    /// Grid layout with automatic column calculation based on available width.
    ///
    /// This is a convenience wrapper that creates a grid with as many columns
    /// as fit in the available width.
    ///
    /// # Arguments
    /// * `item_width` - Width of each grid item
    /// * `item_height` - Height of each grid item
    /// * `spacing` - Spacing between items
    /// * `available_width` - Total width available for the grid
    ///
    /// # Returns
    /// The number of columns that fit
    ///
    /// # Example
    /// ```ignore
    /// let cols = ui.auto_grid(64.0, 24.0, 8.0, 400.0);
    /// for item in items {
    ///     let bounds = ui.grid_item(Vec2::new(64.0, 24.0));
    ///     ui.add(Button::new(item.name).bounds(bounds));
    /// }
    /// ui.end_grid();
    /// ```
    pub(crate) fn auto_grid(
        &mut self,
        item_width: f32,
        item_height: f32,
        spacing: f32,
        available_width: f32,
    ) -> usize {
        let item_with_spacing = item_width + spacing;
        let columns = ((available_width + spacing) / item_with_spacing).max(1.0) as usize;
        self.begin_grid(columns, item_width, item_height, spacing);
        columns
    }
}
