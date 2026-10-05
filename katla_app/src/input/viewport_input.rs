//! Viewport input routing for multi-viewport camera control.
//!
//! This module provides input routing that determines which viewport's camera
//! should receive input based on mouse hover position.

use crate::resources::viewport_state::ViewportGridState;
use katla_math::Vec2;

/// Updates the active viewport based on mouse position.
///
/// Call this each frame before processing camera input.
/// The hovered viewport becomes the active viewport for input routing.
///
/// # Arguments
///
/// * `state` - The viewport grid state to update
/// * `mouse_pos` - Current mouse position
/// * `grid_bounds_min` - Top-left corner of the viewport grid
/// * `grid_bounds_max` - Bottom-right corner of the viewport grid
///
/// # Example
///
/// ```ignore
/// update_active_viewport(&mut state, ui.input.mouse_pos, grid_min, grid_max);
///
/// // Now camera input affects only the active viewport
/// if let Some(active_slot) = state.active_viewport {
///     // Route input to camera at active_slot
/// }
/// ```
pub fn update_active_viewport(
    state: &mut ViewportGridState,
    mouse_pos: Vec2,
    grid_bounds_min: Vec2,
    grid_bounds_max: Vec2,
) {
    let (rows, cols) = state.layout.grid_dimensions();

    // Check if mouse is within grid bounds
    if mouse_pos.x() < grid_bounds_min.x()
        || mouse_pos.x() >= grid_bounds_max.x()
        || mouse_pos.y() < grid_bounds_min.y()
        || mouse_pos.y() >= grid_bounds_max.y()
    {
        // Mouse outside grid - no active viewport
        state.active_viewport = None;
        return;
    }

    // Calculate relative position within grid
    let rel_x = mouse_pos.x() - grid_bounds_min.x();
    let rel_y = mouse_pos.y() - grid_bounds_min.y();
    let grid_width = grid_bounds_max.x() - grid_bounds_min.x();
    let grid_height = grid_bounds_max.y() - grid_bounds_min.y();

    // Calculate cell size
    let cell_width = grid_width / cols as f32;
    let cell_height = grid_height / rows as f32;

    // Calculate column and row
    let col = (rel_x / cell_width).floor() as usize;
    let row = (rel_y / cell_height).floor() as usize;

    // Clamp to valid range
    let col = col.min(cols - 1);
    let row = row.min(rows - 1);

    // Calculate slot index
    let slot_index = row * cols + col;

    // Update active viewport
    state.set_active(Some(slot_index));
}

/// Checks if input should be routed to a specific viewport slot.
///
/// Returns true if the given slot is the currently active viewport.
#[inline]
pub fn is_viewport_active(state: &ViewportGridState, slot: usize) -> bool {
    state.active_viewport == Some(slot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::viewport_state::ViewportLayout;

    fn create_state(layout: ViewportLayout) -> ViewportGridState {
        ViewportGridState::with_layout(layout)
    }

    #[test]
    fn test_update_active_viewport_single() {
        let mut state = create_state(ViewportLayout::Single);
        let min = Vec2::new(0.0, 0.0);
        let max = Vec2::new(800.0, 600.0);

        // Mouse in center
        update_active_viewport(&mut state, Vec2::new(400.0, 300.0), min, max);
        assert_eq!(state.active_viewport, Some(0));

        // Mouse near edge
        update_active_viewport(&mut state, Vec2::new(10.0, 10.0), min, max);
        assert_eq!(state.active_viewport, Some(0));

        // Mouse outside
        update_active_viewport(&mut state, Vec2::new(900.0, 300.0), min, max);
        assert_eq!(state.active_viewport, None);
    }

    #[test]
    fn test_update_active_viewport_quad() {
        let mut state = create_state(ViewportLayout::Quad2x2);
        let min = Vec2::new(0.0, 0.0);
        let max = Vec2::new(800.0, 600.0);

        // Top-left (slot 0)
        update_active_viewport(&mut state, Vec2::new(100.0, 100.0), min, max);
        assert_eq!(state.active_viewport, Some(0));

        // Top-right (slot 1)
        update_active_viewport(&mut state, Vec2::new(500.0, 100.0), min, max);
        assert_eq!(state.active_viewport, Some(1));

        // Bottom-left (slot 2)
        update_active_viewport(&mut state, Vec2::new(100.0, 400.0), min, max);
        assert_eq!(state.active_viewport, Some(2));

        // Bottom-right (slot 3)
        update_active_viewport(&mut state, Vec2::new(500.0, 400.0), min, max);
        assert_eq!(state.active_viewport, Some(3));

        // Exactly on boundary (goes to right/bottom due to >= comparison)
        update_active_viewport(&mut state, Vec2::new(400.0, 300.0), min, max);
        assert_eq!(state.active_viewport, Some(3));
    }

    #[test]
    fn test_update_active_viewport_horizontal2() {
        let mut state = create_state(ViewportLayout::Horizontal2);
        let min = Vec2::new(0.0, 0.0);
        let max = Vec2::new(800.0, 600.0);

        // Left half (slot 0)
        update_active_viewport(&mut state, Vec2::new(100.0, 300.0), min, max);
        assert_eq!(state.active_viewport, Some(0));

        // Right half (slot 1)
        update_active_viewport(&mut state, Vec2::new(600.0, 300.0), min, max);
        assert_eq!(state.active_viewport, Some(1));
    }

    #[test]
    fn test_update_active_viewport_vertical2() {
        let mut state = create_state(ViewportLayout::Vertical2);
        let min = Vec2::new(0.0, 0.0);
        let max = Vec2::new(800.0, 600.0);

        // Top half (slot 0)
        update_active_viewport(&mut state, Vec2::new(400.0, 100.0), min, max);
        assert_eq!(state.active_viewport, Some(0));

        // Bottom half (slot 1)
        update_active_viewport(&mut state, Vec2::new(400.0, 500.0), min, max);
        assert_eq!(state.active_viewport, Some(1));
    }

    #[test]
    fn test_is_viewport_active() {
        let mut state = create_state(ViewportLayout::Quad2x2);
        let min = Vec2::new(0.0, 0.0);
        let max = Vec2::new(800.0, 600.0);

        update_active_viewport(&mut state, Vec2::new(500.0, 100.0), min, max);

        assert!(!is_viewport_active(&state, 0));
        assert!(is_viewport_active(&state, 1));
        assert!(!is_viewport_active(&state, 2));
        assert!(!is_viewport_active(&state, 3));
    }

    #[test]
    fn test_active_clears_on_mouse_out() {
        let mut state = create_state(ViewportLayout::Single);
        let min = Vec2::new(0.0, 0.0);
        let max = Vec2::new(800.0, 600.0);

        // Set active
        update_active_viewport(&mut state, Vec2::new(400.0, 300.0), min, max);
        assert_eq!(state.active_viewport, Some(0));

        // Mouse leaves
        update_active_viewport(&mut state, Vec2::new(-10.0, 300.0), min, max);
        assert_eq!(state.active_viewport, None);

        // Mouse returns
        update_active_viewport(&mut state, Vec2::new(400.0, 300.0), min, max);
        assert_eq!(state.active_viewport, Some(0));
    }
}
