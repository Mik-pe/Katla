use std::collections::HashSet;

use katla_math::{Rect2D, Vec2};

use crate::input::KeyCode;
use crate::{Response, ScrollArea, ScrollAreaState, UiContext, Widget};

/// Layout info provided to a custom tree row renderer.
pub struct RowInfo {
    /// Full bounds of the row.
    pub bounds: Rect2D,
    /// X position where content should start (after indentation + chevron area).
    pub content_x: f32,
    /// Whether this row is selected.
    pub is_selected: bool,
    /// Whether this row is hovered.
    pub is_hovered: bool,
}

/// Callback type for custom per-row rendering in a TreeView.
pub type RenderItemFn<'a> = dyn FnMut(&mut UiContext, &TreeItem, &RowInfo) + 'a;

/// A single item in a tree view.
#[derive(Debug, Clone)]
pub struct TreeItem {
    /// Unique identifier for this item.
    pub id: u64,
    /// Display label.
    pub label: String,
    /// Depth in the tree (0 = root).
    pub depth: u32,
    /// Whether this item has children (used to show expand/collapse toggle).
    pub has_children: bool,
}

/// Persistent state for a tree view widget.
#[derive(Debug, Clone, Default)]
pub struct TreeState {
    /// Set of expanded item IDs.
    pub expanded: HashSet<u64>,
    /// Currently selected item ID.
    pub selected: Option<u64>,
    /// Scroll offset for virtualized rendering.
    pub scroll_offset: f32,
}

impl TreeState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn toggle_expanded(&mut self, id: u64) {
        if self.expanded.contains(&id) {
            self.expanded.remove(&id);
        } else {
            self.expanded.insert(id);
        }
    }

    pub fn is_expanded(&self, id: u64) -> bool {
        self.expanded.contains(&id)
    }

    pub fn expand_all(&mut self, items: &[TreeItem]) {
        for item in items {
            if item.has_children {
                self.expanded.insert(item.id);
            }
        }
    }

    pub fn collapse_all(&mut self) {
        self.expanded.clear();
    }
}

/// A virtualized tree view widget with expand/collapse and selection support.
///
/// Renders only visible rows for efficient handling of large trees.
/// Each row shows indentation, an expand/collapse toggle, and a label.
///
/// # Example
///
/// ```ignore
/// use katla_ui::widgets::{TreeView, TreeItem, TreeState};
///
/// let mut tree_state = TreeState::new();
/// let items = vec![
///     TreeItem { id: 1, label: "Root".into(), depth: 0, has_children: true },
///     TreeItem { id: 2, label: "Child".into(), depth: 1, has_children: false },
/// ];
///
/// let resp = ui.add(TreeView::new("scene_tree", &mut tree_state)
///     .data(items)
///     .bounds(my_bounds)
///     .row_height(22.0)
///     .indent_per_level(16.0));
/// ```
pub struct TreeView<'a> {
    id: &'a str,
    state: &'a mut TreeState,
    bounds: Rect2D,
    data: Vec<TreeItem>,
    row_height: f32,
    indent_per_level: f32,
    render_item: Option<Box<RenderItemFn<'a>>>,
}

impl<'a> TreeView<'a> {
    pub fn new(id: &'a str, state: &'a mut TreeState) -> Self {
        Self {
            id,
            state,
            bounds: Rect2D::default(),
            data: Vec::new(),
            row_height: 22.0,
            indent_per_level: 16.0,
            render_item: None,
        }
    }

    pub fn bounds(mut self, bounds: Rect2D) -> Self {
        self.bounds = bounds;
        self
    }

    pub fn data(mut self, data: Vec<TreeItem>) -> Self {
        self.data = data;
        self
    }

    pub fn selected(self, _id: Option<u64>) -> Self {
        self
    }

    pub fn row_height(mut self, h: f32) -> Self {
        self.row_height = h;
        self
    }

    pub fn indent_per_level(mut self, indent: f32) -> Self {
        self.indent_per_level = indent;
        self
    }

    pub fn render_item<F>(mut self, f: F) -> Self
    where
        F: FnMut(&mut UiContext, &TreeItem, &RowInfo) + 'a,
    {
        self.render_item = Some(Box::new(f));
        self
    }

    fn compute_visible_items(&self) -> Vec<usize> {
        let mut visible = Vec::new();
        let mut parent_stack: Vec<u64> = Vec::new();

        for (i, item) in self.data.iter().enumerate() {
            while parent_stack.len() > item.depth as usize {
                parent_stack.pop();
            }

            if item.depth == 0 {
                visible.push(i);
            } else if let Some(&parent_id) = parent_stack.last() {
                if self.state.is_expanded(parent_id) {
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

impl Widget for TreeView<'_> {
    fn ui(self, ui: &mut UiContext) -> Response {
        let visible_indices = self.compute_visible_items();
        let visible_count = visible_indices.len();
        let bounds = self.bounds;
        let row_height = self.row_height;
        let indent_per_level = self.indent_per_level;

        let scroll_state = ScrollAreaState {
            scroll_offset: self.state.scroll_offset,
            content_height: 0.0,
            stick_to_bottom: false,
            at_bottom: false,
        };

        let state_ptr = self.state as *mut TreeState;

        let mut toggle_clicked: Option<u64> = None;
        let mut row_clicked: Option<u64> = None;
        let mut row_right_clicked: Option<u64> = None;

        let mut render_item = self.render_item;

        let scroll_result = ui.scroll_area(
            ScrollArea::new(self.id).max_height(bounds.height()),
            scroll_state,
            bounds,
            |ui| {
                let scroll_offset = ui.scroll_offset();
                let padding = ui.style.item_inner_spacing;
                let total_content_height = visible_count as f32 * row_height + padding * 2.0;

                let first_visible_row =
                    ((scroll_offset - padding).max(0.0) / row_height).floor() as usize;
                let last_visible_row =
                    ((scroll_offset - padding + bounds.height()) / row_height).ceil() as usize;
                let first_row = first_visible_row.min(visible_count);
                let last_row = last_visible_row.min(visible_count);

                let font_size = ui.style.font_size;
                let selected_color = ui.style.selectable_selected;
                let hovered_color = ui.style.selectable_hovered;
                let text_color = ui.style.text_color;
                let arrow_color = ui.style.text_disabled;
                let item_spacing = ui.style.item_inner_spacing;

                for vis_idx in first_row..last_row {
                    let data_idx = visible_indices[vis_idx];
                    let item = &self.data[data_idx];

                    let item_y =
                        bounds.min.y() + padding + vis_idx as f32 * row_height - scroll_offset;
                    let item_bounds = Rect2D::from_origin_size(
                        Vec2::new(bounds.min.x(), item_y),
                        Vec2::new(bounds.width(), row_height),
                    );

                    let is_selected = self.state.selected == Some(item.id);
                    let row_hovered = ui.is_hovered(item_bounds);

                    if is_selected {
                        ui.draw_rect(item_bounds, selected_color);
                    } else if row_hovered {
                        ui.draw_rect(item_bounds, hovered_color);
                    }

                    let guide_color = ui.style.border;
                    for depth_level in 0..item.depth {
                        let guide_x =
                            bounds.min.x() + depth_level as f32 * indent_per_level + item_spacing;
                        ui.draw_line(
                            Vec2::new(guide_x, item_bounds.min.y()),
                            Vec2::new(guide_x, item_bounds.max.y()),
                            guide_color,
                            1.0,
                        );
                    }

                    let indent = item.depth as f32 * indent_per_level;
                    let arrow_x = bounds.min.x() + indent + item_spacing;
                    let arrow_y = item_bounds.center().y() - font_size * 0.5;

                    if item.has_children {
                        let arrow_char = if self.state.is_expanded(item.id) {
                            '▼'
                        } else {
                            '▶'
                        };
                        let arrow_text = arrow_char.to_string();
                        let arrow_size = ui.measure_text(&arrow_text, font_size);
                        ui.draw_text(
                            &arrow_text,
                            Vec2::new(arrow_x, arrow_y),
                            arrow_color,
                            font_size,
                        );

                        let chevron_bounds = Rect2D::from_origin_size(
                            Vec2::new(arrow_x, item_y),
                            Vec2::new(arrow_size.x().max(12.0), row_height),
                        );
                        let chevron_hovered = ui.is_hovered(chevron_bounds);
                        if chevron_hovered
                            && ui.input.mouse_pressed[crate::input::mouse_button::LEFT]
                        {
                            toggle_clicked = Some(item.id);
                        }
                    }

                    let content_x = arrow_x + indent_per_level;

                    if let Some(ref mut render_fn) = render_item {
                        let row_info = RowInfo {
                            bounds: item_bounds,
                            content_x,
                            is_selected,
                            is_hovered: row_hovered,
                        };
                        render_fn(ui, item, &row_info);
                    } else {
                        let label_y = item_bounds.center().y() - font_size * 0.5;
                        ui.draw_text(
                            &item.label,
                            Vec2::new(content_x, label_y),
                            text_color,
                            font_size,
                        );
                    }

                    if row_hovered
                        && ui.input.mouse_pressed[crate::input::mouse_button::LEFT]
                        && toggle_clicked.is_none()
                    {
                        row_clicked = Some(item.id);
                    }

                    if row_hovered && ui.input.mouse_pressed[crate::input::mouse_button::RIGHT] {
                        row_right_clicked = Some(item.id);
                    }
                }

                total_content_height
            },
        );

        let state = unsafe { &mut *state_ptr };
        state.scroll_offset = scroll_result.scroll_offset;

        if let Some(id) = toggle_clicked {
            state.toggle_expanded(id);
        }
        if let Some(id) = row_clicked {
            state.selected = Some(id);
        }

        if let Some(selected_id) = state.selected {
            if let Some(vis_pos) = visible_indices
                .iter()
                .position(|&idx| self.data[idx].id == selected_id)
            {
                let data_idx = visible_indices[vis_pos];
                let item = &self.data[data_idx];

                if ui.key_pressed(KeyCode::ArrowDown) {
                    if vis_pos + 1 < visible_count {
                        state.selected = Some(self.data[visible_indices[vis_pos + 1]].id);
                    }
                } else if ui.key_pressed(KeyCode::ArrowUp) {
                    if vis_pos > 0 {
                        state.selected = Some(self.data[visible_indices[vis_pos - 1]].id);
                    }
                } else if ui.key_pressed(KeyCode::ArrowRight) {
                    if item.has_children && !state.is_expanded(item.id) {
                        state.toggle_expanded(item.id);
                    } else if vis_pos + 1 < visible_count {
                        let next_idx = visible_indices[vis_pos + 1];
                        if self.data[next_idx].depth == item.depth + 1 {
                            state.selected = Some(self.data[next_idx].id);
                        }
                    }
                } else if ui.key_pressed(KeyCode::ArrowLeft) {
                    if item.has_children && state.is_expanded(item.id) {
                        state.toggle_expanded(item.id);
                    } else if item.depth > 0 {
                        for &idx in &visible_indices[..vis_pos] {
                            if self.data[idx].depth == item.depth - 1 {
                                state.selected = Some(self.data[idx].id);
                            }
                        }
                    }
                }
            }
        } else if !visible_indices.is_empty()
            && (ui.key_pressed(KeyCode::ArrowDown) || ui.key_pressed(KeyCode::ArrowUp))
        {
            state.selected = Some(self.data[visible_indices[0]].id);
        }

        let mut response = Response::new(bounds);
        response.clicked = row_clicked.is_some() || toggle_clicked.is_some();
        response.right_clicked = row_right_clicked.is_some();
        response.changed = row_clicked.is_some();
        response
    }
}
