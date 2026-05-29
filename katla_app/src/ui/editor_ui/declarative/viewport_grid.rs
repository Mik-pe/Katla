use katla_math::{Color, Rect2D, Vec2};
use katla_ui::declarative::{Alignment, Build, BuildContext, ViewDescriptor};
use katla_ui::{FontSize, TextureId};

use crate::resources::viewport_state::{ViewportGridState, ViewportLayout};
use crate::ui::editor_ui::ColorScheme;

/// Environment data injected before each frame for the viewport grid.
#[derive(Clone)]
pub(crate) struct ViewportGridDrawCtx {
    pub bounds: Rect2D,
    pub state: ViewportGridState,
    pub texture_ids: [Option<TextureId>; 4],
    pub theme: ColorScheme,
    pub mouse_pos: Vec2,
}

pub(crate) struct ViewportGridView;

impl Build for ViewportGridView {
    fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor {
        use katla_ui::declarative::{grid, image, text, zstack};

        let draw_ctx = ctx.env::<ViewportGridDrawCtx>().cloned();
        let Some(draw_ctx) = draw_ctx else {
            return ViewDescriptor::Empty;
        };

        let (rows, cols) = draw_ctx.state.layout.grid_dimensions();
        let cell_width = draw_ctx.bounds.width() / cols as f32;
        let cell_height = draw_ctx.bounds.height() / rows as f32;
        let cell_size = Vec2::new(cell_width, cell_height);

        let hovered_slot =
            get_slot_at_position(draw_ctx.bounds, &draw_ctx.state, draw_ctx.mouse_pos);

        let mut grid_children = Vec::new();

        for row in 0..rows {
            for col in 0..cols {
                let slot_index = draw_ctx.state.layout.slot_index(row, col).unwrap();
                let is_active = draw_ctx.state.active_viewport == Some(slot_index);
                let is_hovered = hovered_slot == Some(slot_index);

                let texture = draw_ctx.texture_ids[slot_index];

                let label = match draw_ctx.state.layout {
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

                let mut cell_content = Vec::new();

                if let Some(texture) = texture {
                    cell_content.push((Alignment::Center, image(texture, Color::WHITE)));
                }

                let label_text = text(label)
                    .color(Color::WHITE.with_alpha(0.8))
                    .font_size(FontSize::Small);

                cell_content.push((Alignment::Leading, label_text));

                let content = zstack(cell_content);

                // Use selectable for visual feedback (hover/active highlighting)
                // The actual selection is handled in layout.rs via update_active_viewport
                grid_children.push(katla_ui::declarative::selectable(content).selected(is_active));
            }
        }

        grid(cols, cell_size, grid_children).grid_spacing(0.0)
    }
}

fn get_viewport_bounds(
    bounds: Rect2D,
    state: &ViewportGridState,
    row: usize,
    col: usize,
) -> Rect2D {
    let (rows, cols) = state.layout.grid_dimensions();
    let cell_width = bounds.width() / cols as f32;
    let cell_height = bounds.height() / rows as f32;
    let min_x = bounds.min.x() + col as f32 * cell_width;
    let min_y = bounds.min.y() + row as f32 * cell_height;
    Rect2D::from_origin_size(Vec2::new(min_x, min_y), Vec2::new(cell_width, cell_height))
}

fn get_slot_at_position(bounds: Rect2D, state: &ViewportGridState, pos: Vec2) -> Option<usize> {
    if !bounds.contains(pos) {
        return None;
    }
    let (rows, cols) = state.layout.grid_dimensions();
    let cell_width = bounds.width() / cols as f32;
    let cell_height = bounds.height() / rows as f32;
    let col = ((pos.x() - bounds.min.x()) / cell_width).floor() as usize;
    let row = ((pos.y() - bounds.min.y()) / cell_height).floor() as usize;
    state.layout.slot_index(row, col)
}

#[cfg(test)]
mod tests {
    use super::*;
    use katla_math::Vec2;

    fn make_bounds() -> Rect2D {
        Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(800.0, 600.0))
    }

    #[test]
    fn test_viewport_bounds_single() {
        let state = ViewportGridState::with_layout(ViewportLayout::Single);
        let bounds = make_bounds();
        let vp = get_viewport_bounds(bounds, &state, 0, 0);
        assert_eq!(vp.min.x(), 0.0);
        assert_eq!(vp.min.y(), 0.0);
        assert_eq!(vp.width(), 800.0);
        assert_eq!(vp.height(), 600.0);
    }

    #[test]
    fn test_viewport_bounds_quad() {
        let state = ViewportGridState::with_layout(ViewportLayout::Quad2x2);
        let bounds = make_bounds();

        let vp00 = get_viewport_bounds(bounds, &state, 0, 0);
        assert_eq!(vp00.min.x(), 0.0);
        assert_eq!(vp00.min.y(), 0.0);
        assert_eq!(vp00.width(), 400.0);
        assert_eq!(vp00.height(), 300.0);

        let vp11 = get_viewport_bounds(bounds, &state, 1, 1);
        assert_eq!(vp11.min.x(), 400.0);
        assert_eq!(vp11.min.y(), 300.0);
        assert_eq!(vp11.width(), 400.0);
        assert_eq!(vp11.height(), 300.0);
    }

    #[test]
    fn test_get_slot_at_position_single() {
        let state = ViewportGridState::with_layout(ViewportLayout::Single);
        let bounds = make_bounds();
        assert_eq!(
            get_slot_at_position(bounds, &state, Vec2::new(100.0, 100.0)),
            Some(0)
        );
        assert_eq!(
            get_slot_at_position(bounds, &state, Vec2::new(700.0, 500.0)),
            Some(0)
        );
        assert_eq!(
            get_slot_at_position(bounds, &state, Vec2::new(900.0, 100.0)),
            None
        );
        assert_eq!(
            get_slot_at_position(bounds, &state, Vec2::new(-10.0, 100.0)),
            None
        );
    }

    #[test]
    fn test_get_slot_at_position_quad() {
        let state = ViewportGridState::with_layout(ViewportLayout::Quad2x2);
        let bounds = make_bounds();
        assert_eq!(
            get_slot_at_position(bounds, &state, Vec2::new(100.0, 100.0)),
            Some(0)
        );
        assert_eq!(
            get_slot_at_position(bounds, &state, Vec2::new(400.0, 100.0)),
            Some(1)
        );
        assert_eq!(
            get_slot_at_position(bounds, &state, Vec2::new(100.0, 300.0)),
            Some(2)
        );
        assert_eq!(
            get_slot_at_position(bounds, &state, Vec2::new(400.0, 300.0)),
            Some(3)
        );
    }

    #[test]
    fn test_get_slot_at_position_horizontal2() {
        let state = ViewportGridState::with_layout(ViewportLayout::Horizontal2);
        let bounds = make_bounds();
        assert_eq!(
            get_slot_at_position(bounds, &state, Vec2::new(100.0, 300.0)),
            Some(0)
        );
        assert_eq!(
            get_slot_at_position(bounds, &state, Vec2::new(400.0, 300.0)),
            Some(1)
        );
    }

    #[test]
    fn test_get_slot_at_position_vertical2() {
        let state = ViewportGridState::with_layout(ViewportLayout::Vertical2);
        let bounds = make_bounds();
        assert_eq!(
            get_slot_at_position(bounds, &state, Vec2::new(400.0, 100.0)),
            Some(0)
        );
        assert_eq!(
            get_slot_at_position(bounds, &state, Vec2::new(400.0, 300.0)),
            Some(1)
        );
    }
}
