use std::collections::HashMap;

use std::collections::HashSet;

use katla_math::{Rect2D, Vec2};

use crate::input::{KeyCode, mouse_button};

use super::build::CallbackTable;
use super::descriptor::{Callback, DraggablePanelState, DraggablePanelVisibility, ViewDescriptor};
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
        | ViewDescriptor::LabeledSlider { .. }
        | ViewDescriptor::Slider { .. }
        | ViewDescriptor::Vec3Slider { .. }
        | ViewDescriptor::Toggle { .. }
        | ViewDescriptor::TextField { .. }
        | ViewDescriptor::ColorPicker { .. }
        | ViewDescriptor::ImageButton { .. }
        | ViewDescriptor::RadioButton { .. }
        | ViewDescriptor::DraggablePanel { .. }
        | ViewDescriptor::MenuBar { .. }
        | ViewDescriptor::TreeView { .. }
        | ViewDescriptor::Modal { .. }
        | ViewDescriptor::ContextMenu { .. }
        | ViewDescriptor::ScrollView(_)
        | ViewDescriptor::Selectable { .. }
        | ViewDescriptor::Section { .. }
        | ViewDescriptor::TabBar(_) => true,

        ViewDescriptor::Empty
        | ViewDescriptor::Text { .. }
        | ViewDescriptor::Progress { .. }
        | ViewDescriptor::VuMeter { .. }
        | ViewDescriptor::Image { .. }
        | ViewDescriptor::PropertyRow { .. }
        | ViewDescriptor::Separator { .. }
        | ViewDescriptor::Icon { .. }
        | ViewDescriptor::Grid(_)
        | ViewDescriptor::HStack(_)
        | ViewDescriptor::VStack(_)
        | ViewDescriptor::ZStack(_)
        | ViewDescriptor::Panel(_)
        | ViewDescriptor::Overlay(_)
        | ViewDescriptor::StatusBar(_)
        | ViewDescriptor::TransitionContainer { .. } => false,
    }
}

fn get_callback(descriptor: &ViewDescriptor) -> Option<&Callback> {
    match descriptor {
        ViewDescriptor::Button { on_click, .. } => on_click.as_ref(),
        ViewDescriptor::ImageButton { on_click, .. } => on_click.as_ref(),
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
            tree.get(active_id).and_then(|node| match &node.descriptor {
                ViewDescriptor::Slider {
                    value_id, range, ..
                }
                | ViewDescriptor::LabeledSlider {
                    value_id, range, ..
                } => Some((*value_id, *range.start(), *range.end())),
                ViewDescriptor::Vec3Slider {
                    value_ids, range, ..
                } => {
                    let axis = tree.interaction().drag_axis.unwrap_or(0);
                    let value_id = value_ids[axis.min(2)];
                    Some((value_id, *range.start(), *range.end()))
                }
                _ => None,
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
        ViewDescriptor::Button { .. } | ViewDescriptor::ImageButton { .. }
            if input.mouse_clicked(mouse_button::LEFT) =>
        {
            if let Some(callback) = get_callback(&descriptor).cloned() {
                callbacks.invoke(&callback, tree.actions_mut());
            }
            result.input_consumed = true;
            result.clicked_id = Some(hit.id);
        }

        ViewDescriptor::LabeledSlider {
            value_id, range, ..
        }
        | ViewDescriptor::Slider {
            value_id, range, ..
        } if tree.interaction().active_id.is_none() && input.mouse_pressed[mouse_button::LEFT] => {
            tree.interaction_mut().active_id = Some(hit.id);
            tree.interaction_mut().drag_axis = None;
            if let Some(bounds) = bounds_map.get(&hit.id) {
                // For LabeledSlider, the track starts after label_width
                let track_x = match &descriptor {
                    ViewDescriptor::LabeledSlider { label_width, .. } => {
                        bounds.min.x() + *label_width
                    }
                    _ => bounds.min.x(),
                };
                let track_width = bounds.max.x() - track_x;
                let t = if track_width > 0.0 {
                    ((input.mouse_pos.x() - track_x) / track_width).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let new_val = *range.start() + t * (*range.end() - *range.start());
                tree.state_arena_mut().set(*value_id, new_val);
            }
            result.input_consumed = true;
        }

        ViewDescriptor::Vec3Slider {
            value_ids, range, ..
        } if tree.interaction().active_id.is_none() && input.mouse_pressed[mouse_button::LEFT] => {
            if let Some(bounds) = bounds_map.get(&hit.id) {
                let row_height = bounds.height() / 3.0;
                let axis =
                    ((input.mouse_pos.y() - bounds.min.y()) / row_height).clamp(0.0, 2.99) as usize;
                tree.interaction_mut().active_id = Some(hit.id);
                tree.interaction_mut().drag_axis = Some(axis);
                let axis_label_width = 20.0;
                let track_x = bounds.min.x() + axis_label_width;
                let track_width = bounds.max.x() - track_x - 40.0;
                let t = if track_width > 0.0 {
                    ((input.mouse_pos.x() - track_x) / track_width).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let new_val = *range.start() + t * (*range.end() - *range.start());
                tree.state_arena_mut().set(value_ids[axis], new_val);
            }
            result.input_consumed = true;
        }

        ViewDescriptor::Slider { .. }
        | ViewDescriptor::LabeledSlider { .. }
        | ViewDescriptor::Vec3Slider { .. } => {}

        ViewDescriptor::RadioButton {
            value_id, index, ..
        } if input.mouse_clicked(mouse_button::LEFT) => {
            tree.state_arena_mut().set(*value_id, *index);
            result.input_consumed = true;
            result.clicked_id = Some(hit.id);
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
            let current = tree.state_arena().get::<bool>(*value_id);
            tree.state_arena_mut().set(*value_id, !current);
            result.input_consumed = true;
            result.clicked_id = Some(hit.id);
        }

        ViewDescriptor::DraggablePanel(desc) => {
            let Some(node_bounds) = bounds_map.get(&hit.id).copied() else {
                return result;
            };
            let mut state: DraggablePanelState = tree.state_arena().get(desc.state_id);

            if !state.visibility.is_visible() {
                return result;
            }

            let title_bar_height = 25.0_f32;
            let close_size = 24.0;
            let close_bounds = Rect2D::from_origin_size(
                Vec2::new(
                    node_bounds.max.x() - close_size - 6.0,
                    node_bounds.min.y() + 4.0,
                ),
                Vec2::new(close_size, close_size),
            );

            if close_bounds.contains(input.mouse_pos) && input.mouse_clicked(mouse_button::LEFT) {
                state.visibility = DraggablePanelVisibility::Hidden;
                tree.state_arena_mut().set(desc.state_id, state);
                result.input_consumed = true;
                return result;
            }

            let title_bounds = Rect2D::new(
                node_bounds.min,
                Vec2::new(node_bounds.max.x(), node_bounds.min.y() + title_bar_height),
            );
            let in_title = title_bounds.contains(input.mouse_pos);
            let in_close = close_bounds.contains(input.mouse_pos);

            if state.dragging {
                if input.mouse_down[mouse_button::LEFT] {
                    let new_pos = input.mouse_pos - state.drag_offset;
                    state.position = Some(new_pos);
                    tree.state_arena_mut().set(desc.state_id, state);
                } else {
                    state.dragging = false;
                    tree.state_arena_mut().set(desc.state_id, state);
                }
                result.input_consumed = true;
                return result;
            }

            if in_title && !in_close && input.mouse_pressed[mouse_button::LEFT] {
                state.dragging = true;
                state.drag_offset = input.mouse_pos - node_bounds.min;
                tree.state_arena_mut().set(desc.state_id, state);
                result.input_consumed = true;
            }
        }

        ViewDescriptor::MenuBar(desc) => {
            let Some(node_bounds) = bounds_map.get(&hit.id).copied() else {
                return result;
            };
            let font_size = 14.0_f32;
            let item_spacing = 8.0_f32;
            let mut x = node_bounds.min.x() + item_spacing;
            let bar_height = desc.height;

            for group in &desc.groups {
                let label_size = measure_menu_label(&group.label, font_size);
                let group_bounds = Rect2D::from_origin_size(
                    Vec2::new(x, node_bounds.min.y()),
                    Vec2::new(label_size + item_spacing * 2.0, bar_height),
                );

                if group_bounds.contains(input.mouse_pos) && input.mouse_clicked(mouse_button::LEFT)
                {
                    let is_open: bool = tree.state_arena().get(group.open_id);
                    tree.state_arena_mut().set(group.open_id, !is_open);
                    result.input_consumed = true;
                    break;
                }

                let is_open: bool = tree.state_arena().get(group.open_id);
                if is_open {
                    let dropdown_y = group_bounds.max.y();
                    let dropdown_width = 180.0_f32;
                    let dropdown_bounds = Rect2D::from_origin_size(
                        Vec2::new(group_bounds.min.x(), dropdown_y),
                        Vec2::new(dropdown_width, group.items.len() as f32 * 28.0),
                    );

                    if dropdown_bounds.contains(input.mouse_pos)
                        && input.mouse_clicked(mouse_button::LEFT)
                    {
                        for (ii, entry) in group.items.iter().enumerate() {
                            let entry_bounds = Rect2D::from_origin_size(
                                Vec2::new(dropdown_bounds.min.x(), dropdown_y + ii as f32 * 28.0),
                                Vec2::new(dropdown_width, 28.0),
                            );
                            if entry_bounds.contains(input.mouse_pos) && !entry.disabled {
                                if let Some(ref callback) = entry.on_click {
                                    callbacks.invoke(callback, tree.actions_mut());
                                }
                                tree.state_arena_mut().set(group.open_id, false);
                                result.input_consumed = true;
                                break;
                            }
                        }
                    }

                    if !group_bounds.contains(input.mouse_pos)
                        && !dropdown_bounds.contains(input.mouse_pos)
                        && input.mouse_clicked(mouse_button::LEFT)
                    {
                        tree.state_arena_mut().set(group.open_id, false);
                        result.input_consumed = true;
                    }
                }

                x += label_size + item_spacing * 2.0;
            }
        }

        ViewDescriptor::TreeView(desc) => {
            let Some(node_bounds) = bounds_map.get(&hit.id).copied() else {
                return result;
            };
            let row_height = desc.row_height;
            let indent = desc.indent_per_level;
            let item_spacing = 8.0_f32;

            let visible_indices =
                compute_visible_tree_items_input(&desc.items, tree, desc.expanded_id);
            let scroll_offset: f32 = tree.state_arena().get(desc.scroll_id);
            let selected: Option<u64> = tree.state_arena().get(desc.selected_id);

            let visible_count = visible_indices.len();
            let first_row =
                ((scroll_offset.max(0.0) / row_height).floor() as usize).min(visible_count);
            let last_row = ((scroll_offset + node_bounds.height()) / row_height).ceil() as usize;
            let last_row = last_row.min(visible_count);

            for (vis_idx, &data_idx) in visible_indices
                .iter()
                .enumerate()
                .skip(first_row)
                .take(last_row - first_row)
            {
                let item = &desc.items[data_idx];
                let item_y = node_bounds.min.y() + vis_idx as f32 * row_height - scroll_offset;
                let item_bounds = Rect2D::from_origin_size(
                    Vec2::new(node_bounds.min.x(), item_y),
                    Vec2::new(node_bounds.width(), row_height),
                );

                if item_bounds.contains(input.mouse_pos) {
                    let arrow_x = node_bounds.min.x() + item.depth as f32 * indent + item_spacing;
                    let chevron_bounds = Rect2D::from_origin_size(
                        Vec2::new(arrow_x, item_y),
                        Vec2::new(16.0, row_height),
                    );

                    if item.has_children
                        && chevron_bounds.contains(input.mouse_pos)
                        && input.mouse_clicked(mouse_button::LEFT)
                    {
                        let mut expanded: HashSet<u64> = tree.state_arena().get(desc.expanded_id);
                        if expanded.contains(&item.id) {
                            expanded.remove(&item.id);
                        } else {
                            expanded.insert(item.id);
                        }
                        tree.state_arena_mut().set(desc.expanded_id, expanded);
                        result.input_consumed = true;
                        break;
                    }

                    if input.mouse_clicked(mouse_button::LEFT) {
                        tree.state_arena_mut().set(desc.selected_id, Some(item.id));
                        if let Some(ref callback) = desc.on_select {
                            callbacks.invoke(callback, tree.actions_mut());
                        }
                        result.input_consumed = true;
                        break;
                    }

                    if input.mouse_clicked(mouse_button::RIGHT) {
                        tree.state_arena_mut().set(desc.selected_id, Some(item.id));
                        if let Some(ref callback) = desc.on_right_click {
                            callbacks.invoke(callback, tree.actions_mut());
                        }
                        result.input_consumed = true;
                        break;
                    }
                }
            }

            if let Some(selected_id) = selected
                && let Some(vis_pos) = visible_indices
                    .iter()
                    .position(|&idx| desc.items[idx].id == selected_id)
            {
                let data_idx = visible_indices[vis_pos];
                let item = &desc.items[data_idx];

                if input.key_pressed(KeyCode::ArrowDown) && vis_pos + 1 < visible_count {
                    tree.state_arena_mut().set(
                        desc.selected_id,
                        Some(desc.items[visible_indices[vis_pos + 1]].id),
                    );
                    result.input_consumed = true;
                } else if input.key_pressed(KeyCode::ArrowUp) && vis_pos > 0 {
                    tree.state_arena_mut().set(
                        desc.selected_id,
                        Some(desc.items[visible_indices[vis_pos - 1]].id),
                    );
                    result.input_consumed = true;
                } else if input.key_pressed(KeyCode::ArrowRight) && item.has_children {
                    let mut expanded: HashSet<u64> = tree.state_arena().get(desc.expanded_id);
                    if !expanded.contains(&item.id) {
                        expanded.insert(item.id);
                        tree.state_arena_mut().set(desc.expanded_id, expanded);
                        result.input_consumed = true;
                    }
                } else if input.key_pressed(KeyCode::ArrowLeft) && item.has_children {
                    let mut expanded: HashSet<u64> = tree.state_arena().get(desc.expanded_id);
                    if expanded.contains(&item.id) {
                        expanded.remove(&item.id);
                        tree.state_arena_mut().set(desc.expanded_id, expanded);
                        result.input_consumed = true;
                    }
                }
            }
        }

        ViewDescriptor::Modal(desc) => {
            let is_open: bool = tree.state_arena().get(desc.open_id);
            if !is_open {
                return result;
            }

            let Some(node_bounds) = bounds_map.get(&hit.id).copied() else {
                return result;
            };

            if !node_bounds.contains(input.mouse_pos) && input.mouse_clicked(mouse_button::LEFT) {
                tree.state_arena_mut().set(desc.open_id, false);
                result.input_consumed = true;
                return result;
            }

            if input.key_pressed(KeyCode::Escape) {
                tree.state_arena_mut().set(desc.open_id, false);
                result.input_consumed = true;
            }
        }

        ViewDescriptor::ContextMenu(desc) => {
            let is_open: bool = tree.state_arena().get(desc.open_id);
            if !is_open {
                return result;
            }

            let Some(node_bounds) = bounds_map.get(&hit.id).copied() else {
                return result;
            };

            if node_bounds.contains(input.mouse_pos) && input.mouse_clicked(mouse_button::LEFT) {
                let item_height = 28.0_f32;
                for (i, entry) in desc.items.iter().enumerate() {
                    let entry_bounds = Rect2D::from_origin_size(
                        Vec2::new(
                            node_bounds.min.x(),
                            node_bounds.min.y() + i as f32 * item_height,
                        ),
                        Vec2::new(node_bounds.width(), item_height),
                    );
                    if entry_bounds.contains(input.mouse_pos) && !entry.disabled {
                        if let Some(ref callback) = entry.on_click {
                            callbacks.invoke(callback, tree.actions_mut());
                        }
                        tree.state_arena_mut().set(desc.open_id, false);
                        result.input_consumed = true;
                        return result;
                    }
                }
            }

            if !node_bounds.contains(input.mouse_pos) && input.mouse_clicked(mouse_button::LEFT) {
                tree.state_arena_mut().set(desc.open_id, false);
                result.input_consumed = true;
            }
        }

        ViewDescriptor::ScrollView(desc)
            if input.scroll_delta.y() != 0.0 && bounds_map.get(&hit.id).is_some() =>
        {
            let mut offset: f32 = tree.state_arena().get(desc.scroll_state_id);
            offset -= input.scroll_delta.y() * 30.0;
            offset = offset.max(0.0);
            tree.state_arena_mut().set(desc.scroll_state_id, offset);
            result.input_consumed = true;
        }

        ViewDescriptor::Selectable { on_click, .. } if input.mouse_clicked(mouse_button::LEFT) => {
            if let Some(callback) = on_click {
                callbacks.invoke(callback, tree.actions_mut());
            }
            result.input_consumed = true;
            result.clicked_id = Some(hit.id);
        }

        ViewDescriptor::Section {
            expanded_id,
            on_remove,
            ..
        } => {
            let Some(node_bounds) = bounds_map.get(&hit.id).copied() else {
                return result;
            };
            let font_size = 14.0_f32;
            let header_height = font_size + 8.0;

            // Check remove button click first
            if on_remove.is_some() {
                let close_x = node_bounds.max.x() - font_size - 4.0;
                let close_bounds = Rect2D::from_origin_size(
                    Vec2::new(close_x, node_bounds.min.y()),
                    Vec2::new(font_size + 4.0, header_height),
                );
                if close_bounds.contains(input.mouse_pos) && input.mouse_clicked(mouse_button::LEFT)
                {
                    if let Some(callback) = on_remove {
                        callbacks.invoke(callback, tree.actions_mut());
                    }
                    result.input_consumed = true;
                    return result;
                }
            }

            // Header click toggles expanded
            let header_bounds = Rect2D::from_origin_size(
                node_bounds.min,
                Vec2::new(node_bounds.width(), header_height),
            );
            if header_bounds.contains(input.mouse_pos) && input.mouse_clicked(mouse_button::LEFT) {
                let expanded: bool = tree.state_arena().get(*expanded_id);
                tree.state_arena_mut().set(*expanded_id, !expanded);
                result.input_consumed = true;
            }
        }

        ViewDescriptor::TabBar(desc) => {
            let Some(node_bounds) = bounds_map.get(&hit.id).copied() else {
                return result;
            };
            let tab_count = desc.tabs.len().max(1);
            let tab_width = node_bounds.width() / tab_count as f32;

            if node_bounds.contains(input.mouse_pos) && input.mouse_clicked(mouse_button::LEFT) {
                let tab_index = ((input.mouse_pos.x() - node_bounds.min.x()) / tab_width)
                    .clamp(0.0, tab_count as f32 - 0.01) as usize;
                if tab_index < desc.tabs.len() {
                    tree.state_arena_mut().set(desc.selected_id, tab_index);
                    result.input_consumed = true;
                }
            }
        }

        _ => {}
    }

    // Clear active if mouse released and not a slider
    if !input.mouse_down[mouse_button::LEFT]
        && let Some(active) = tree.interaction().active_id
        && let Some(node) = tree.get(active)
        && !matches!(
            node.descriptor,
            ViewDescriptor::Slider { .. }
                | ViewDescriptor::LabeledSlider { .. }
                | ViewDescriptor::Vec3Slider { .. }
        )
    {
        tree.interaction_mut().active_id = None;
        tree.interaction_mut().drag_axis = None;
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
        callbacks.invoke(&callback, tree.actions_mut());
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

fn compute_visible_tree_items_input(
    items: &[super::descriptor::TreeItem],
    tree: &ViewTree,
    expanded_id: super::state::StateId,
) -> Vec<usize> {
    let expanded: HashSet<u64> = tree.state_arena().get(expanded_id);
    let mut visible = Vec::new();
    let mut parent_stack: Vec<u64> = Vec::new();

    for (i, item) in items.iter().enumerate() {
        while parent_stack.len() > item.depth as usize {
            parent_stack.pop();
        }

        if item.depth == 0 {
            visible.push(i);
        } else if let Some(&parent_id) = parent_stack.last() {
            if expanded.contains(&parent_id) {
                visible.push(i);
            } else {
                continue;
            }
        }

        if item.has_children {
            parent_stack.push(item.id);
        }
    }

    visible
}

fn measure_menu_label(label: &str, font_size: f32) -> f32 {
    let char_width = font_size * 0.6;
    label.chars().count() as f32 * char_width
}
