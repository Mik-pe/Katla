use std::collections::HashMap;

use katla_math::{Rect2D, Vec2};

use crate::input::mouse_button;

use super::build::CallbackTable;
use super::descriptor::{DraggablePanelState, DraggablePanelVisibility};
use super::state::ViewId;
use super::tree::ViewTree;
use super::widget::InputContext;

pub use super::widget::InputResult as WidgetInputResult;

/// Result of hit testing against the declarative tree.
#[derive(Clone, Copy, Debug)]
pub(crate) struct HitResult {
    pub id: ViewId,
}

/// Walk the view tree in reverse Z-order (last child first) to find the
/// deepest interactive node whose bounds contain `mouse_pos`.
pub(crate) fn hit_test(
    tree: &ViewTree,
    mouse_pos: Vec2,
    bounds_map: &HashMap<ViewId, Rect2D>,
) -> Option<HitResult> {
    let root_id = tree.root()?;
    hit_test_recursive(tree, root_id, mouse_pos, bounds_map)
}

fn hit_test_recursive(
    tree: &ViewTree,
    node_id: ViewId,
    mouse_pos: Vec2,
    bounds_map: &HashMap<ViewId, Rect2D>,
) -> Option<HitResult> {
    let node = tree.get(node_id)?;
    let bounds = bounds_map.get(&node_id)?;

    if !bounds.contains(mouse_pos) {
        return None;
    }

    // Only descend into children if the widget is currently drawing them
    let draw_children = node.widget.should_draw_children(tree.state_arena());

    if draw_children {
        for &child_id in node.children.iter().rev() {
            if let Some(hit) = hit_test_recursive(tree, child_id, mouse_pos, bounds_map) {
                return Some(hit);
            }
        }
    }

    if node.widget.interactive() {
        Some(HitResult { id: node_id })
    } else {
        None
    }
}

/// Process input events against the declarative tree.
pub(crate) fn process_input(
    tree: &mut ViewTree,
    input: &crate::input::UiInputState,
    callbacks: &mut CallbackTable,
    bounds_map: &HashMap<ViewId, Rect2D>,
) -> ProcessInputResult {
    let mut result = ProcessInputResult::default();

    // --- Slider drag continuation ---
    if let Some(active_id) = tree.interaction().active_id {
        let active_info = tree.get(active_id).and_then(|node| {
            // Check if it's a slider-type widget by trying downcasts
            if let Some(s) = node
                .widget
                .as_any()
                .downcast_ref::<super::widgets::slider::Slider>()
            {
                Some((s.value_id, *s.range.start(), *s.range.end(), false))
            } else if let Some(s) = node
                .widget
                .as_any()
                .downcast_ref::<super::widgets::labeled_slider::LabeledSlider>()
            {
                Some((s.value_id, *s.range.start(), *s.range.end(), true))
            } else if let Some(s) = node
                .widget
                .as_any()
                .downcast_ref::<super::widgets::vec3_slider::Vec3Slider>()
            {
                let axis = tree.interaction().drag_axis.unwrap_or(0);
                Some((
                    s.value_ids[axis.min(2)],
                    *s.range.start(),
                    *s.range.end(),
                    false,
                ))
            } else {
                None
            }
        });

        if let Some((value_id, range_start, range_end, _is_labeled)) = active_info {
            if input.mouse_down[mouse_button::LEFT] {
                if let Some(bounds) = bounds_map.get(&active_id) {
                    let t =
                        ((input.mouse_pos.x() - bounds.min.x()) / bounds.width()).clamp(0.0, 1.0);
                    let new_val = range_start + t * (range_end - range_start);
                    tree.state_arena_mut().set(value_id, new_val);
                    result.input_consumed = true;
                }
            } else {
                tree.interaction_mut().active_id = None;
            }
            result.hovered_id = Some(active_id);
            return result;
        }
    }

    // --- Hit test for new interactions ---
    let hit = hit_test(tree, input.mouse_pos, bounds_map);

    // --- Keyboard activation: Enter/Space presses the focused widget ---
    // Only fires when the focused widget exposes a press action (buttons,
    // selectables); text inputs return None so those keys keep editing.
    if !input.key_pressed(crate::input::KeyCode::Tab)
        && (input.key_pressed(crate::input::KeyCode::Enter)
            || input.key_pressed(crate::input::KeyCode::Space))
        && let Some(focused_id) = tree.interaction().focused_id
        && let Some(node) = tree.get(focused_id)
        && let Some(callback) = node.widget.press_action()
    {
        let mut actions = std::mem::take(tree.actions_mut());
        callbacks.invoke(&callback, &mut actions);
        *tree.actions_mut() = actions;
        result.input_consumed = true;
        result.clicked_id = Some(focused_id);
        return result;
    }

    if hit.is_none() && input.mouse_clicked(mouse_button::LEFT) {
        close_outside_draggable_panels(tree, bounds_map, input.mouse_pos);
    }

    let Some(hit) = hit else {
        // No widget hit by normal hit-test. Still run global input pass
        // for widgets that want input outside their bounds (e.g. MenuBar).
        let mut actions = std::mem::take(tree.actions_mut());
        let mut state_arena = std::mem::take(tree.state_arena_mut());
        let new_active_id = tree.interaction().active_id;
        let focused_id = tree.interaction().focused_id;

        let global_ids: Vec<ViewId> = tree
            .iter_nodes()
            .filter(|(_, node)| node.widget.wants_global_input(&state_arena))
            .map(|(id, _)| id)
            .collect();

        for gid in global_ids {
            let (children, bounds) = {
                let Some(node) = tree.get(gid) else {
                    continue;
                };
                (
                    node.children.clone(),
                    bounds_map.get(&gid).copied().unwrap_or_default(),
                )
            };

            let mut ctx = InputContext {
                input,
                mouse_pos: input.mouse_pos,
                callbacks: &mut *callbacks,
                actions: &mut actions,
                view_id: gid,
                active_id: new_active_id,
                focused_id,
            };

            let widget_result = tree
                .get(gid)
                .map(|n| {
                    n.widget
                        .handle_input(&mut ctx, &mut state_arena, bounds, &children)
                })
                .unwrap_or(WidgetInputResult::Ignore);

            if widget_result == WidgetInputResult::Consumed {
                result.input_consumed = true;
                result.clicked_id = Some(gid);
                break;
            }
        }

        *tree.state_arena_mut() = state_arena;
        *tree.actions_mut() = actions;
        return result;
    };

    result.hovered_id = Some(hit.id);

    // Close DraggablePanels with close_on_outside_click if click landed on something else
    if input.mouse_clicked(mouse_button::LEFT) {
        let is_panel = tree
            .get(hit.id)
            .map(|n| {
                n.widget
                    .as_any()
                    .downcast_ref::<super::widgets::draggable_panel::DraggablePanel>()
                    .is_some()
            })
            .unwrap_or(false);
        if !is_panel {
            close_outside_draggable_panels(tree, bounds_map, input.mouse_pos);
        }
    }

    // Take actions and state out for the dispatch loop
    let mut actions = std::mem::take(tree.actions_mut());
    let mut state_arena = std::mem::take(tree.state_arena_mut());
    let mut new_active_id = tree.interaction().active_id;
    let focused_id = tree.interaction().focused_id;

    // Bubbling dispatch loop: start at the hit widget, propagate to parents
    // on Bubble, stop on Consumed or Ignore.
    let mut current_id = hit.id;

    loop {
        let (children, bounds, parent) = {
            let Some(node) = tree.get(current_id) else {
                break;
            };
            (
                node.children.clone(),
                bounds_map.get(&current_id).copied().unwrap_or_default(),
                node.parent,
            )
        };

        let mut ctx = InputContext {
            input,
            mouse_pos: input.mouse_pos,
            callbacks: &mut *callbacks,
            actions: &mut actions,
            view_id: current_id,
            active_id: new_active_id,
            focused_id,
        };

        let widget_result = tree
            .get(current_id)
            .map(|n| {
                n.widget
                    .handle_input(&mut ctx, &mut state_arena, bounds, &children)
            })
            .unwrap_or(WidgetInputResult::Ignore);

        new_active_id = ctx.active_id;

        match widget_result {
            WidgetInputResult::Consumed => {
                result.input_consumed = true;
                result.clicked_id = Some(current_id);
                break;
            }
            WidgetInputResult::Bubble => {
                current_id = match parent {
                    Some(id) => id,
                    None => break,
                };
            }
            WidgetInputResult::Ignore => {
                break;
            }
        }
    }

    // --- Global input pass: dispatch to widgets that want input outside bounds ---
    // This handles cases like MenuBar dropdowns that extend beyond the widget's
    // layout bounds. Only runs if the normal hit-test didn't consume input.
    if !result.input_consumed {
        let global_ids: Vec<ViewId> = tree
            .iter_nodes()
            .filter(|(_, node)| node.widget.wants_global_input(&state_arena))
            .map(|(id, _)| id)
            .collect();

        for gid in global_ids {
            let (children, bounds) = {
                let Some(node) = tree.get(gid) else {
                    continue;
                };
                (
                    node.children.clone(),
                    bounds_map.get(&gid).copied().unwrap_or_default(),
                )
            };

            let mut ctx = InputContext {
                input,
                mouse_pos: input.mouse_pos,
                callbacks: &mut *callbacks,
                actions: &mut actions,
                view_id: gid,
                active_id: new_active_id,
                focused_id,
            };

            let widget_result = tree
                .get(gid)
                .map(|n| {
                    n.widget
                        .handle_input(&mut ctx, &mut state_arena, bounds, &children)
                })
                .unwrap_or(WidgetInputResult::Ignore);

            new_active_id = ctx.active_id;

            if widget_result == WidgetInputResult::Consumed {
                result.input_consumed = true;
                result.clicked_id = Some(gid);
                break;
            }
        }
    }

    // Put state back
    *tree.state_arena_mut() = state_arena;
    *tree.actions_mut() = actions;
    tree.interaction_mut().active_id = new_active_id;

    // Clear active if mouse released and not a slider
    if !input.mouse_down[mouse_button::LEFT]
        && let Some(active) = tree.interaction().active_id
        && let Some(node) = tree.get(active)
    {
        let is_slider = node
            .widget
            .as_any()
            .downcast_ref::<super::widgets::slider::Slider>()
            .is_some()
            || node
                .widget
                .as_any()
                .downcast_ref::<super::widgets::labeled_slider::LabeledSlider>()
                .is_some()
            || node
                .widget
                .as_any()
                .downcast_ref::<super::widgets::vec3_slider::Vec3Slider>()
                .is_some();
        if !is_slider {
            tree.interaction_mut().active_id = None;
            tree.interaction_mut().drag_axis = None;
        }
    }

    result
}

/// Close any DraggablePanel with `close_on_outside_click` if the click position
/// is outside its bounds.
fn close_outside_draggable_panels(
    tree: &mut ViewTree,
    bounds_map: &HashMap<ViewId, Rect2D>,
    mouse_pos: Vec2,
) {
    let ids_to_close: Vec<super::state::StateId> = tree
        .iter_nodes()
        .filter_map(|(id, node)| {
            let dp = node
                .widget
                .as_any()
                .downcast_ref::<super::widgets::draggable_panel::DraggablePanel>()?;
            if !dp.close_on_outside_click {
                return None;
            }
            let state: DraggablePanelState =
                tree.state_arena().get(dp.state_id).unwrap_or_default();
            if !state.visibility.is_visible() {
                return None;
            }
            let bounds = bounds_map.get(&id)?;
            if bounds.contains(mouse_pos) {
                return None;
            }
            Some(dp.state_id)
        })
        .collect();

    for state_id in ids_to_close {
        let mut state: DraggablePanelState = tree.state_arena().get(state_id).unwrap_or_default();
        state.visibility = DraggablePanelVisibility::Hidden;
        tree.state_arena_mut().set(state_id, state);
    }
}

/// Result of processing input against the declarative tree.
#[derive(Default)]
pub(crate) struct ProcessInputResult {
    pub input_consumed: bool,
    pub hovered_id: Option<ViewId>,
    pub clicked_id: Option<ViewId>,
}

#[cfg(test)]
mod tests {
    use std::any::Any;
    use std::cell::Cell;
    use std::rc::Rc;

    use katla_math::{Rect2D, Vec2};
    use taffy::Style;

    use super::*;
    use crate::context::UiContext;
    use crate::declarative::animation::AnimationState;
    use crate::declarative::build::CallbackTable;
    use crate::declarative::diff::DiffAction;
    use crate::declarative::state::{StateArena, ViewId};
    use crate::declarative::tree::ViewTree;
    use crate::declarative::widget::{
        ChildWidgets, DrawInfo, InputContext, InputResult, MeasureFn, Widget, WidgetBox,
    };
    use crate::input::UiInputState;

    /// Test widget that returns a configured `InputResult` and tracks calls.
    struct StubWidget {
        result: InputResult,
        called: Rc<Cell<bool>>,
        child: Option<Box<dyn Widget>>,
    }

    impl StubWidget {
        fn new(result: InputResult) -> (Self, Rc<Cell<bool>>) {
            let called = Rc::new(Cell::new(false));
            (
                Self {
                    result,
                    called: called.clone(),
                    child: None,
                },
                called,
            )
        }

        fn with_child(mut self, child: impl Widget + 'static) -> Self {
            self.child = Some(Box::new(child));
            self
        }
    }

    impl Widget for StubWidget {
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
        fn diff_against(&self, _prev: &dyn Widget) -> DiffAction {
            DiffAction::Update
        }
        fn layout_style(&self, _measure: MeasureFn<'_>) -> Style {
            Style::default()
        }
        fn handle_input(
            &self,
            _ctx: &mut InputContext<'_>,
            _state: &mut StateArena,
            _bounds: Rect2D,
            _children: &[ViewId],
        ) -> InputResult {
            self.called.set(true);
            self.result
        }
        fn draw(
            &self,
            _ctx: &mut UiContext,
            _state: &StateArena,
            _bounds: Rect2D,
            _animation: &AnimationState,
            _children: &[ViewId],
            _info: &DrawInfo,
        ) {
        }
        fn interactive(&self) -> bool {
            true
        }
        fn take_children(&mut self) -> ChildWidgets {
            self.child
                .take()
                .map(ChildWidgets::Single)
                .unwrap_or(ChildWidgets::None)
        }
    }

    fn build_parent_child_tree(
        parent_result: InputResult,
        child_result: InputResult,
    ) -> (ViewTree, Rc<Cell<bool>>, Rc<Cell<bool>>) {
        let (child_widget, child_called) = StubWidget::new(child_result);
        let (parent_widget, parent_called) = StubWidget::new(parent_result);
        let parent_with_child = parent_widget.with_child(child_widget);

        let mut tree = ViewTree::new();
        tree.set_root(parent_with_child.boxed());

        (tree, parent_called, child_called)
    }

    fn make_bounds(tree: &ViewTree) -> HashMap<ViewId, Rect2D> {
        let root_id = tree.root().unwrap();
        let child_id = tree.get(root_id).unwrap().children[0];

        let mut bounds = HashMap::new();
        bounds.insert(
            root_id,
            Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(800.0, 600.0)),
        );
        bounds.insert(
            child_id,
            Rect2D::new(Vec2::new(100.0, 100.0), Vec2::new(300.0, 200.0)),
        );
        bounds
    }

    #[test]
    fn test_consumed_stops_propagation() {
        let (mut tree, parent_called, child_called) =
            build_parent_child_tree(InputResult::Ignore, InputResult::Consumed);
        let bounds = make_bounds(&tree);

        let mut input = UiInputState::new();
        input.mouse_pos = Vec2::new(150.0, 150.0);

        let mut callbacks = CallbackTable::new();
        let result = process_input(&mut tree, &input, &mut callbacks, &bounds);

        assert!(child_called.get(), "child handle_input should be called");
        assert!(
            !parent_called.get(),
            "parent handle_input should NOT be called when child returns Consumed"
        );
        assert!(result.input_consumed);
    }

    #[test]
    fn test_bubble_propagates_to_parent() {
        let (mut tree, parent_called, child_called) =
            build_parent_child_tree(InputResult::Consumed, InputResult::Bubble);
        let bounds = make_bounds(&tree);

        let mut input = UiInputState::new();
        input.mouse_pos = Vec2::new(150.0, 150.0);

        let mut callbacks = CallbackTable::new();
        let result = process_input(&mut tree, &input, &mut callbacks, &bounds);

        assert!(child_called.get(), "child handle_input should be called");
        assert!(
            parent_called.get(),
            "parent handle_input should be called when child returns Bubble"
        );
        assert!(result.input_consumed, "parent consumed the bubbled event");
    }

    #[test]
    fn test_ignore_does_not_propagate() {
        let (mut tree, parent_called, child_called) =
            build_parent_child_tree(InputResult::Consumed, InputResult::Ignore);
        let bounds = make_bounds(&tree);

        let mut input = UiInputState::new();
        input.mouse_pos = Vec2::new(150.0, 150.0);

        let mut callbacks = CallbackTable::new();
        let result = process_input(&mut tree, &input, &mut callbacks, &bounds);

        assert!(child_called.get(), "child handle_input should be called");
        assert!(
            !parent_called.get(),
            "parent handle_input should NOT be called when child returns Ignore"
        );
        assert!(
            !result.input_consumed,
            "Ignore should not mark the event as consumed"
        );
    }

    #[test]
    fn test_bubble_chain_propagates_through_multiple_ancestors() {
        // Grandparent (Consumed) → Parent (Bubble) → Child (Bubble)
        let (grandchild, gc_called) = StubWidget::new(InputResult::Bubble);
        let (mut parent, parent_called) = StubWidget::new(InputResult::Bubble);
        parent.child = Some(grandchild.boxed());

        let (mut grandparent, gp_called) = StubWidget::new(InputResult::Consumed);
        grandparent.child = Some(parent.boxed());

        let mut tree = ViewTree::new();
        tree.set_root(grandparent.boxed());

        let root_id = tree.root().unwrap();
        let parent_id = tree.get(root_id).unwrap().children[0];
        let child_id = tree.get(parent_id).unwrap().children[0];

        let mut bounds = HashMap::new();
        bounds.insert(
            root_id,
            Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(800.0, 600.0)),
        );
        bounds.insert(
            parent_id,
            Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(800.0, 600.0)),
        );
        bounds.insert(
            child_id,
            Rect2D::new(Vec2::new(100.0, 100.0), Vec2::new(300.0, 200.0)),
        );

        let mut input = UiInputState::new();
        input.mouse_pos = Vec2::new(150.0, 150.0);

        let mut callbacks = CallbackTable::new();
        let result = process_input(&mut tree, &input, &mut callbacks, &bounds);

        assert!(gc_called.get(), "grandchild should be called");
        assert!(parent_called.get(), "parent should receive bubbled event");
        assert!(gp_called.get(), "grandparent should receive bubbled event");
        assert!(result.input_consumed);
    }

    #[test]
    fn test_bubble_stops_at_root_with_no_parent() {
        // Parent (Bubble) → Child (Bubble)
        // Parent has no parent, so Bubble from parent should just stop
        let (child, child_called) = StubWidget::new(InputResult::Bubble);
        let (parent, parent_called) = StubWidget::new(InputResult::Bubble);
        let parent_with_child = parent.with_child(child);

        let mut tree = ViewTree::new();
        tree.set_root(parent_with_child.boxed());

        let root_id = tree.root().unwrap();
        let child_id = tree.get(root_id).unwrap().children[0];

        let mut bounds = HashMap::new();
        bounds.insert(
            root_id,
            Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(800.0, 600.0)),
        );
        bounds.insert(
            child_id,
            Rect2D::new(Vec2::new(100.0, 100.0), Vec2::new(300.0, 200.0)),
        );

        let mut input = UiInputState::new();
        input.mouse_pos = Vec2::new(150.0, 150.0);

        let mut callbacks = CallbackTable::new();
        let result = process_input(&mut tree, &input, &mut callbacks, &bounds);

        assert!(child_called.get());
        assert!(parent_called.get());
        assert!(
            !result.input_consumed,
            "Bubble to root with no parent should not consume"
        );
    }

    #[test]
    fn test_slider_drag_continuation_across_frames() {
        use super::super::constructors;
        use super::super::widget::WidgetBox;

        let mut tree = ViewTree::new();
        let arena = tree.state_arena_mut();
        let vid = ViewId::from(slotmap::KeyData::from_ffi(1));
        let value_id = arena.get_or_create(vid, 0.0f32);

        let slider = constructors::slider("vol", value_id, 0.0..=100.0);
        tree.set_root(slider.boxed());

        let root_id = tree.root().unwrap();
        let mut bounds = HashMap::new();
        bounds.insert(
            root_id,
            Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(200.0, 20.0)),
        );

        // Frame 1: Press on slider
        let mut input1 = UiInputState::new();
        input1.mouse_pos = Vec2::new(100.0, 10.0);
        input1.set_mouse_button(mouse_button::LEFT, true);
        input1.mouse_down[mouse_button::LEFT] = true;

        let mut callbacks = CallbackTable::new();
        let result1 = process_input(&mut tree, &input1, &mut callbacks, &bounds);
        assert!(result1.input_consumed);
        assert_eq!(result1.hovered_id, Some(root_id));

        // Frame 2: Drag outside bounds (mouse at x=250 which is outside the 200px slider)
        let mut input2 = UiInputState::new();
        input2.mouse_pos = Vec2::new(250.0, 10.0);
        input2.mouse_down[mouse_button::LEFT] = true;

        let result2 = process_input(&mut tree, &input2, &mut callbacks, &bounds);
        assert!(
            result2.input_consumed,
            "slider drag should continue outside bounds"
        );

        let value: f32 = tree.state_arena().get(value_id).unwrap_or_default();
        assert!(
            value > 95.0,
            "dragging beyond right edge should clamp to max, got {value}"
        );
    }
}
