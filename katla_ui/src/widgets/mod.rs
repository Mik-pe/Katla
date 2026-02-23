//! Built-in UI widgets.
//!
//! This module contains widget implementations that build on top of
//! the core UI primitives and implement the `Widget` trait.
//!
//! # Using Builder Widgets
//!
//! ```ignore
//! use katla_ui::widgets::Button;
//!
//! // Builder pattern
//! if ui.add(Button::new("Click Me")).clicked {
//!     // handle click
//! }
//!
//! // With options
//! ui.add(Button::new("Submit").style(MyStyle::Primary));
//! ```

use crate::input::{mouse_button, KeyCode};
use crate::{Response, UiContext};
use katla_math::{Color, Rect2D, Vec2};
use crate::icons::ForkAwesome;

// =============================================================================
// Button Widget
// =============================================================================

/// A clickable button widget.
///
/// # Example
///
/// ```ignore
/// use katla_ui::widgets::Button;
///
/// if ui.add(Button::new("Click Me").bounds(my_bounds)).clicked {
///     println!("Clicked!");
/// }
/// ```
pub struct Button<'a> {
    text: &'a str,
    bounds: Rect2D,
    id: Option<&'a str>,
}

impl<'a> Button<'a> {
    /// Create a new button with text.
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            bounds: Rect2D::from_size(Vec2::new(100.0, 30.0)),
            id: None,
        }
    }

    /// Set the button bounds.
    pub fn bounds(mut self, bounds: Rect2D) -> Self {
        self.bounds = bounds;
        self
    }

    /// Set a custom ID (for unique identification).
    pub fn id(mut self, id: &'a str) -> Self {
        self.id = Some(id);
        self
    }
}

impl<'a> crate::Widget for Button<'a> {
    fn ui(self, ui: &mut UiContext) -> Response {
        let id = self.id.unwrap_or(self.text);
        ui.button(id, self.text, self.bounds)
    }
}

// =============================================================================
// Checkbox Widget
// =============================================================================

/// A checkbox widget with label.
///
/// # Example
///
/// ```ignore
/// use katla_ui::widgets::Checkbox;
///
/// let mut checked = false;
/// if ui.add(Checkbox::new(&mut checked, "Enable feature")).changed {
///     println!("Checkbox changed to: {}", checked);
/// }
/// ```
pub struct Checkbox<'a> {
    checked: &'a mut bool,
    label: &'a str,
    bounds: Rect2D,
    id: Option<&'a str>,
}

impl<'a> Checkbox<'a> {
    /// Create a new checkbox.
    pub fn new(checked: &'a mut bool, label: &'a str) -> Self {
        Self {
            checked,
            label,
            bounds: Rect2D::from_size(Vec2::new(150.0, 24.0)),
            id: None,
        }
    }

    /// Set the checkbox bounds.
    pub fn bounds(mut self, bounds: Rect2D) -> Self {
        self.bounds = bounds;
        self
    }

    /// Set a custom ID.
    pub fn id(mut self, id: &'a str) -> Self {
        self.id = Some(id);
        self
    }
}

impl<'a> crate::Widget for Checkbox<'a> {
    fn ui(self, ui: &mut UiContext) -> Response {
        let id = self.id.unwrap_or(self.label);
        ui.checkbox(id, self.label, self.checked, self.bounds)
    }
}

// =============================================================================
// Slider Widget
// =============================================================================

/// A slider widget for numeric values.
///
/// # Example
///
/// ```ignore
/// use katla_ui::widgets::Slider;
///
/// let mut volume = 0.5;
/// if ui.add(Slider::new(&mut volume, 0.0..=1.0)).changed {
///     println!("Volume: {}", volume);
/// }
/// ```
pub struct Slider<'a> {
    value: &'a mut f32,
    range: std::ops::RangeInclusive<f32>,
    bounds: Rect2D,
    id: Option<&'a str>,
}

impl<'a> Slider<'a> {
    /// Create a new slider with value and range.
    pub fn new(value: &'a mut f32, range: std::ops::RangeInclusive<f32>) -> Self {
        Self {
            value,
            range,
            bounds: Rect2D::from_size(Vec2::new(150.0, 20.0)),
            id: None,
        }
    }

    /// Set the slider bounds.
    pub fn bounds(mut self, bounds: Rect2D) -> Self {
        self.bounds = bounds;
        self
    }

    /// Set a custom ID.
    pub fn id(mut self, id: &'a str) -> Self {
        self.id = Some(id);
        self
    }
}

impl<'a> crate::Widget for Slider<'a> {
    fn ui(self, ui: &mut UiContext) -> Response {
        let id = self.id.map(|s| s.to_string())
            .unwrap_or_else(|| format!("slider_{:?}", self.range));
        ui.slider(&id, self.value, *self.range.start(), *self.range.end(), self.bounds)
    }
}

// =============================================================================
// TextInput Widget
// =============================================================================

/// A text input field widget.
///
/// # Example
///
/// ```ignore
/// use katla_ui::widgets::TextInput;
///
/// let mut name = String::new();
/// if ui.add(TextInput::new(&mut name).placeholder("Enter name...")).changed {
///     println!("Name: {}", name);
/// }
/// ```
pub struct TextInput<'a> {
    text: &'a mut String,
    placeholder: Option<&'a str>,
    bounds: Rect2D,
    id: Option<&'a str>,
}

impl<'a> TextInput<'a> {
    /// Create a new text input.
    pub fn new(text: &'a mut String) -> Self {
        Self {
            text,
            placeholder: None,
            bounds: Rect2D::from_size(Vec2::new(200.0, 24.0)),
            id: None,
        }
    }

    /// Set placeholder text (shown when empty).
    pub fn placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = Some(placeholder);
        self
    }

    /// Set the input bounds.
    pub fn bounds(mut self, bounds: Rect2D) -> Self {
        self.bounds = bounds;
        self
    }

    /// Set a custom ID.
    pub fn id(mut self, id: &'a str) -> Self {
        self.id = Some(id);
        self
    }
}

impl<'a> crate::Widget for TextInput<'a> {
    fn ui(self, ui: &mut UiContext) -> Response {
        let id = self.id.unwrap_or("text_input");
        let response = ui.text_input(id, self.text, self.bounds);

        // Draw placeholder if empty
        if self.text.is_empty() {
            if let Some(placeholder) = self.placeholder {
                let padding = 4.0;
                let text_pos = Vec2::new(
                    self.bounds.min.x() + padding,
                    self.bounds.center().y() - ui.style.font_size * 0.5,
                );
                ui.draw_text(placeholder, text_pos, ui.style.text_color * 0.5, ui.style.font_size);
            }
        }

        response
    }
}

// =============================================================================
// Label Widget
// =============================================================================

/// A text label widget.
///
/// # Example
///
/// ```ignore
/// use katla_ui::widgets::Label;
///
/// ui.add(Label::new("Hello, World!"));
/// ```
pub struct Label<'a> {
    text: &'a str,
    bounds: Rect2D,
    color: Option<Color>,
}

impl<'a> Label<'a> {
    /// Create a new label.
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            bounds: Rect2D::from_size(Vec2::new(100.0, 20.0)),
            color: None,
        }
    }

    /// Set the label bounds.
    pub fn bounds(mut self, bounds: Rect2D) -> Self {
        self.bounds = bounds;
        self
    }

    /// Set a custom text color.
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

impl<'a> crate::Widget for Label<'a> {
    fn ui(self, ui: &mut UiContext) -> Response {
        let text_size = ui.measure_text(self.text, ui.style.font_size);
        let text_pos = Vec2::new(
            self.bounds.min.x() + (self.bounds.width() - text_size.x()) * 0.5,
            self.bounds.center().y() - text_size.y() * 0.5,
        );
        let color = self.color.unwrap_or(ui.style.text_color);
        ui.draw_text(self.text, text_pos, color, ui.style.font_size);
        Response::new(self.bounds)
    }
}

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
