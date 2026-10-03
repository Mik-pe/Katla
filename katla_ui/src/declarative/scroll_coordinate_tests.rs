//! Resolved geometry, native UI quads and clipped input share nested scroll coordinates.

use super::*;
use crate::declarative::{WidgetBox, button, scroll, vstack};
use crate::input::{UiInputState, mouse_button};
use katla_math::Color;

#[test]
fn test_nested_scroll_draw_bounds_clicks_and_clipping_agree() {
    for (outer, inner) in [(0f32, 0f32), (30., 0.), (0., 20.), (30., 20.)] {
        let mut tree = ViewTree::new();
        let outer_id = tree
            .state_arena_mut()
            .get_or_create(ViewId::default(), outer);
        let inner_id = tree
            .state_arena_mut()
            .get_or_create(ViewId::default(), inner);
        let mut callbacks = CallbackTable::new();
        let callback = callbacks.push(|actions| actions.emit(7u32));
        tree.set_root(
            scroll(
                scroll(
                    vstack([
                        button("Visible")
                            .fill(Color::RED)
                            .on_click(callback)
                            .boxed(),
                        button("Clipped").boxed(),
                    ])
                    .boxed(),
                    inner_id,
                )
                .boxed(),
                outer_id,
            )
            .boxed(),
        );
        let root = tree.root().unwrap();
        let nested = tree.nodes[root].children[0];
        let stack = tree.nodes[nested].children[0];
        let visible = tree.nodes[stack].children[0];
        let clipped = tree.nodes[stack].children[1];
        for (id, y, height) in [
            (root, 0., 200.),
            (nested, 40., 100.),
            (stack, 40., 200.),
            (visible, 70., 20.),
            (clipped, 160., 20.),
        ] {
            tree.nodes[id].bounds =
                Rect2D::from_origin_size(Vec2::new(0., y), Vec2::new(100., height));
        }
        tree.resolve_positions();
        let expected = 70. - outer - inner;
        assert_eq!(tree.resolved_bounds[&visible].min.y(), expected);
        let mut ui = UiContext::new();
        ui.begin(Vec2::new(200., 200.), 1.);
        tree.draw_recursive(root, &mut ui);
        let draw = ui.end();
        let top = draw
            .vertices()
            .iter()
            .filter(|vertex| {
                vertex.color[0] == 255
                    && vertex.color[1] == 0
                    && vertex.color[2] == 0
                    && vertex.color[3] == 255
            })
            .map(|vertex| vertex.pos.y())
            .reduce(f32::min)
            .unwrap();
        assert_eq!(top, expected);
        let mut input = UiInputState::new();
        input.set_mouse_pos(tree.resolved_bounds[&visible].center());
        input.set_mouse_button(mouse_button::LEFT, true);
        let bounds = tree.resolved_bounds.clone();
        assert!(input::process_input(&mut tree, &input, &mut callbacks, &bounds).input_consumed);
        assert_eq!(tree.actions_mut().drain::<u32>(), vec![7]);
        input.clear_frame_state();
        input.set_mouse_pos(tree.resolved_bounds[&clipped].center());
        input.set_mouse_button(mouse_button::LEFT, false);
        input.clear_frame_state();
        input.set_mouse_button(mouse_button::LEFT, true);
        let result = input::process_input(&mut tree, &input, &mut callbacks, &bounds);
        assert_ne!(result.clicked_id, Some(clipped));
        assert!(tree.actions_mut().drain::<u32>().is_empty());
    }
}
