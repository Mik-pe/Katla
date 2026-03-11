//! Viewport grid widget for multi-viewport layout.
//!
//! This module provides a UI widget that displays multiple viewports
//! in a configurable grid layout (1x1, 1x2, 2x1, or 2x2).

use crate::resources::viewport_state::{ViewportGridState, ViewportLayout};
use katla_math::{Color, Rect2D, Vec2};
use katla_ui::{mouse_button, Response, TextureId, UiContext, Widget};

use super::{FocusedPanel, Theme};

/// Widget that displays multiple viewports in a grid layout.
pub struct ViewportGrid<'a> {
    /// The bounds of the entire grid.
    pub bounds: Rect2D,
    /// The grid state (layout, active viewport, etc.).
    pub state: &'a ViewportGridState,
    /// Texture IDs for each viewport slot (indexed 0-3).
    pub texture_ids: &'a [Option<TextureId>; 4],
    /// The UI theme.
    pub theme: &'a Theme,
    /// The currently focused panel.
    pub focused_panel: &'a mut FocusedPanel,
}

impl<'a> ViewportGrid<'a> {
    /// Creates a new viewport grid widget.
    pub fn new(
        bounds: Rect2D,
        state: &'a ViewportGridState,
        texture_ids: &'a [Option<TextureId>; 4],
        theme: &'a Theme,
        focused_panel: &'a mut FocusedPanel,
    ) -> Self {
        Self {
            bounds,
            state,
            texture_ids,
            theme,
            focused_panel,
        }
    }

    /// Returns the bounds for a viewport at the given grid position.
    fn get_viewport_bounds(&self, row: usize, col: usize) -> Rect2D {
        let (rows, cols) = self.state.layout.grid_dimensions();

        let cell_width = self.bounds.width() / cols as f32;
        let cell_height = self.bounds.height() / rows as f32;

        let min_x = self.bounds.min.x() + col as f32 * cell_width;
        let min_y = self.bounds.min.y() + row as f32 * cell_height;

        Rect2D::from_origin_size(Vec2::new(min_x, min_y), Vec2::new(cell_width, cell_height))
    }

    /// Returns the viewport slot index at the given position, if any.
    pub fn get_slot_at_position(&self, pos: Vec2) -> Option<usize> {
        if !self.bounds.contains(pos) {
            return None;
        }

        let (rows, cols) = self.state.layout.grid_dimensions();
        let cell_width = self.bounds.width() / cols as f32;
        let cell_height = self.bounds.height() / rows as f32;

        let col = ((pos.x() - self.bounds.min.x()) / cell_width).floor() as usize;
        let row = ((pos.y() - self.bounds.min.y()) / cell_height).floor() as usize;

        self.state.layout.slot_index(row, col)
    }
}

impl<'a> Widget for ViewportGrid<'a> {
    fn ui(self, ui: &mut UiContext) -> Response {
        let (rows, cols) = self.state.layout.grid_dimensions();

        // Check which viewport is hovered
        let hovered_slot = self.get_slot_at_position(ui.mouse_pos());

        // Draw each viewport in the grid
        for row in 0..rows {
            for col in 0..cols {
                let slot_index = self.state.layout.slot_index(row, col).unwrap();
                let viewport_bounds = self.get_viewport_bounds(row, col);

                // Get the texture handle for this slot
                let texture = self.texture_ids[slot_index].unwrap_or(TextureId::NONE);

                ui.draw_image(
                    viewport_bounds,
                    Vec2::new(0.0, 0.0),
                    Vec2::new(1.0, 1.0),
                    Color::WHITE,
                    texture,
                );

                // Draw border (highlight if hovered/active)
                let border_width = 2.0;
                let is_active = self.state.active_viewport == Some(slot_index);
                let is_hovered = hovered_slot == Some(slot_index);
                let border_color = if is_active {
                    self.theme.selection
                } else if is_hovered {
                    self.theme.selection_hover
                } else {
                    self.theme.viewport_border
                };

                // Top border
                ui.draw_rect(
                    Rect2D::from_origin_size(
                        viewport_bounds.min,
                        Vec2::new(viewport_bounds.width(), border_width),
                    ),
                    border_color,
                );
                // Bottom border
                ui.draw_rect(
                    Rect2D::from_origin_size(
                        Vec2::new(
                            viewport_bounds.min.x(),
                            viewport_bounds.max.y() - border_width,
                        ),
                        Vec2::new(viewport_bounds.width(), border_width),
                    ),
                    border_color,
                );
                // Left border
                ui.draw_rect(
                    Rect2D::from_origin_size(
                        viewport_bounds.min,
                        Vec2::new(border_width, viewport_bounds.height()),
                    ),
                    border_color,
                );
                // Right border
                ui.draw_rect(
                    Rect2D::from_origin_size(
                        Vec2::new(
                            viewport_bounds.max.x() - border_width,
                            viewport_bounds.min.y(),
                        ),
                        Vec2::new(border_width, viewport_bounds.height()),
                    ),
                    border_color,
                );

                // Draw label for this viewport
                let label = match self.state.layout {
                    ViewportLayout::Single => "3D View",
                    ViewportLayout::Horizontal2 => {
                        if slot_index == 0 {
                            "Left"
                        } else {
                            "Right"
                        }
                    }
                    ViewportLayout::Vertical2 => {
                        if slot_index == 0 {
                            "Top"
                        } else {
                            "Bottom"
                        }
                    }
                    ViewportLayout::Quad2x2 => match slot_index {
                        0 => "Top-Left",
                        1 => "Top-Right",
                        2 => "Bottom-Left",
                        _ => "Bottom-Right",
                    },
                };
                let label_pos =
                    Vec2::new(viewport_bounds.min.x() + 8.0, viewport_bounds.min.y() + 8.0);
                ui.draw_text(
                    label,
                    label_pos,
                    Color::WHITE.with_alpha(0.7),
                    12.0, // font size
                );
            }
        }

        // Handle focus on click
        if ui.is_hovered(self.bounds) && ui.mouse_clicked(mouse_button::LEFT) {
            *self.focused_panel = FocusedPanel::Viewport;
        }

        // Return hovered response if any viewport is hovered
        if hovered_slot.is_some() {
            Response::hovered(self.bounds)
        } else {
            Response::new(self.bounds)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use katla_math::Vec2;

    fn create_test_state(layout: ViewportLayout) -> ViewportGridState {
        ViewportGridState::with_layout(layout)
    }

    fn create_test_grid<'a>(
        bounds: Rect2D,
        state: &'a ViewportGridState,
        texture_ids: &'a [Option<TextureId>; 4],
        theme: &'a Theme,
        focused: &'a mut FocusedPanel,
    ) -> ViewportGrid<'a> {
        ViewportGrid::new(bounds, state, texture_ids, theme, focused)
    }

    fn run_test<F>(layout: ViewportLayout, test_fn: F)
    where
        F: FnOnce(&mut ViewportGrid, &Theme),
    {
        let state = create_test_state(layout);
        let texture_ids = [None; 4];
        let bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(800.0, 600.0));

        let theme = Theme::default();
        let mut focused = FocusedPanel::None;
        let mut grid = ViewportGrid::new(bounds, &state, &texture_ids, &theme, &mut focused);
        test_fn(&mut grid, &theme);
    }

    #[test]
    fn test_viewport_bounds_single() {
        run_test(ViewportLayout::Single, |grid, _| {
            let vp_bounds = grid.get_viewport_bounds(0, 0);
            assert_eq!(vp_bounds.min.x(), 0.0);
            assert_eq!(vp_bounds.min.y(), 0.0);
            assert_eq!(vp_bounds.width(), 800.0);
            assert_eq!(vp_bounds.height(), 600.0);
        });
    }

    #[test]
    fn test_viewport_bounds_quad() {
        run_test(ViewportLayout::Quad2x2, |grid, _| {
            // Top-left
            let vp00 = grid.get_viewport_bounds(0, 0);
            assert_eq!(vp00.min.x(), 0.0);
            assert_eq!(vp00.min.y(), 0.0);
            assert_eq!(vp00.width(), 400.0);
            assert_eq!(vp00.height(), 300.0);

            // Bottom-right
            let vp11 = grid.get_viewport_bounds(1, 1);
            assert_eq!(vp11.min.x(), 400.0);
            assert_eq!(vp11.min.y(), 300.0);
            assert_eq!(vp11.width(), 400.0);
            assert_eq!(vp11.height(), 300.0);
        });
    }

    #[test]
    fn test_get_slot_at_position_single() {
        run_test(ViewportLayout::Single, |grid, _| {
            // Any position in bounds should return slot 0
            assert_eq!(grid.get_slot_at_position(Vec2::new(100.0, 100.0)), Some(0));
            assert_eq!(grid.get_slot_at_position(Vec2::new(700.0, 500.0)), Some(0));

            // Out of bounds should return None
            assert_eq!(grid.get_slot_at_position(Vec2::new(900.0, 100.0)), None);
            assert_eq!(grid.get_slot_at_position(Vec2::new(-10.0, 100.0)), None);
        });
    }

    #[test]
    fn test_get_slot_at_position_quad() {
        run_test(ViewportLayout::Quad2x2, |grid, _| {
            // Top-left quadrant
            assert_eq!(grid.get_slot_at_position(Vec2::new(100.0, 100.0)), Some(0));
            assert_eq!(grid.get_slot_at_position(Vec2::new(399.0, 299.0)), Some(0));

            // Top-right quadrant
            assert_eq!(grid.get_slot_at_position(Vec2::new(400.0, 100.0)), Some(1));
            assert_eq!(grid.get_slot_at_position(Vec2::new(700.0, 299.0)), Some(1));

            // Bottom-left quadrant
            assert_eq!(grid.get_slot_at_position(Vec2::new(100.0, 300.0)), Some(2));
            assert_eq!(grid.get_slot_at_position(Vec2::new(399.0, 500.0)), Some(2));

            // Bottom-right quadrant
            assert_eq!(grid.get_slot_at_position(Vec2::new(400.0, 300.0)), Some(3));
            assert_eq!(grid.get_slot_at_position(Vec2::new(700.0, 500.0)), Some(3));
        });
    }

    #[test]
    fn test_get_slot_at_position_horizontal2() {
        run_test(ViewportLayout::Horizontal2, |grid, _| {
            // Left half
            assert_eq!(grid.get_slot_at_position(Vec2::new(100.0, 300.0)), Some(0));
            assert_eq!(grid.get_slot_at_position(Vec2::new(399.0, 500.0)), Some(0));

            // Right half
            assert_eq!(grid.get_slot_at_position(Vec2::new(400.0, 300.0)), Some(1));
            assert_eq!(grid.get_slot_at_position(Vec2::new(700.0, 500.0)), Some(1));
        });
    }

    #[test]
    fn test_get_slot_at_position_vertical2() {
        run_test(ViewportLayout::Vertical2, |grid, _| {
            // Top half
            assert_eq!(grid.get_slot_at_position(Vec2::new(400.0, 100.0)), Some(0));
            assert_eq!(grid.get_slot_at_position(Vec2::new(400.0, 299.0)), Some(0));

            // Bottom half
            assert_eq!(grid.get_slot_at_position(Vec2::new(400.0, 300.0)), Some(1));
            assert_eq!(grid.get_slot_at_position(Vec2::new(400.0, 500.0)), Some(1));
        });
    }
}
