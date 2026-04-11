//! High-level helper methods for common UI patterns.
//!
//! These methods provide ergonomic wrappers around low-level UI primitives,
//! reducing boilerplate and ensuring consistent styling across the application.

use crate::{FontSize, UiContext};
use katla_math::{Rect2D, Vec2};

impl UiContext {
    /// Display a labeled property row (label: value on same line).
    ///
    /// Creates a horizontal row with a fixed-width label and a value that
    /// fills the remaining space. Commonly used in inspectors and property panels.
    ///
    /// # Example
    /// ```ignore
    /// ui.property_row("Position:", "(1.0, 2.0, 3.0)");
    /// ui.property_row("Rotation:", "(0.0, 90.0, 0.0)");
    /// ```
    pub fn property_row(&mut self, label: &str, value: &str) {
        let start_x = self.cursor().x();
        let text_height = self.measure_text(value, self.style.font_size).y();

        self.draw_text(
            label,
            self.cursor(),
            self.style.text_color,
            self.style.font_size,
        );
        self.set_cursor(Vec2::new(
            start_x + self.style.property_label_width,
            self.cursor().y(),
        ));
        self.draw_text(
            value,
            self.cursor(),
            self.style.text_color,
            self.style.font_size,
        );

        self.set_cursor(Vec2::new(start_x, self.cursor().y() + text_height + 4.0));
    }

    /// Display a text label at the current cursor position.
    ///
    /// Draws text with the default text color and advances the cursor automatically.
    ///
    /// # Example
    /// ```ignore
    /// ui.label("Hello, World!");
    /// ui.label(&format!("Value: {}", x));
    /// ```
    pub fn label(&mut self, text: &str) {
        self.text_label_colored(text, self.style.text_color);
    }

    /// Display a text label at the cursor position with custom color.
    ///
    /// Draws text with the specified color and advances the cursor automatically.
    ///
    /// # Example
    /// ```ignore
    /// ui.text_label_colored("Error:", Color::RED);
    /// ui.text_label_colored(&format!("FPS: {:.1}", fps), fps_color);
    /// ```
    pub fn text_label_colored(&mut self, text: &str, color: katla_math::Color) {
        let text_size = self.measure_text(text, self.style.font_size);
        self.draw_text(text, self.cursor(), color, self.style.font_size);
        self.advance_cursor(text_size);
    }

    /// Display a section header with text.
    ///
    /// Draws styled header text and adds spacing below it.
    ///
    /// # Example
    /// ```ignore
    /// ui.header("Transform");
    /// // Now add transform properties...
    /// ```
    pub fn header(&mut self, text: &str) {
        self.draw_text(
            text,
            self.cursor(),
            self.style.text_color,
            self.scaled_font_size(FontSize::Medium),
        );
        self.spacing(20.0);
    }

    /// Draw a separator line across the current content width.
    ///
    /// Draws a horizontal separator line and adds spacing below it.
    /// The line spans from the left edge of the clip region with padding.
    ///
    /// # Example
    /// ```ignore
    /// ui.separator_line();
    /// ```
    pub fn separator_line(&mut self) {
        let y = self.cursor().y();
        let start_x = self.cursor().x();
        let width = {
            let clip = self.clip_rect();
            clip.max.x() - start_x
        };
        let padding = self.style.window_padding;

        self.draw_line(
            Vec2::new(start_x, y),
            Vec2::new(start_x + width - padding * 2.0, y),
            self.style.separator,
            1.0,
        );
        self.spacing(self.style.separator_height);
    }

    /// Display a separator text (vertical bar "|") with spacing.
    ///
    /// Commonly used in status bars and toolbars to visually separate items.
    ///
    /// # Example
    /// ```ignore
    /// ui.text_label("FPS: 60");
    /// ui.separator_text();
    /// ui.text_label("Frame: 1234");
    /// ```
    pub fn separator_text(&mut self) {
        self.text_label_colored("|", self.style.text_disabled);
    }

    /// Display a named section with header and separator.
    ///
    /// Creates a complete section with a styled header, the provided content,
    /// and a separator line below. This is the preferred way to group related
    /// UI elements in inspectors and panels.
    ///
    /// # Example
    /// ```ignore
    /// ui.section("Transform", || {
    ///     ui.property_row("Position:", &format!("({:.2}, {:.2}, {:.2})", x, y, z));
    ///     ui.property_row("Rotation:", &format!("({:.1}, {:.1}, {:.1})", rx, ry, rz));
    ///     ui.property_row("Scale:", &format!("({:.2}, {:.2}, {:.2})", sx, sy, sz));
    /// });
    /// ```
    pub fn section<F: FnOnce(&mut Self)>(&mut self, title: &str, content: F) {
        self.header(title);
        content(self);
        self.separator_line();
    }

    pub fn draw_panel_header(&mut self, bounds: Rect2D, title: &str) {
        self.draw_rect(bounds, self.style.window_title_bg);
        let text_size = self.measure_text(title, self.style.font_size);
        let text_pos = Vec2::new(
            bounds.min.x() + self.style.window_padding,
            bounds.min.y() + (bounds.height() - text_size.y()) * 0.5,
        );
        self.draw_text(title, text_pos, self.style.text_color, self.style.font_size);
    }

    pub fn draw_empty_state(&mut self, bounds: Rect2D, text: &str) {
        let font_size = self.scaled_font_size(FontSize::Medium);
        let text_size = self.measure_text(text, font_size);
        let pos = Vec2::new(
            bounds.center().x() - text_size.x() * 0.5,
            bounds.center().y() - text_size.y() * 0.5,
        );
        self.draw_text(text, pos, self.style.text_disabled, font_size);
    }

    pub fn truncate_text(&self, text: &str, max_width: f32, font_size: f32) -> String {
        let full_width = self.measure_text(text, font_size).x();
        if full_width <= max_width {
            return text.to_string();
        }

        let ellipsis = "...";
        let ellipsis_width = self.measure_text(ellipsis, font_size).x();
        let target_width = max_width - ellipsis_width;

        if target_width <= 0.0 {
            return ellipsis.to_string();
        }

        let char_indices: Vec<(usize, char)> = text.char_indices().collect();
        let mut lo = 0usize;
        let mut hi = char_indices.len();

        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let end = char_indices.get(mid).map(|(i, _)| *i).unwrap_or(text.len());
            let width = self.measure_text(&text[..end], font_size).x();
            if width <= target_width {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }

        let end = if lo > 0 {
            char_indices.get(lo - 1).map(|(i, _)| *i).unwrap_or(0)
        } else {
            0
        };

        format!("{}{}", &text[..end], ellipsis)
    }
}
