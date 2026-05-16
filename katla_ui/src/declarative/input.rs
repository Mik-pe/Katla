use std::collections::HashMap;

use katla_math::Rect2D;

use crate::input::{KeyCode, mouse_button};

use super::build::CallbackTable;
use super::descriptor::{Callback, ViewDescriptor};
use super::state::ViewId;
use super::tree::ViewTree;

/// Result of hit testing against the declarative tree.
#[derive(Clone, Copy, Debug)]
pub(crate) struct HitResult {
    pub id: ViewId,
}

/// Walk the view tree in reverse Z-order (last child first) to find the
/// deepest interactive node whose bounds contain `mouse_pos`.
pub(crate) fn hit_test(
    tree: &ViewTree,
    mouse_pos: katla_math::Vec2,
    bounds_map: &HashMap<ViewId, Rect2D>,
) -> Option<HitResult> {
    let root_id = tree.root()?;
    hit_test_recursive(tree, root_id, mouse_pos, bounds_map)
}

fn hit_test_recursive(
    tree: &ViewTree,
    node_id: ViewId,
    mouse_pos: katla_math::Vec2,
    bounds_map: &HashMap<ViewId, Rect2D>,
) -> Option<HitResult> {
    let node = tree.get(node_id)?;
    let bounds = bounds_map.get(&node_id)?;

    if !bounds.contains(mouse_pos) {
        return None;
    }

    // Walk children in reverse order (topmost first for Z-ordering)
    for &child_id in node.children.iter().rev() {
        if let Some(hit) = hit_test_recursive(tree, child_id, mouse_pos, bounds_map) {
            return Some(hit);
        }
    }

    // No child hit — check if this node itself is interactive
    let is_interactive = is_interactive(&node.descriptor);
    if is_interactive {
        Some(HitResult { id: node_id })
    } else {
        None
    }
}

fn is_interactive(descriptor: &ViewDescriptor) -> bool {
    match descriptor {
        ViewDescriptor::Button { .. }
        | ViewDescriptor::Toggle { .. }
        | ViewDescriptor::TextField { .. }
        | ViewDescriptor::Slider { .. }
        | ViewDescriptor::ColorPicker { .. } => true,

        ViewDescriptor::Empty
        | ViewDescriptor::Text { .. }
        | ViewDescriptor::Progress { .. }
        | ViewDescriptor::Image { .. }
        | ViewDescriptor::HStack(_)
        | ViewDescriptor::VStack(_)
        | ViewDescriptor::ZStack(_)
        | ViewDescriptor::ScrollView(_)
        | ViewDescriptor::Panel(_)
        | ViewDescriptor::Overlay(_)
        | ViewDescriptor::Custom(_) => false,
    }
}

fn get_callback(descriptor: &ViewDescriptor) -> Option<&Callback> {
    match descriptor {
        ViewDescriptor::Button { on_click, .. } => on_click.as_ref(),
        ViewDescriptor::TextField { on_submit, .. } => on_submit.as_ref(),
        _ => None,
    }
}

/// Process input events against the declarative tree.
///
/// Handles mouse clicks (dispatching callbacks), hover tracking, slider drag,
/// toggle state changes, text field focus/input, color picker interaction,
/// and scroll view scroll offsets.
pub(crate) fn process_input(
    tree: &mut ViewTree,
    input: &crate::input::UiInputState,
    callbacks: &mut CallbackTable,
    bounds_map: &HashMap<ViewId, Rect2D>,
) -> InputResult {
    let mut result = InputResult::default();

    // --- Slider drag continuation ---
    // If we have an active slider, continue dragging even if mouse left bounds.
    // Extract data from the active node before mutating the tree.
    let active_slider_info: Option<(super::state::StateId, f32, f32)> =
        if let Some(active_id) = tree.interaction().active_id {
            tree.get(active_id).and_then(|node| {
                if let ViewDescriptor::Slider {
                    value_id, range, ..
                } = &node.descriptor
                {
                    Some((*value_id, *range.start(), *range.end()))
                } else {
                    None
                }
            })
        } else {
            None
        };

    if let Some((value_id, range_start, range_end)) = active_slider_info {
        let active_id = tree.interaction().active_id.unwrap();
        if input.mouse_down[mouse_button::LEFT] {
            if let Some(bounds) = bounds_map.get(&active_id) {
                let t = ((input.mouse_pos.x() - bounds.min.x()) / bounds.width()).clamp(0.0, 1.0);
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

    // --- Hit test for new interactions ---
    let Some(hit) = hit_test(tree, input.mouse_pos, bounds_map) else {
        // No hover target — but keep active drag alive
        return result;
    };

    result.hovered_id = Some(hit.id);

    let Some(node) = tree.get(hit.id) else {
        return result;
    };
    let descriptor = node.descriptor.clone();

    match &descriptor {
        ViewDescriptor::Button { .. } if input.mouse_clicked(mouse_button::LEFT) => {
            if let Some(callback) = get_callback(&descriptor).cloned() {
                callbacks.invoke(&callback);
            }
            result.input_consumed = true;
            result.clicked_id = Some(hit.id);
        }

        ViewDescriptor::Slider {
            value_id, range, ..
        } if tree.interaction().active_id.is_none() && input.mouse_pressed[mouse_button::LEFT] => {
            tree.interaction_mut().active_id = Some(hit.id);
            if let Some(bounds) = bounds_map.get(&hit.id) {
                let t = ((input.mouse_pos.x() - bounds.min.x()) / bounds.width()).clamp(0.0, 1.0);
                let new_val = *range.start() + t * (*range.end() - *range.start());
                tree.state_arena_mut().set(*value_id, new_val);
            }
            result.input_consumed = true;
        }

        ViewDescriptor::Toggle { value_id, .. } if input.mouse_clicked(mouse_button::LEFT) => {
            let current = tree.state_arena().get::<bool>(*value_id);
            tree.state_arena_mut().set(*value_id, !current);
            result.input_consumed = true;
            result.clicked_id = Some(hit.id);
        }

        ViewDescriptor::TextField {
            value_id,
            on_submit,
            ..
        } => {
            // Click to focus
            if input.mouse_pressed[mouse_button::LEFT] {
                tree.interaction_mut().focused_id = Some(hit.id);
                tree.interaction_mut().active_id = None;
                result.input_consumed = true;
            }

            // Keyboard input when focused
            let is_focused = tree.interaction().focused_id == Some(hit.id);
            if is_focused {
                handle_text_field_input(tree, *value_id, on_submit.clone(), callbacks, input);
                result.input_consumed = true;
            }
        }

        ViewDescriptor::ColorPicker { value_id, .. } if input.mouse_clicked(mouse_button::LEFT) => {
            // Toggle picker open state
            let current = tree.state_arena().get::<bool>(*value_id);
            tree.state_arena_mut().set(*value_id, !current);
            result.input_consumed = true;
            result.clicked_id = Some(hit.id);
        }

        _ => {}
    }

    // Clear active if mouse released and not a slider
    if !input.mouse_down[mouse_button::LEFT]
        && let Some(active) = tree.interaction().active_id
        && let Some(node) = tree.get(active)
        && !matches!(node.descriptor, ViewDescriptor::Slider { .. })
    {
        tree.interaction_mut().active_id = None;
    }

    result
}

/// Handle keyboard input for a focused TextField.
fn handle_text_field_input(
    tree: &mut ViewTree,
    value_id: super::state::StateId,
    on_submit: Option<Callback>,
    callbacks: &mut CallbackTable,
    input: &crate::input::UiInputState,
) {
    let mut text: String = tree.state_arena().get::<String>(value_id);
    let mut changed = false;

    // Backspace
    if input.key_pressed(KeyCode::Backspace) && !text.is_empty() {
        let prev = text[..]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0);
        text.truncate(prev);
        changed = true;
    }

    // Delete
    if input.key_pressed(KeyCode::Delete) {
        // Simplified: no cursor tracking, just truncate for now
        changed = true;
    }

    // Enter
    if input.key_pressed(KeyCode::Enter)
        && let Some(callback) = on_submit
    {
        callbacks.invoke(&callback);
    }

    // Escape — clear focus
    if input.key_pressed(KeyCode::Escape) {
        tree.interaction_mut().focused_id = None;
    }

    // Character input
    for &c in &input.characters {
        if c >= ' ' {
            text.push(c);
            changed = true;
        }
    }

    if changed {
        tree.state_arena_mut().set(value_id, text);
    }
}

/// Result of processing input against the declarative tree.
#[derive(Default)]
pub(crate) struct InputResult {
    /// Whether a declarative node consumed the input.
    pub input_consumed: bool,
    /// The node that was hovered this frame.
    pub hovered_id: Option<ViewId>,
    /// The node that was clicked this frame.
    pub clicked_id: Option<ViewId>,
}
