//! High-level helper methods for common UI patterns.
//!
//! These methods provide ergonomic wrappers around low-level UI primitives,
//! reducing boilerplate and ensuring consistent styling across the application.

use katla_math::Vec2;
use crate::{FontSize, UiContext};
use crate::widgets::Label;

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
        let label_width = 60.0;
        
        self.begin_row();
        self.add(Label::new(label).at_cursor_width(self, label_width));
        self.advance_cursor(Vec2::new(label_width, 20.0));
        self.add(Label::new(value).at_cursor_width(self, 0.0)); // 0.0 = measure text
        self.advance_cursor(Vec2::new(0.0, 20.0));
        self.end_row();
    }

    /// Display a text label at the current cursor position.
    ///
    /// Convenience wrapper for creating and adding a Label widget.
    ///
    /// # Example
    /// ```ignore
    /// ui.label("Hello, World!");
    /// ui.label(&format!("Value: {}", x));
    /// ```
    pub fn label(&mut self, text: &str) {
        self.add(Label::new(text).at_cursor(self));
        self.spacing(20.0);
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
        let clip = self.clip_rect();
        let y = self.cursor().y();
        let padding = 8.0;
        
        self.draw_line(
            Vec2::new(clip.min.x() + padding, y),
            Vec2::new(clip.max.x() - padding, y),
            self.style.separator,
            1.0,
        );
        self.spacing(8.0);
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
}
