//! Internal container widget implementations.
//!
//! This module contains rendering logic for container widgets like windows
//! and headers. These are private implementation details.

use katla_math::{Rect2D, Vec2};

use super::super::{UiContext, WindowGuard, WindowState};

impl UiContext {
    /// Begin a window container with an optional title bar.
    ///
    /// If title is provided, draws a title bar at the top.
    /// Returns a WindowState for window information.
    /// Call `end_window()` after adding contents.
    pub fn begin_window(&mut self, id: &str, title: Option<&str>, bounds: Rect2D) -> WindowState {
        let window_id = self.generate_id(id);

        // Title bar height
        let title_height = if title.is_some() {
            self.style.title_bar_height
        } else {
            0.0
        };

        // Draw window background
        self.draw_rect(bounds, self.style.window_bg);

        // Draw title bar if provided
        if let Some(title_text) = title {
            let title_bounds =
                Rect2D::from_origin_size(bounds.min, Vec2::new(bounds.width(), title_height));
            self.draw_rect(title_bounds, self.style.window_title_bg);

            // Draw title text (top-left positioning, centered vertically in title bar)
            let text_size = self.measure_text(title_text, self.style.font_size);
            let text_pos = Vec2::new(
                bounds.min.x() + self.style.window_padding,
                bounds.min.y() + (title_height - text_size.y()) * 0.5,
            );
            self.draw_text(
                title_text,
                text_pos,
                self.style.text_color,
                self.style.font_size,
            );
        }

        // Draw border around entire window
        self.draw_rect_border(
            bounds.contract(1.0),
            self.style.window_bg,
            self.style.window_border,
            1.0,
        );

        // Content area starts below title bar
        let content_top = bounds.min.y() + title_height;
        let content_bounds = Rect2D::new(Vec2::new(bounds.min.x(), content_top), bounds.max);
        self.push_clip(content_bounds);

        WindowState {
            id: window_id,
            bounds,
            content_cursor: Vec2::new(
                bounds.min.x() + self.style.window_padding,
                content_top + self.style.window_padding,
            ),
            title_height,
        }
    }

    /// End a window container.
    pub fn end_window(&mut self) {
        self.pop_clip();
    }

    /// Begin a window container with an RAII guard that pops the clip on drop.
    ///
    /// # Example
    /// ```ignore
    /// {
    ///     let win = ui.begin_window_guard("my_window", Some("Title"), bounds);
    ///     let cursor = win.state.content_cursor;
    ///     // draw content
    /// } // auto-pops clip
    /// ```
    pub fn begin_window_guard(
        &mut self,
        id: &str,
        title: Option<&str>,
        bounds: Rect2D,
    ) -> WindowGuard<'_> {
        let state = self.begin_window(id, title, bounds);
        WindowGuard { state, ctx: self }
    }

    /// Execute a closure within a window context, automatically managing the clip.
    ///
    /// # Example
    /// ```ignore
    /// ui.with_window("my_window", Some("Title"), bounds, |ui, state| {
    ///     ui.label(state.content_cursor, "Hello");
    /// }); // auto-pops clip
    /// ```
    pub fn with_window<F, R>(&mut self, id: &str, title: Option<&str>, bounds: Rect2D, f: F) -> R
    where
        F: FnOnce(&mut Self, &WindowState) -> R,
    {
        let state = self.begin_window(id, title, bounds);
        let result = f(self, &state);
        self.end_window();
        result
    }
}
