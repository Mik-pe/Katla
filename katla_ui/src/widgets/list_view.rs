use katla_math::{Rect2D, Vec2};

use crate::{Response, ScrollArea, ScrollAreaState, UiContext, Widget};

/// A reusable, virtualized list view widget.
///
/// Only renders rows that are visible within the scroll area, making it efficient
/// for large lists. Each row has a uniform height. The render callback receives
/// the item index and its computed bounds, and is responsible for drawing and
/// handling interactions (click, right-click) directly via the `UiContext`.
///
/// # Example
///
/// ```ignore
/// use katla_ui::widgets::ListView;
///
/// let mut scroll_state = ScrollAreaState::default();
/// let mut clicked_index: Option<usize> = None;
///
/// ui.add(ListView::new("my_list", &mut scroll_state)
///     .item_count(items.len())
///     .row_height(22.0)
///     .bounds(content_bounds)
///     .render_each(|ui, index, item_bounds| {
///         ui.draw_text(&items[index].name, item_bounds.min, Color::WHITE, 14.0);
///     }));
/// ```
pub struct ListView<'a, F>
where
    F: FnMut(&mut UiContext, usize, Rect2D),
{
    id: &'a str,
    bounds: Rect2D,
    scroll_state: &'a mut ScrollAreaState,
    item_count: usize,
    row_height: f32,
    render_fn: Option<F>,
}

impl<'a, F> ListView<'a, F>
where
    F: FnMut(&mut UiContext, usize, Rect2D),
{
    /// Create a new ListView with an ID and scroll state.
    pub fn new(id: &'a str, scroll_state: &'a mut ScrollAreaState) -> Self {
        Self {
            id,
            bounds: Rect2D::default(),
            scroll_state,
            item_count: 0,
            row_height: 22.0,
            render_fn: None,
        }
    }

    /// Set the list bounds (the area the list occupies).
    pub fn bounds(mut self, bounds: Rect2D) -> Self {
        self.bounds = bounds;
        self
    }

    /// Set the total number of items.
    pub fn item_count(mut self, count: usize) -> Self {
        self.item_count = count;
        self
    }

    /// Set the uniform row height in pixels.
    pub fn row_height(mut self, height: f32) -> Self {
        self.row_height = height;
        self
    }

    /// Set the render callback. Called for each visible item with `(ui, index, item_bounds)`.
    ///
    /// The callback is responsible for drawing the item and handling interactions
    /// (click, right-click) directly using `UiContext` methods like `ui.mouse_clicked()`,
    /// `ui.is_hovered()`, etc.
    pub fn render_each(mut self, f: F) -> Self {
        self.render_fn = Some(f);
        self
    }
}

impl<F> Widget for ListView<'_, F>
where
    F: FnMut(&mut UiContext, usize, Rect2D),
{
    fn ui(mut self, ui: &mut UiContext) -> Response {
        let mut render_fn = self
            .render_fn
            .take()
            .expect("render_each callback required");

        let total_content_height = self.item_count as f32 * self.row_height + 4.0;
        let row_height = self.row_height;
        let item_count = self.item_count;
        let bounds = self.bounds;

        *self.scroll_state = ui.scroll_area(
            ScrollArea::new(self.id).max_height(bounds.height()),
            *self.scroll_state,
            bounds,
            |ui| {
                let scroll_offset = ui.scroll_offset();
                let visible_height = bounds.height();

                let first_visible_row = (scroll_offset / row_height).floor() as usize;
                let last_visible_row =
                    ((scroll_offset + visible_height) / row_height).ceil() as usize;
                let first_row = first_visible_row.min(item_count);
                let last_row = last_visible_row.min(item_count);

                for index in first_row..last_row {
                    let item_y = bounds.min.y() + 2.0 + index as f32 * row_height - scroll_offset;
                    let item_bounds = Rect2D::from_origin_size(
                        Vec2::new(bounds.min.x(), item_y),
                        Vec2::new(bounds.width(), row_height),
                    );

                    render_fn(ui, index, item_bounds);
                }

                total_content_height
            },
        );

        Response::new(bounds)
    }
}
