//! Internal container widget implementations.
//!
//! This module contains rendering logic for container widgets like windows
//! and headers. These are private implementation details.

use katla_math::{Rect2D, Vec2};

use crate::icons::ForkAwesome;
use crate::text::FontId;

use super::super::{UiContext, WindowState};

impl UiContext {
    /// Begin a window container.
    ///
    /// Returns a WindowState for window information.
    /// Call `end_window()` after adding contents.
    pub fn begin_window(&mut self, id: &str, bounds: Rect2D) -> WindowState {
        self.begin_window_with_title(id, None, bounds)
    }

    /// Begin a window container with an optional title bar.
    ///
    /// If title is provided, draws a title bar at the top.
    /// Returns a WindowState for window information.
    /// Call `end_window()` after adding contents.
    pub fn begin_window_with_title(
        &mut self,
        id: &str,
        title: Option<&str>,
        bounds: Rect2D,
    ) -> WindowState {
        let window_id = self.generate_id(id);

        // Title bar height
        let title_height = if title.is_some() { 25.0 } else { 0.0 };

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

    /// Begin a collapsible header/panel.
    ///
    /// Returns true if the header is expanded.
    pub fn begin_header(&mut self, id: &str, label: &str, open: &mut bool, bounds: Rect2D) -> bool {
        let widget_id = self.generate_id(id);

        // Click to toggle
        if self.button_behavior(widget_id, bounds) {
            *open = !*open;
        }

        // Draw header background
        let bg_color = if *open {
            self.style.window_title_bg_active
        } else {
            self.style.window_title_bg
        };
        self.draw_rect(bounds, bg_color);

        // Draw expand/collapse icon
        let icon = if *open {
            ForkAwesome::CHEVRON_DOWN
        } else {
            ForkAwesome::CHEVRON_RIGHT
        };
        let icon_size = self.style.font_size;
        let icon_pos = Vec2::new(bounds.min.x() + 4.0, bounds.center().y() - icon_size * 0.5);
        self.draw_icon_aligned(
            icon,
            icon_pos,
            icon_size,
            self.style.text_color,
            FontId::DEFAULT,
        );

        // Draw label text after icon
        let text_size = self.measure_text(label, self.style.font_size);
        let text_pos = Vec2::new(
            bounds.min.x() + icon_size + 8.0,
            bounds.center().y() - text_size.y() * 0.5,
        );
        self.draw_text(label, text_pos, self.style.text_color, self.style.font_size);

        *open
    }

    /// Begin a child region with clipping.
    ///
    /// Returns the content area bounds.
    pub fn begin_child(&mut self, _id: &str, bounds: Rect2D) -> Rect2D {
        // Draw background
        self.draw_rect(bounds, self.style.window_bg);

        // Push clip
        self.push_clip(bounds);

        // Return content area (with padding)
        bounds.contract(self.style.window_padding)
    }

    /// End a child region.
    pub fn end_child(&mut self) {
        self.pop_clip();
    }
}
