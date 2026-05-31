use std::any::Any;
use std::collections::HashSet;

use crate::input::{KeyCode, mouse_button};
use katla_icons::ForkAwesome;
use katla_math::{Rect2D, Vec2};
use taffy::Style;

use super::super::animation::AnimationState;
use super::super::descriptor::{Callback, TreeItem};
use super::super::diff::DiffAction;
use super::super::state::{StateArena, StateId, ViewId};
use super::super::widget::{DrawInteraction, InputContext, InputResult, MeasureFn, Widget};
use crate::context::UiContext;

pub(crate) struct TreeView {
    pub items: Vec<TreeItem>,
    pub expanded_id: StateId,
    pub selected_id: StateId,
    pub scroll_id: StateId,
    pub row_height: f32,
    pub indent_per_level: f32,
    pub on_select: Option<Callback>,
    pub on_right_click: Option<Callback>,
    children: Vec<ViewId>,
}

impl TreeView {
    pub fn new(
        items: Vec<TreeItem>,
        expanded_id: StateId,
        selected_id: StateId,
        scroll_id: StateId,
        row_height: f32,
        indent_per_level: f32,
        on_select: Option<Callback>,
        on_right_click: Option<Callback>,
    ) -> Self {
        Self {
            items,
            expanded_id,
            selected_id,
            scroll_id,
            row_height,
            indent_per_level,
            on_select,
            on_right_click,
            children: Vec::new(),
        }
    }

    pub fn compute_visible_items(items: &[TreeItem], expanded: &HashSet<u64>) -> Vec<usize> {
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
}

impl Widget for TreeView {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<TreeView>().is_some() {
            DiffAction::Update
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, _measure: MeasureFn<'_>) -> Style {
        Style {
            flex_grow: 1.0,
            flex_shrink: 1.0,
            ..Style::default()
        }
    }

    fn handle_input(
        &self,
        ctx: &mut InputContext<'_>,
        state: &mut StateArena,
        bounds: Rect2D,
        _children: &[ViewId],
    ) -> InputResult {
        let row_height = self.row_height;
        let indent = self.indent_per_level;
        let item_spacing = 8.0_f32;

        let expanded: HashSet<u64> = state.get(self.expanded_id).unwrap_or_default();
        let visible_indices = Self::compute_visible_items(&self.items, &expanded);
        let scroll_offset: f32 = state.get(self.scroll_id).unwrap_or_default();
        let selected: Option<u64> = state.get(self.selected_id).unwrap_or_default();

        let visible_count = visible_indices.len();
        let first_row = ((scroll_offset.max(0.0) / row_height).floor() as usize).min(visible_count);
        let last_row = ((scroll_offset + bounds.height()) / row_height).ceil() as usize;
        let last_row = last_row.min(visible_count);

        for (vis_idx, &data_idx) in visible_indices
            .iter()
            .enumerate()
            .skip(first_row)
            .take(last_row - first_row)
        {
            let item = &self.items[data_idx];
            let item_y = bounds.min.y() + vis_idx as f32 * row_height - scroll_offset;
            let item_bounds = Rect2D::from_origin_size(
                Vec2::new(bounds.min.x(), item_y),
                Vec2::new(bounds.width(), row_height),
            );

            if item_bounds.contains(ctx.mouse_pos) {
                let arrow_x = bounds.min.x() + item.depth as f32 * indent + item_spacing;
                let chevron_bounds = Rect2D::from_origin_size(
                    Vec2::new(arrow_x, item_y),
                    Vec2::new(16.0, row_height),
                );

                if item.has_children
                    && chevron_bounds.contains(ctx.mouse_pos)
                    && ctx.input.mouse_clicked(mouse_button::LEFT)
                {
                    let mut expanded: HashSet<u64> =
                        state.get(self.expanded_id).unwrap_or_default();
                    if expanded.contains(&item.id) {
                        expanded.remove(&item.id);
                    } else {
                        expanded.insert(item.id);
                    }
                    state.set(self.expanded_id, expanded);
                    return InputResult::Consumed;
                }

                if ctx.input.mouse_clicked(mouse_button::LEFT) {
                    state.set(self.selected_id, Some(item.id));
                    if let Some(ref callback) = self.on_select {
                        ctx.callbacks.invoke(callback, ctx.actions);
                    }
                    return InputResult::Consumed;
                }

                if ctx.input.mouse_clicked(mouse_button::RIGHT) {
                    state.set(self.selected_id, Some(item.id));
                    if let Some(ref callback) = self.on_right_click {
                        ctx.callbacks.invoke(callback, ctx.actions);
                    }
                    return InputResult::Consumed;
                }
            }
        }

        // Keyboard navigation
        if let Some(selected_id) = selected
            && let Some(vis_pos) = visible_indices
                .iter()
                .position(|&idx| self.items[idx].id == selected_id)
        {
            let data_idx = visible_indices[vis_pos];
            let item = &self.items[data_idx];

            if ctx.input.key_pressed(KeyCode::ArrowDown) && vis_pos + 1 < visible_count {
                state.set(
                    self.selected_id,
                    Some(self.items[visible_indices[vis_pos + 1]].id),
                );
                return InputResult::Consumed;
            } else if ctx.input.key_pressed(KeyCode::ArrowUp) && vis_pos > 0 {
                state.set(
                    self.selected_id,
                    Some(self.items[visible_indices[vis_pos - 1]].id),
                );
                return InputResult::Consumed;
            } else if ctx.input.key_pressed(KeyCode::ArrowRight) && item.has_children {
                let mut expanded: HashSet<u64> = state.get(self.expanded_id).unwrap_or_default();
                if !expanded.contains(&item.id) {
                    expanded.insert(item.id);
                    state.set(self.expanded_id, expanded);
                    return InputResult::Consumed;
                }
            } else if ctx.input.key_pressed(KeyCode::ArrowLeft) && item.has_children {
                let mut expanded: HashSet<u64> = state.get(self.expanded_id).unwrap_or_default();
                if expanded.contains(&item.id) {
                    expanded.remove(&item.id);
                    state.set(self.expanded_id, expanded);
                    return InputResult::Consumed;
                }
            }
        }

        InputResult::Ignore
    }

    fn draw(
        &self,
        ctx: &mut UiContext,
        state: &StateArena,
        bounds: Rect2D,
        _animation: &AnimationState,
        _children: &[ViewId],
        _interaction: &DrawInteraction,
        _view_id: ViewId,
        _children_bounds: &[Rect2D],
    ) {
        let font_size = ctx.style().font_size;
        let row_height = self.row_height;
        let indent = self.indent_per_level;
        let item_spacing = ctx.style().item_inner_spacing;

        let expanded: HashSet<u64> = state.get(self.expanded_id).unwrap_or_default();
        let visible_indices = Self::compute_visible_items(&self.items, &expanded);

        let scroll_offset: f32 = state.get(self.scroll_id).unwrap_or_default();
        let selected_id: Option<u64> = state.get(self.selected_id).unwrap_or_default();

        let visible_count = visible_indices.len();
        let first_row = ((scroll_offset.max(0.0) / row_height).floor() as usize).min(visible_count);
        let last_row = ((scroll_offset + bounds.height()) / row_height).ceil() as usize;
        let last_row = last_row.min(visible_count);

        for (vis_idx, &data_idx) in visible_indices
            .iter()
            .enumerate()
            .skip(first_row)
            .take(last_row - first_row)
        {
            let item = &self.items[data_idx];
            let item_y = bounds.min.y() + vis_idx as f32 * row_height - scroll_offset;
            let item_bounds = Rect2D::from_origin_size(
                Vec2::new(bounds.min.x(), item_y),
                Vec2::new(bounds.width(), row_height),
            );

            let is_selected = selected_id == Some(item.id);
            let row_hovered =
                bounds.contains(ctx.mouse_pos()) && item_bounds.contains(ctx.mouse_pos());

            if is_selected {
                ctx.draw_rect(item_bounds, ctx.style().selectable_selected);
            } else if row_hovered {
                ctx.draw_rect(item_bounds, ctx.style().selectable_hovered);
            }

            for depth_level in 0..item.depth {
                let guide_x = bounds.min.x() + depth_level as f32 * indent + item_spacing;
                ctx.draw_line(
                    Vec2::new(guide_x, item_bounds.min.y()),
                    Vec2::new(guide_x, item_bounds.max.y()),
                    ctx.style().border,
                    1.0,
                );
            }

            let arrow_x = bounds.min.x() + item.depth as f32 * indent + item_spacing;
            let arrow_y = item_bounds.center().y() - font_size * 0.5;

            if item.has_children {
                let arrow_char = if expanded.contains(&item.id) {
                    ForkAwesome::CHEVRON_DOWN
                } else {
                    ForkAwesome::CHEVRON_RIGHT
                };
                ctx.draw_icon(
                    arrow_char,
                    Vec2::new(arrow_x, arrow_y),
                    font_size,
                    ctx.style().text_disabled,
                );
            }

            let content_x = arrow_x + indent;
            let label_y = item_bounds.center().y() - font_size * 0.5;
            ctx.draw_text(
                &item.label,
                Vec2::new(content_x, label_y),
                ctx.style().text_color,
                font_size,
            );
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn children(&self) -> &[ViewId] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut Vec<ViewId> {
        &mut self.children
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declarative::build::CallbackTable;

    fn make_tree_view() -> TreeView {
        let mut arena = StateArena::new();
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(0));
        let expanded_id = arena.get_or_create(view_id, HashSet::<u64>::new());
        let selected_id = arena.get_or_create(view_id, Option::<u64>::None);
        let scroll_id = arena.get_or_create(view_id, 0.0f32);
        TreeView::new(
            vec![],
            expanded_id,
            selected_id,
            scroll_id,
            20.0,
            16.0,
            None,
            None,
        )
    }

    #[test]
    fn test_tree_view_diff_same_type() {
        let a = make_tree_view();
        let b = make_tree_view();
        assert_eq!(b.diff_against(&a), DiffAction::Update);
    }

    #[test]
    fn test_tree_view_diff_different_type() {
        let tv = make_tree_view();
        let other = crate::declarative::constructors::text("hello");
        assert_eq!(tv.diff_against(&other), DiffAction::Replace);
    }

    #[test]
    fn test_tree_view_children() {
        let mut tv = make_tree_view();
        assert!(tv.children().is_empty());
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(1));
        tv.children_mut().push(view_id);
        assert_eq!(tv.children().len(), 1);
    }

    #[test]
    fn test_tree_view_click_selects_item() {
        let mut arena = StateArena::new();
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(0));
        let expanded_id = arena.get_or_create(view_id, HashSet::<u64>::new());
        let selected_id = arena.get_or_create(view_id, Option::<u64>::None);
        let scroll_id = arena.get_or_create(view_id, 0.0f32);

        let mut callbacks = CallbackTable::new();
        let cb = callbacks.push(|_actions| {});

        let tree_view = TreeView::new(
            vec![
                TreeItem {
                    id: 1,
                    label: "Item 1".into(),
                    depth: 0,
                    has_children: false,
                },
                TreeItem {
                    id: 2,
                    label: "Item 2".into(),
                    depth: 0,
                    has_children: false,
                },
            ],
            expanded_id,
            selected_id,
            scroll_id,
            20.0,
            16.0,
            Some(cb),
            None,
        );

        let mut input = crate::input::UiInputState::default();
        input.set_mouse_pos(Vec2::new(50.0, 25.0));
        input.set_mouse_button(mouse_button::LEFT, true);

        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: Vec2::new(50.0, 25.0),
            callbacks: &mut callbacks,
            actions: &mut actions,
        };

        let bounds = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(200.0, 200.0));
        let result = tree_view.handle_input(&mut ctx, &mut arena, bounds, &[]);
        assert_eq!(result, InputResult::Consumed);

        let selected: Option<u64> = arena.get(selected_id).unwrap_or_default();
        assert_eq!(selected, Some(2), "clicking second item should select id 2");
    }

    #[test]
    fn test_tree_view_expand_collapse() {
        let mut arena = StateArena::new();
        let view_id = ViewId::from(slotmap::KeyData::from_ffi(0));
        let expanded_id = arena.get_or_create(view_id, HashSet::<u64>::new());
        let selected_id = arena.get_or_create(view_id, Option::<u64>::None);
        let scroll_id = arena.get_or_create(view_id, 0.0f32);

        let tree_view = TreeView::new(
            vec![
                TreeItem {
                    id: 1,
                    label: "Parent".into(),
                    depth: 0,
                    has_children: true,
                },
                TreeItem {
                    id: 2,
                    label: "Child".into(),
                    depth: 1,
                    has_children: false,
                },
            ],
            expanded_id,
            selected_id,
            scroll_id,
            20.0,
            16.0,
            None,
            None,
        );

        let mut input = crate::input::UiInputState::default();
        input.set_mouse_pos(Vec2::new(16.0, 5.0)); // chevron area
        input.set_mouse_button(mouse_button::LEFT, true);

        let mut callbacks = CallbackTable::new();
        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: Vec2::new(16.0, 5.0),
            callbacks: &mut callbacks,
            actions: &mut actions,
        };

        let bounds = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(200.0, 200.0));
        let result = tree_view.handle_input(&mut ctx, &mut arena, bounds, &[]);
        assert_eq!(result, InputResult::Consumed);

        let expanded: HashSet<u64> = arena.get(expanded_id).unwrap_or_default();
        assert!(
            expanded.contains(&1),
            "clicking chevron should expand parent"
        );
    }

    #[test]
    fn test_tree_view_compute_visible_items() {
        let items = vec![
            TreeItem {
                id: 1,
                label: "A".into(),
                depth: 0,
                has_children: true,
            },
            TreeItem {
                id: 2,
                label: "B".into(),
                depth: 1,
                has_children: false,
            },
            TreeItem {
                id: 3,
                label: "C".into(),
                depth: 0,
                has_children: false,
            },
        ];
        let mut expanded = HashSet::new();
        expanded.insert(1);

        let visible = TreeView::compute_visible_items(&items, &expanded);
        assert_eq!(visible, vec![0, 1, 2]);

        let collapsed = HashSet::new();
        let visible_collapsed = TreeView::compute_visible_items(&items, &collapsed);
        assert_eq!(visible_collapsed, vec![0, 2]);
    }

    #[test]
    fn test_tree_view_focusable() {
        let tv = make_tree_view();
        assert!(tv.focusable());
    }
}
