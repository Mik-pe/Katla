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

    // Skip Modal children when modal is closed
    if let Some(modal) = node
        .widget
        .as_any()
        .downcast_ref::<super::widgets::modal::Modal>()
    {
        let is_open: bool = tree.state_arena().get(modal.open_id).unwrap_or_default();
        if !is_open {
            return None;
        }
    }

    // Walk children in reverse order (topmost first for Z-ordering)
    for &child_id in node.children.iter().rev() {
        if let Some(hit) = hit_test_recursive(tree, child_id, mouse_pos, bounds_map) {
            return Some(hit);
        }
    }

    // No child hit — check if this node itself is interactive
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
    let Some(hit) = hit_test(tree, input.mouse_pos, bounds_map) else {
        if input.mouse_clicked(mouse_button::LEFT) {
            close_outside_draggable_panels(tree, bounds_map, input.mouse_pos);
        }
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

    // Extract widget data and tree state needed for dispatch
    let (children, bounds, active_id) = {
        let Some(node) = tree.get(hit.id) else {
            return result;
        };
        (
            node.children.clone(),
            bounds_map.get(&hit.id).copied().unwrap_or_default(),
            tree.interaction().active_id,
        )
    };

    // Take actions out temporarily
    let mut actions = std::mem::take(tree.actions_mut());
    let mut state_arena = std::mem::take(tree.state_arena_mut());

    // Dispatch input to the widget via handle_input
    let mut ctx = InputContext {
        input,
        mouse_pos: input.mouse_pos,
        callbacks,
        actions: &mut actions,
        view_id: hit.id,
        active_id,
    };

    let widget_result = {
        let Some(node) = tree.get(hit.id) else {
            *tree.state_arena_mut() = state_arena;
            *tree.actions_mut() = actions;
            return result;
        };
        node.widget
            .handle_input(&mut ctx, &mut state_arena, bounds, &children)
    };

    // Extract what we need from ctx before moving actions back
    let new_active_id = ctx.active_id;

    // Put state back
    *tree.state_arena_mut() = state_arena;
    *tree.actions_mut() = actions;
    tree.interaction_mut().active_id = new_active_id;

    match widget_result {
        WidgetInputResult::Consumed => {
            result.input_consumed = true;
            result.clicked_id = Some(hit.id);
        }
        WidgetInputResult::Bubble | WidgetInputResult::Ignore => {}
    }

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
