//! Built-in UI widgets.
//!
//! This module contains widget implementations that build on top of
//! the core UI primitives and implement the `Widget` trait.

use crate::{Response, UiContext};
use katla_math::{Color, Rect2D};

/// A labeled separator widget.
///
/// # Example
///
/// ```ignore
/// use katla_ui::widgets::Separator;
///
/// ui.add(Separator::new().label("Section Title"));
/// ```
pub struct Separator {
    label: Option<&'static str>,
    color: Option<Color>,
}

impl Separator {
    /// Create a new separator.
    pub fn new() -> Self {
        Self {
            label: None,
            color: None,
        }
    }

    /// Add a label to the separator.
    pub fn label(mut self, label: &'static str) -> Self {
        self.label = Some(label);
        self
    }

    /// Set a custom color.
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

impl Default for Separator {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::Widget for Separator {
    fn ui(self, ui: &mut UiContext) -> Response {
        let bounds = Rect2D::from_origin_size(
            ui.cursor,
            katla_math::Vec2::new(100.0, 20.0),
        );

        let color = self.color.unwrap_or(ui.style.border);
        let y = ui.cursor.y() + 10.0;

        if let Some(label) = self.label {
            let label_size = ui.measure_text(label, ui.scaled_font_size(crate::FontSize::Small));

            // Draw line before label
            ui.draw_line(
                katla_math::Vec2::new(ui.cursor.x(), y),
                katla_math::Vec2::new(ui.cursor.x() + 10.0, y),
                color,
                1.0,
            );

            // Draw label
            ui.draw_text(
                label,
                katla_math::Vec2::new(ui.cursor.x() + 16.0, ui.cursor.y() + 3.0),
                ui.style.text_color,
                ui.scaled_font_size(crate::FontSize::Small),
            );

            // Draw line after label
            let line_start = ui.cursor.x() + 20.0 + label_size.x();
            ui.draw_line(
                katla_math::Vec2::new(line_start, y),
                katla_math::Vec2::new(ui.cursor.x() + 200.0, y),
                color,
                1.0,
            );
        } else {
            // Just a line
            ui.draw_line(
                katla_math::Vec2::new(ui.cursor.x(), y),
                katla_math::Vec2::new(ui.cursor.x() + 200.0, y),
                color,
                1.0,
            );
        }

        Response::new(bounds)
    }
}

/// A colored badge/label widget.
///
/// # Example
///
/// ```ignore
/// use katla_ui::widgets::Badge;
///
/// ui.add(Badge::new("Beta", Color::new(0.2, 0.6, 1.0, 1.0)));
/// ```
pub struct Badge {
    text: &'static str,
    color: Color,
}

impl Badge {
    /// Create a new badge with text and color.
    pub fn new(text: &'static str, color: Color) -> Self {
        Self { text, color }
    }
}

impl crate::Widget for Badge {
    fn ui(self, ui: &mut UiContext) -> crate::Response {
        let padding = 4.0;
        let text_size = ui.measure_text(self.text, ui.scaled_font_size(crate::FontSize::XSmall));
        let badge_size = katla_math::Vec2::new(text_size.x() + padding * 2.0, text_size.y() + padding);

        let bounds = Rect2D::from_origin_size(ui.cursor, badge_size);

        // Background
        ui.draw_rect(bounds, self.color);

        // Text
        ui.draw_text(
            self.text,
            katla_math::Vec2::new(ui.cursor.x() + padding, ui.cursor.y() + padding / 2.0),
            Color::WHITE,
            ui.scaled_font_size(crate::FontSize::XSmall),
        );

        Response::new(bounds)
    }
}

/// A spacer widget for adding vertical/horizontal gaps.
///
/// # Example
///
/// ```ignore
/// use katla_ui::widgets::Spacer;
///
/// ui.add(Spacer::vertical(20.0));
/// ui.add(Spacer::horizontal(10.0));
/// ```
pub struct Spacer {
    width: f32,
    height: f32,
}

impl Spacer {
    /// Create vertical spacer.
    pub fn vertical(height: f32) -> Self {
        Self { width: 0.0, height }
    }

    /// Create horizontal spacer.
    pub fn horizontal(width: f32) -> Self {
        Self { width, height: 0.0 }
    }

    /// Create spacer with custom dimensions.
    pub fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

impl crate::Widget for Spacer {
    fn ui(self, ui: &mut UiContext) -> crate::Response {
        let bounds = Rect2D::from_origin_size(ui.cursor, katla_math::Vec2::new(self.width, self.height));
        Response::new(bounds)
    }
}
