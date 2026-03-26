//! Viewport grid state management for editor multi-viewport layouts.
//!
//! This module provides types for managing the multi-viewport grid layout
//! in the editor. The grid supports 1x1, 1x2, 2x1, and 2x2 layouts.
//!
//! # Slot Indexing
//!
//! Slots are indexed 0-3 in row-major order:
//! ```text
//! | 0 | 1 |
//! | 2 | 3 |
//! ```
//!
//! For layouts with fewer than 4 viewports, only the first N slots are active.

/// Layout configuration for the viewport grid.
///
/// Each variant defines the grid dimensions and number of viewport slots.
/// Use [`viewport_count`](Self::viewport_count) and [`grid_dimensions`](Self::grid_dimensions)
/// to query layout properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ViewportLayout {
    /// Single viewport (1x1). Default layout.
    #[default]
    Single,
    /// Two viewports side by side (1x2). Slot 0 = left, Slot 1 = right.
    Horizontal2,
    /// Two viewports stacked vertically (2x1). Slot 0 = top, Slot 1 = bottom.
    Vertical2,
    /// Four viewports in a 2x2 grid. Slots 0-3 in row-major order.
    Quad2x2,
}

impl ViewportLayout {
    /// Returns the number of viewports in this layout.
    pub fn viewport_count(&self) -> usize {
        match self {
            ViewportLayout::Single => 1,
            ViewportLayout::Horizontal2 => 2,
            ViewportLayout::Vertical2 => 2,
            ViewportLayout::Quad2x2 => 4,
        }
    }

    /// Returns the grid dimensions as (rows, cols).
    pub fn grid_dimensions(&self) -> (usize, usize) {
        match self {
            ViewportLayout::Single => (1, 1),
            ViewportLayout::Horizontal2 => (1, 2),
            ViewportLayout::Vertical2 => (2, 1),
            ViewportLayout::Quad2x2 => (2, 2),
        }
    }

    /// Returns the slot index for a given row and column.
    /// Returns None if the position is outside the grid.
    pub fn slot_index(&self, row: usize, col: usize) -> Option<usize> {
        let (rows, cols) = self.grid_dimensions();
        if row < rows && col < cols {
            Some(row * cols + col)
        } else {
            None
        }
    }

    /// Returns the (row, col) for a given slot index.
    /// Returns None if the index is out of range.
    pub fn slot_position(&self, index: usize) -> Option<(usize, usize)> {
        if index < self.viewport_count() {
            let cols = self.grid_dimensions().1;
            Some((index / cols, index % cols))
        } else {
            None
        }
    }
}

/// State for the viewport grid in the editor.
///
/// Tracks the current layout and which viewport handles are assigned to each slot.
/// This type is typically stored as a resource in the ECS world.
///
/// # Slot Assignment
///
/// Viewport handles are stored by index (0-3). The handles reference viewports
/// managed by [`ViewportManager`] in katla_gfx. When the layout changes,
/// viewport assignments persist (cameras don't reset).
///
/// # Example
///
/// ```ignore
/// let mut state = ViewportGridState::with_layout(ViewportLayout::Quad2x2);
///
/// // Assign viewports to slots
/// state.set_viewport_at(0, Some(viewport_handle_0));
/// state.set_viewport_at(1, Some(viewport_handle_1));
///
/// // Change layout - assignments persist
/// state.set_layout(ViewportLayout::Horizontal2);
/// assert_eq!(state.get_viewport_at(0), Some(viewport_handle_0));
/// ```
#[derive(Debug, Clone, Default)]
pub struct ViewportGridState {
    /// Current layout configuration.
    pub layout: ViewportLayout,
    /// Viewport handle for each slot (None if not assigned).
    /// Index 0 = top-left, 1 = top-right, 2 = bottom-left, 3 = bottom-right.
    pub viewport_slots: [Option<u32>; 4],
    /// Index of the currently active (hovered/focused) viewport.
    /// Only indices 0 to `layout.viewport_count() - 1` are valid.
    pub active_viewport: Option<usize>,
}

impl ViewportGridState {
    /// Creates a new ViewportGridState with the default (Single) layout.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new ViewportGridState with a specific layout.
    pub fn with_layout(layout: ViewportLayout) -> Self {
        Self {
            layout,
            viewport_slots: [None; 4],
            active_viewport: None,
        }
    }

    /// Sets the layout configuration.
    ///
    /// Viewport slot assignments persist across layout changes. This allows
    /// cameras to maintain their state when switching between layouts.
    pub fn set_layout(&mut self, layout: ViewportLayout) {
        self.layout = layout;
    }

    /// Gets the viewport handle at a specific slot index.
    ///
    /// Returns `None` if the index is out of range (>= 4) or no viewport is assigned.
    pub fn get_viewport_at(&self, index: usize) -> Option<u32> {
        if index < 4 {
            self.viewport_slots[index]
        } else {
            None
        }
    }

    /// Sets the viewport handle at a specific slot index.
    ///
    /// Silently ignores indices >= 4 (out of range).
    pub fn set_viewport_at(&mut self, index: usize, handle: Option<u32>) {
        if index < 4 {
            self.viewport_slots[index] = handle;
        }
    }

    /// Gets the viewport handle at a specific grid position (row, col).
    ///
    /// Returns `None` if the position is outside the current layout's grid.
    pub fn get_viewport_at_position(&self, row: usize, col: usize) -> Option<u32> {
        let index = self.layout.slot_index(row, col)?;
        self.viewport_slots.get(index).copied().flatten()
    }

    /// Returns an iterator over active viewport slots and their handles.
    ///
    /// Only yields slots that are visible in the current layout and have
    /// an assigned viewport handle.
    pub fn active_slots(&self) -> impl Iterator<Item = (usize, u32)> + '_ {
        self.viewport_slots
            .iter()
            .enumerate()
            .take(self.layout.viewport_count())
            .filter_map(|(i, &handle)| handle.map(|h| (i, h)))
    }

    /// Sets the active viewport by index.
    ///
    /// The index is validated against the current layout's viewport count.
    /// Passing an invalid index or `None` clears the active viewport.
    pub fn set_active(&mut self, index: Option<usize>) {
        // Only allow setting active if the index is valid for current layout
        self.active_viewport = index.filter(|&i| i < self.layout.viewport_count());
    }

    /// Checks if a slot index is active (visible in current layout).
    ///
    /// For example, in `Single` layout only slot 0 is active, while in
    /// `Quad2x2` all slots 0-3 are active.
    pub fn is_slot_active(&self, index: usize) -> bool {
        index < self.layout.viewport_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_viewport_count() {
        assert_eq!(ViewportLayout::Single.viewport_count(), 1);
        assert_eq!(ViewportLayout::Horizontal2.viewport_count(), 2);
        assert_eq!(ViewportLayout::Vertical2.viewport_count(), 2);
        assert_eq!(ViewportLayout::Quad2x2.viewport_count(), 4);
    }

    #[test]
    fn test_layout_grid_dimensions() {
        assert_eq!(ViewportLayout::Single.grid_dimensions(), (1, 1));
        assert_eq!(ViewportLayout::Horizontal2.grid_dimensions(), (1, 2));
        assert_eq!(ViewportLayout::Vertical2.grid_dimensions(), (2, 1));
        assert_eq!(ViewportLayout::Quad2x2.grid_dimensions(), (2, 2));
    }

    #[test]
    fn test_layout_slot_index() {
        let layout = ViewportLayout::Quad2x2;

        assert_eq!(layout.slot_index(0, 0), Some(0)); // top-left
        assert_eq!(layout.slot_index(0, 1), Some(1)); // top-right
        assert_eq!(layout.slot_index(1, 0), Some(2)); // bottom-left
        assert_eq!(layout.slot_index(1, 1), Some(3)); // bottom-right
        assert_eq!(layout.slot_index(2, 0), None); // out of bounds
    }

    #[test]
    fn test_layout_slot_position() {
        let layout = ViewportLayout::Quad2x2;

        assert_eq!(layout.slot_position(0), Some((0, 0)));
        assert_eq!(layout.slot_position(1), Some((0, 1)));
        assert_eq!(layout.slot_position(2), Some((1, 0)));
        assert_eq!(layout.slot_position(3), Some((1, 1)));
        assert_eq!(layout.slot_position(4), None);
    }

    #[test]
    fn test_grid_state_set_layout() {
        let mut state = ViewportGridState::new();

        state.set_layout(ViewportLayout::Quad2x2);
        assert_eq!(state.layout, ViewportLayout::Quad2x2);
        assert_eq!(state.layout.viewport_count(), 4);
    }

    #[test]
    fn test_grid_state_viewport_slots() {
        let mut state = ViewportGridState::new();

        state.set_viewport_at(0, Some(42));
        state.set_viewport_at(1, Some(100));

        assert_eq!(state.get_viewport_at(0), Some(42));
        assert_eq!(state.get_viewport_at(1), Some(100));
        assert_eq!(state.get_viewport_at(2), None);
    }

    #[test]
    fn test_grid_state_active_slots() {
        let mut state = ViewportGridState::with_layout(ViewportLayout::Horizontal2);

        state.set_viewport_at(0, Some(10));
        state.set_viewport_at(1, Some(20));
        state.set_viewport_at(2, Some(30)); // Not active in Horizontal2

        let active: Vec<_> = state.active_slots().collect();
        assert_eq!(active, vec![(0, 10), (1, 20)]);
    }

    #[test]
    fn test_grid_state_active_viewport() {
        let mut state = ViewportGridState::with_layout(ViewportLayout::Quad2x2);

        // Set active to valid index
        state.set_active(Some(2));
        assert_eq!(state.active_viewport, Some(2));

        // Set active to invalid index (should be None)
        state.set_active(Some(5));
        assert_eq!(state.active_viewport, None);

        // Set active to None
        state.set_active(None);
        assert_eq!(state.active_viewport, None);
    }

    #[test]
    fn test_layout_persists_across_changes() {
        let mut state = ViewportGridState::new();

        // Set up viewports in Single mode
        state.set_viewport_at(0, Some(1));

        // Change to Quad mode - slot 0 should still have viewport 1
        state.set_layout(ViewportLayout::Quad2x2);
        assert_eq!(state.get_viewport_at(0), Some(1));

        // Back to Single - slot 0 still has viewport 1
        state.set_layout(ViewportLayout::Single);
        assert_eq!(state.get_viewport_at(0), Some(1));
    }
}
