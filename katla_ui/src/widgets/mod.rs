//! Builder widgets — internal rendering primitives.
//!
//! These widgets provide an immediate-mode builder pattern on top of `UiContext`.
//! They are used internally by the declarative draw pipeline and as escape hatches
//! for complex custom rendering via `ViewDescriptor::Custom`.
//!
//! For new UI code, prefer the declarative system (`crate::declarative`) which
//! provides `ViewDescriptor` variants with automatic layout, diffing, and input
//! handling. Use these builders only when a declarative equivalent does not yet
//! exist or when porting legacy immediate-mode code.

pub(crate) struct ToggleButtonParams<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub checked: bool,
    pub bounds: katla_math::Rect2D,
    pub checked_color: katla_math::Color,
    pub unchecked_color: katla_math::Color,
    pub text_color: katla_math::Color,
}

use crate::style::DEFAULTS;
use crate::{Response, UiContext};
use katla_math::{Color, Rect2D, Vec2};

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
    fill_color: Option<Color>,
    hover_color: Option<Color>,
    border_color: Option<Color>,
}

impl<'a> Button<'a> {
    /// Create a new button with text.
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            bounds: Rect2D::from_size(Vec2::new(
                DEFAULTS.button_default_width,
                DEFAULTS.button_default_height,
            )),
            id: None,
            fill_color: None,
            hover_color: None,
            border_color: None,
        }
    }

    /// Set the button bounds.
    pub fn bounds(mut self, bounds: Rect2D) -> Self {
        self.bounds = bounds;
        self
    }

    /// Position the button at the current cursor position.
    ///
    /// Uses the current UI cursor for positioning and updates the cursor
    /// after adding the button (in vertical layouts).
    ///
    /// # Example
    /// ```ignore
    /// ui.add(Button::new("Click Me").at_cursor(ui));
    /// ```
    pub fn at_cursor(mut self, ui: &UiContext) -> Self {
        self.bounds = Rect2D::from_origin_size(
            ui.cursor(),
            Vec2::new(
                ui.style.button_default_width,
                ui.style.button_default_height,
            ),
        );
        self
    }

    /// Position the button at the current cursor with custom size.
    ///
    /// # Example
    /// ```ignore
    /// ui.add(Button::new("Wide Button").at_cursor_sized(ui, 150.0, 28.0));
    /// ```
    pub fn at_cursor_sized(mut self, ui: &UiContext, width: f32, height: f32) -> Self {
        self.bounds = Rect2D::from_origin_size(ui.cursor(), Vec2::new(width, height));
        self
    }

    /// Set the button width.
    pub fn width(mut self, width: f32) -> Self {
        self.bounds =
            Rect2D::from_origin_size(self.bounds.min, Vec2::new(width, self.bounds.height()));
        self
    }

    /// Set the button height.
    pub fn height(mut self, height: f32) -> Self {
        self.bounds =
            Rect2D::from_origin_size(self.bounds.min, Vec2::new(self.bounds.width(), height));
        self
    }

    /// Set a custom ID (for unique identification).
    pub fn id(mut self, id: &'a str) -> Self {
        self.id = Some(id);
        self
    }

    /// Set a custom fill color (background).
    pub fn fill_color(mut self, color: Color) -> Self {
        self.fill_color = Some(color);
        self
    }

    /// Set a custom hover color.
    pub fn hover_color(mut self, color: Color) -> Self {
        self.hover_color = Some(color);
        self
    }

    /// Set a border color.
    pub fn border(mut self, color: Color) -> Self {
        self.border_color = Some(color);
        self
    }
}

impl<'a> crate::Widget for Button<'a> {
    fn ui(self, ui: &mut UiContext) -> Response {
        let id = self.id.unwrap_or(self.text);
        ui.button_with_colors(
            id,
            self.text,
            self.bounds,
            self.fill_color,
            self.hover_color,
            self.border_color,
        )
    }
}

// =============================================================================
// ImageButton Widget
// =============================================================================

/// A clickable button with an icon.
///
/// # Example
///
/// ```ignore
/// use katla_ui::widgets::ImageButton;
/// use katla_ui::icons::ForkAwesome;
///
/// if ui.add(ImageButton::new(ForkAwesome::ARROW_LEFT).bounds(my_bounds)).clicked {
///     println!("Clicked!");
/// }
///
/// // Disabled button
/// ui.add(ImageButton::new(ForkAwesome::TRASH).enabled(false));
/// ```
pub struct ImageButton<'a> {
    icon: char,
    bounds: Rect2D,
    id: Option<&'a str>,
    enabled: bool,
}

impl<'a> ImageButton<'a> {
    /// Create a new image button with an icon.
    pub fn new(icon: char) -> Self {
        Self {
            icon,
            bounds: Rect2D::from_size(Vec2::new(
                DEFAULTS.icon_button_size,
                DEFAULTS.icon_button_size,
            )),
            id: None,
            enabled: true,
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

    /// Set whether the button is enabled.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Position the button at the current cursor position with default size.
    pub fn at_cursor(mut self, ui: &UiContext) -> Self {
        let s = ui.style.icon_button_size;
        self.bounds = Rect2D::from_origin_size(ui.cursor(), Vec2::new(s, s));
        self
    }
}

impl<'a> crate::Widget for ImageButton<'a> {
    fn ui(self, ui: &mut UiContext) -> Response {
        let id = self.id.unwrap_or("image_btn");
        ui.image_button(id, self.icon, self.bounds, self.enabled)
    }
}

// =============================================================================
// ToggleButton Widget
// =============================================================================

/// A toggle button with a check icon when enabled.
///
/// Similar to a checkbox but styled as a full-width button, useful for
/// settings panels and preference toggles.
///
/// # Example
///
/// ```ignore
/// use katla_ui::widgets::ToggleButton;
///
/// if ui.add(ToggleButton::new(true, "Feature").bounds(my_bounds)).clicked {
///     println!("Toggled!");
/// }
/// ```
pub struct ToggleButton<'a> {
    checked: bool,
    label: &'a str,
    bounds: Rect2D,
    id: Option<&'a str>,
    checked_color: Option<Color>,
    unchecked_color: Option<Color>,
}

impl<'a> ToggleButton<'a> {
    /// Create a new toggle button.
    pub fn new(checked: bool, label: &'a str) -> Self {
        Self {
            checked,
            label,
            bounds: Rect2D::from_size(Vec2::new(
                DEFAULTS.checkbox_default_width,
                DEFAULTS.checkbox_default_height,
            )),
            id: None,
            checked_color: None,
            unchecked_color: None,
        }
    }

    /// Set the button bounds.
    pub fn bounds(mut self, bounds: Rect2D) -> Self {
        self.bounds = bounds;
        self
    }

    /// Set a custom ID.
    pub fn id(mut self, id: &'a str) -> Self {
        self.id = Some(id);
        self
    }

    /// Set the checked (on) background color.
    pub fn checked_color(mut self, color: Color) -> Self {
        self.checked_color = Some(color);
        self
    }

    /// Set the unchecked (off) background color.
    pub fn unchecked_color(mut self, color: Color) -> Self {
        self.unchecked_color = Some(color);
        self
    }

    /// Position the toggle button at the current cursor position with default size.
    pub fn at_cursor(mut self, ui: &UiContext) -> Self {
        self.bounds = Rect2D::from_origin_size(
            ui.cursor(),
            Vec2::new(
                ui.style.checkbox_default_width,
                ui.style.checkbox_default_height,
            ),
        );
        self
    }
}

impl<'a> crate::Widget for ToggleButton<'a> {
    fn ui(self, ui: &mut UiContext) -> Response {
        let id = self.id.unwrap_or(self.label);
        let checked_color = self.checked_color.unwrap_or(ui.style.selectable_selected);
        let unchecked_color = self.unchecked_color.unwrap_or(ui.style.menu_bg);
        let text_color = ui.style.button_text;
        ui.toggle_button(&ToggleButtonParams {
            id,
            label: self.label,
            checked: self.checked,
            bounds: self.bounds,
            checked_color,
            unchecked_color,
            text_color,
        })
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
/// if ui.add(Slider::new("volume", &mut volume, 0.0..=1.0)).changed {
///     println!("Volume: {}", volume);
/// }
/// ```
pub struct Slider<'a> {
    label: &'a str,
    value: &'a mut f32,
    range: std::ops::RangeInclusive<f32>,
    bounds: Rect2D,
    id: Option<&'a str>,
    show_value: bool,
    value_precision: usize,
}

impl<'a> Slider<'a> {
    /// Create a new slider with label, value, and range.
    pub fn new(label: &'a str, value: &'a mut f32, range: std::ops::RangeInclusive<f32>) -> Self {
        Self {
            label,
            value,
            range,
            bounds: Rect2D::from_size(Vec2::new(
                DEFAULTS.slider_default_width,
                DEFAULTS.slider_default_height,
            )),
            id: None,
            show_value: false,
            value_precision: 1,
        }
    }

    /// Set the slider bounds.
    pub fn bounds(mut self, bounds: Rect2D) -> Self {
        self.bounds = bounds;
        self
    }

    /// Set a custom ID (overrides label-based ID).
    pub fn id(mut self, id: &'a str) -> Self {
        self.id = Some(id);
        self
    }

    pub fn show_value(mut self, show: bool) -> Self {
        self.show_value = show;
        self
    }

    pub fn precision(mut self, p: usize) -> Self {
        self.value_precision = p;
        self
    }

    /// Position the slider at the current cursor position with default size.
    pub fn at_cursor(mut self, ui: &UiContext) -> Self {
        self.bounds = Rect2D::from_origin_size(
            ui.cursor(),
            Vec2::new(
                ui.style.slider_default_width,
                ui.style.slider_default_height,
            ),
        );
        self
    }
}

impl<'a> crate::Widget for Slider<'a> {
    fn ui(self, ui: &mut UiContext) -> Response {
        let id = self.id.unwrap_or(self.label);
        ui.slider(
            id,
            self.value,
            *self.range.start(),
            *self.range.end(),
            self.bounds,
            self.show_value,
            self.value_precision,
        )
    }
}

// =============================================================================
// LabeledSlider Widget
// =============================================================================

/// A slider with a label and optional value display in a single row.
///
/// Layout: `[label (label_width)] [slider (fills remaining)] [value text (if show_value)]`
///
/// # Example
///
/// ```ignore
/// use katla_ui::widgets::LabeledSlider;
///
/// let mut intensity = 50.0;
/// let resp = ui.add(LabeledSlider::new("Intensity", &mut intensity, 0.0..=100.0)
///     .bounds(row_bounds)
///     .label_width(90.0)
///     .show_value(true)
///     .precision(2));
/// ```
pub struct LabeledSlider<'a> {
    label: &'a str,
    value: &'a mut f32,
    range: std::ops::RangeInclusive<f32>,
    bounds: Rect2D,
    label_width: f32,
    show_value: bool,
    precision: usize,
    id: Option<&'a str>,
}

impl<'a> LabeledSlider<'a> {
    /// Create a new labeled slider with label, value, and range.
    ///
    /// Defaults: `label_width` = 80.0, `show_value` = true, `precision` = 1.
    pub fn new(label: &'a str, value: &'a mut f32, range: std::ops::RangeInclusive<f32>) -> Self {
        Self {
            label,
            value,
            range,
            bounds: Rect2D::from_size(Vec2::new(
                DEFAULTS.slider_default_width,
                DEFAULTS.slider_default_height,
            )),
            label_width: 80.0,
            show_value: true,
            precision: 1,
            id: None,
        }
    }

    /// Set the total row bounds.
    pub fn bounds(mut self, bounds: Rect2D) -> Self {
        self.bounds = bounds;
        self
    }

    /// Set the width allocated for the label text.
    pub fn label_width(mut self, w: f32) -> Self {
        self.label_width = w;
        self
    }

    /// Whether to show the formatted value on the right side.
    pub fn show_value(mut self, show: bool) -> Self {
        self.show_value = show;
        self
    }

    /// Set the decimal places for value display.
    pub fn precision(mut self, p: usize) -> Self {
        self.precision = p;
        self
    }

    /// Set a custom widget ID.
    pub fn id(mut self, id: &'a str) -> Self {
        self.id = Some(id);
        self
    }

    /// Position the labeled slider at the current cursor position with default size.
    pub fn at_cursor(mut self, ui: &UiContext) -> Self {
        self.bounds = Rect2D::from_origin_size(
            ui.cursor(),
            Vec2::new(
                ui.style.slider_default_width,
                ui.style.slider_default_height,
            ),
        );
        self
    }
}

impl<'a> crate::Widget for LabeledSlider<'a> {
    fn ui(self, ui: &mut UiContext) -> Response {
        let font_size = ui.style.font_size;
        let text_color = ui.style.text_color;
        let label_text_size = ui.measure_text(self.label, font_size);

        // Draw label text vertically centered in the label region
        let label_x = self.bounds.min.x();
        let label_y = self.bounds.center().y() - label_text_size.y() * 0.5;
        ui.draw_text(
            self.label,
            Vec2::new(label_x, label_y),
            text_color,
            font_size,
        );

        // Measure value text width if showing
        let value_text_width = if self.show_value {
            let value_text = format!("{:.1$}", *self.value, self.precision);
            let size = ui.measure_text(&value_text, font_size);
            size.x() + 8.0 // padding
        } else {
            0.0
        };

        // Slider fills the space between label and value text
        let slider_x = self.bounds.min.x() + self.label_width;
        let slider_width = (self.bounds.max.x() - value_text_width) - slider_x;
        let slider_bounds = Rect2D::from_origin_size(
            Vec2::new(slider_x, self.bounds.min.y()),
            Vec2::new(slider_width.max(0.0), self.bounds.height()),
        );

        let slider_id = self.id.unwrap_or(self.label);
        let response = ui.add_overlay(
            Slider::new(slider_id, self.value, self.range.clone())
                .bounds(slider_bounds)
                .show_value(false),
        );

        // Draw value text on the right side
        if self.show_value {
            let value_text = format!("{:.1$}", *self.value, self.precision);
            let value_text_size = ui.measure_text(&value_text, font_size);
            let value_x = self.bounds.max.x() - value_text_size.x();
            let value_y = self.bounds.center().y() - value_text_size.y() * 0.5;
            ui.draw_text(
                &value_text,
                Vec2::new(value_x, value_y),
                text_color,
                font_size,
            );
        }

        let mut result = Response::new(self.bounds);
        result.changed = response.changed;
        result.hovered = response.hovered;
        result.active = response.active;
        result.clicked = response.clicked;
        result
    }
}

// =============================================================================
// Vec3Slider Widget
// =============================================================================

/// A three-axis slider widget for Vec3/f32[3] values with colored axis labels.
///
/// Layout: `[label (above)] then per row: [axis_label (20px)] [slider] [value (40px)]`
///
/// # Example
///
/// ```ignore
/// use katla_ui::widgets::Vec3Slider;
///
/// let mut position = [10.0, 20.0, 30.0];
/// let resp = ui.add(Vec3Slider::new("Position", &mut position, -100.0..=100.0)
///     .bounds(row_bounds)
///     .precision(2));
/// ```
pub struct Vec3Slider<'a> {
    label: &'a str,
    values: &'a mut [f32; 3],
    range: std::ops::RangeInclusive<f32>,
    bounds: Rect2D,
    axis_labels: [&'a str; 3],
    axis_colors: [Color; 3],
    precision: usize,
    id: Option<&'a str>,
}

const DEFAULT_AXIS_COLORS: [Color; 3] = [
    Color::rgb(0.9, 0.3, 0.3),
    Color::rgb(0.3, 0.9, 0.3),
    Color::rgb(0.3, 0.5, 0.9),
];

impl<'a> Vec3Slider<'a> {
    pub fn new(
        label: &'a str,
        values: &'a mut [f32; 3],
        range: std::ops::RangeInclusive<f32>,
    ) -> Self {
        Self {
            label,
            values,
            range,
            bounds: Rect2D::from_size(Vec2::new(
                DEFAULTS.slider_default_width,
                DEFAULTS.slider_default_height * 3.0,
            )),
            axis_labels: ["X", "Y", "Z"],
            axis_colors: DEFAULT_AXIS_COLORS,
            precision: 1,
            id: None,
        }
    }

    pub fn bounds(mut self, bounds: Rect2D) -> Self {
        self.bounds = bounds;
        self
    }

    pub fn axis_labels(mut self, labels: [&'a str; 3]) -> Self {
        self.axis_labels = labels;
        self
    }

    pub fn axis_colors(mut self, colors: [Color; 3]) -> Self {
        self.axis_colors = colors;
        self
    }

    pub fn precision(mut self, p: usize) -> Self {
        self.precision = p;
        self
    }

    pub fn id(mut self, id: &'a str) -> Self {
        self.id = Some(id);
        self
    }
}

impl<'a> crate::Widget for Vec3Slider<'a> {
    fn ui(self, ui: &mut UiContext) -> Response {
        let font_size = ui.style.font_size;
        let text_color = ui.style.text_color;
        let axis_label_width = 20.0;
        let value_text_width = 40.0;
        let base_id = self.id.unwrap_or(self.label);

        let row_height = self.bounds.height() / 3.0;

        let mut combined = Response::new(self.bounds);
        combined.changed = false;

        for i in 0..3 {
            let row_y = self.bounds.min.y() + row_height * i as f32;
            let row_bounds = Rect2D::from_origin_size(
                Vec2::new(self.bounds.min.x(), row_y),
                Vec2::new(self.bounds.width(), row_height),
            );

            // Draw axis label with axis color
            let axis_label = self.axis_labels[i];
            let axis_color = self.axis_colors[i];
            let axis_label_size = ui.measure_text(axis_label, font_size);
            let axis_label_y = row_bounds.center().y() - axis_label_size.y() * 0.5;
            ui.draw_text(
                axis_label,
                Vec2::new(row_bounds.min.x(), axis_label_y),
                axis_color,
                font_size,
            );

            // Slider occupies the space between axis label and value text
            let slider_x = row_bounds.min.x() + axis_label_width;
            let slider_width = (row_bounds.max.x() - value_text_width) - slider_x;
            let slider_bounds = Rect2D::from_origin_size(
                Vec2::new(slider_x, row_bounds.min.y()),
                Vec2::new(slider_width.max(0.0), row_bounds.height()),
            );

            let slider_id = format!("{}_{}", base_id, i);
            let response = ui.add_overlay(
                Slider::new(&slider_id, &mut self.values[i], self.range.clone())
                    .bounds(slider_bounds)
                    .show_value(false),
            );

            // Draw value text on the right
            let value_text = format!("{:.1$}", self.values[i], self.precision);
            let value_text_size = ui.measure_text(&value_text, font_size);
            let value_x = row_bounds.max.x() - value_text_size.x();
            let value_y = row_bounds.center().y() - value_text_size.y() * 0.5;
            ui.draw_text(
                &value_text,
                Vec2::new(value_x, value_y),
                text_color,
                font_size,
            );

            combined.changed |= response.changed;
            combined.hovered |= response.hovered;
            combined.active |= response.active;
        }

        combined
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
/// if ui.add(TextInput::new("name", &mut name).placeholder("Enter name...")).changed {
///     println!("Name: {}", name);
/// }
/// ```
pub struct TextInput<'a> {
    label: &'a str,
    text: &'a mut String,
    placeholder: Option<&'a str>,
    show_clear: bool,
    multiline: bool,
    bounds: Rect2D,
    id: Option<&'a str>,
}

impl<'a> TextInput<'a> {
    /// Create a new text input with label.
    pub fn new(label: &'a str, text: &'a mut String) -> Self {
        Self {
            label,
            text,
            placeholder: None,
            show_clear: false,
            multiline: false,
            bounds: Rect2D::from_size(Vec2::new(
                DEFAULTS.text_input_default_width,
                DEFAULTS.text_input_default_height,
            )),
            id: None,
        }
    }

    /// Set placeholder text (shown when empty and not focused).
    pub fn placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = Some(placeholder);
        self
    }

    /// Show a clear button (X) on the right side when text is non-empty.
    pub fn show_clear(mut self, show: bool) -> Self {
        self.show_clear = show;
        self
    }

    /// Enable multiline input. Shift+Enter inserts a newline, Enter submits.
    pub fn multiline(mut self, multiline: bool) -> Self {
        self.multiline = multiline;
        self
    }

    /// Set the input bounds.
    pub fn bounds(mut self, bounds: Rect2D) -> Self {
        self.bounds = bounds;
        self
    }

    /// Set a custom ID (overrides label-based ID).
    pub fn id(mut self, id: &'a str) -> Self {
        self.id = Some(id);
        self
    }

    /// Position the text input at the current cursor position with default size.
    pub fn at_cursor(mut self, ui: &UiContext) -> Self {
        self.bounds = Rect2D::from_origin_size(
            ui.cursor(),
            Vec2::new(
                ui.style.text_input_default_width,
                ui.style.text_input_default_height,
            ),
        );
        self
    }
}

impl<'a> crate::Widget for TextInput<'a> {
    fn ui(self, ui: &mut UiContext) -> Response {
        let id = self.id.unwrap_or(self.label);
        ui.text_input(
            id,
            self.text,
            self.bounds,
            self.placeholder,
            self.show_clear,
            self.multiline,
        )
    }
}

// =============================================================================
// RadioButton Widget
// =============================================================================

/// A radio button for selecting one option from a group.
///
/// # Example
///
/// ```ignore
/// use katla_ui::widgets::RadioButton;
///
/// let mut selected = 0;
/// if ui.add(RadioButton::new(&mut selected, 0, "Option A")).changed {
///     println!("Selected: {}", selected);
/// }
/// ui.add(RadioButton::new(&mut selected, 1, "Option B"));
/// ui.add(RadioButton::new(&mut selected, 2, "Option C"));
/// ```
pub struct RadioButton<'a> {
    value: &'a mut usize,
    index: usize,
    label: &'a str,
    id: Option<&'a str>,
    bounds: Rect2D,
}

impl<'a> RadioButton<'a> {
    /// Create a new radio button.
    ///
    /// # Arguments
    /// * `value` - Mutable reference to the selected index
    /// * `index` - This button's index value
    /// * `label` - Text label for the button
    pub fn new(value: &'a mut usize, index: usize, label: &'a str) -> Self {
        Self {
            value,
            index,
            label,
            id: None,
            bounds: Rect2D::from_size(Vec2::new(
                DEFAULTS.radio_button_default_width,
                DEFAULTS.radio_button_default_height,
            )),
        }
    }

    pub fn id(mut self, id: &'a str) -> Self {
        self.id = Some(id);
        self
    }

    /// Set the button bounds.
    pub fn bounds(mut self, bounds: Rect2D) -> Self {
        self.bounds = bounds;
        self
    }

    /// Position the radio button at the current cursor position with default size.
    pub fn at_cursor(mut self, ui: &UiContext) -> Self {
        self.bounds = Rect2D::from_origin_size(
            ui.cursor(),
            Vec2::new(
                ui.style.radio_button_default_width,
                ui.style.radio_button_default_height,
            ),
        );
        self
    }
}

impl<'a> crate::Widget for RadioButton<'a> {
    fn ui(self, ui: &mut UiContext) -> Response {
        ui.radio_button(
            self.id.unwrap_or(self.label),
            self.value,
            self.index,
            self.label,
            self.bounds,
        )
    }
}

// =============================================================================
// Panel Widget
// =============================================================================

/// RAII guard for panel content rendering.
///
/// Restores clip state on drop.
pub struct PanelGuard<'a> {
    ui: &'a mut UiContext,
    content_bounds: Rect2D,
}

impl<'a> PanelGuard<'a> {
    /// Returns the content area bounds (below the header).
    pub fn content_bounds(&self) -> Rect2D {
        self.content_bounds
    }
}

impl Drop for PanelGuard<'_> {
    fn drop(&mut self) {
        self.ui.pop_clip();
    }
}

/// A panel builder for drawing panel chrome (background, border, header, title).
///
/// Returns a [`PanelGuard`] on `show()` that clips content to the panel area
/// and restores clip state on drop.
///
/// # Example
///
/// ```ignore
/// use katla_ui::widgets::Panel;
///
/// let guard = Panel::new("Settings")
///     .bounds(my_bounds)
///     .show(ui);
///
/// // Render content inside the panel
/// ui.add(Button::new("OK").at_cursor(ui));
///
/// // guard dropped here, clip restored
/// ```
pub struct Panel<'a> {
    title: &'a str,
    bounds: Rect2D,
    header_height: f32,
    id: Option<&'a str>,
}

impl<'a> Panel<'a> {
    /// Create a new panel with a title.
    pub fn new(title: &'a str) -> Self {
        Self {
            title,
            bounds: Rect2D::from_size(Vec2::new(200.0, 200.0)),
            header_height: 25.0,
            id: None,
        }
    }

    /// Set the panel bounds.
    pub fn bounds(mut self, bounds: Rect2D) -> Self {
        self.bounds = bounds;
        self
    }

    /// Set the header height.
    pub fn header_height(mut self, height: f32) -> Self {
        self.header_height = height;
        self
    }

    /// Set a custom ID.
    pub fn id(mut self, id: &'a str) -> Self {
        self.id = Some(id);
        self
    }

    /// Show the panel and return a guard for content rendering.
    ///
    /// The guard restores clip state on drop.
    pub fn show(self, ui: &mut UiContext) -> PanelGuard<'_> {
        let bounds = self.bounds;
        let header_height = self.header_height;

        ui.draw_rounded_rect(bounds, ui.style.window_bg, ui.style.window_rounding);
        ui.draw_rounded_selection_border(
            bounds,
            ui.style.window_border,
            1.0,
            ui.style.window_rounding,
        );

        let header_bounds =
            Rect2D::from_origin_size(bounds.min, Vec2::new(bounds.width(), header_height));

        // Draw header with rounded top corners and subtle gradient
        let header_color = ui.style.window_title_bg;
        let header_light = katla_math::Color::new(
            (header_color.r + 0.04).min(1.0),
            (header_color.g + 0.04).min(1.0),
            (header_color.b + 0.04).min(1.0),
            header_color.a,
        );
        let header_full = Rect2D::from_origin_size(
            bounds.min,
            Vec2::new(bounds.width(), header_height + ui.style.window_rounding),
        );
        ui.draw_rounded_rect(header_full, header_color, ui.style.window_rounding);
        // Gradient overlay: lighter at top, fading to header color
        ui.draw_gradient_rect(
            header_full,
            header_light,
            header_light,
            header_color,
            header_color,
        );
        // Cover the bottom rounded corners of the header
        ui.draw_rect(
            Rect2D::from_origin_size(
                Vec2::new(
                    bounds.min.x(),
                    bounds.min.y() + header_height - ui.style.window_rounding,
                ),
                Vec2::new(bounds.width(), ui.style.window_rounding + 1.0),
            ),
            header_color,
        );

        let title_size = ui.measure_text(self.title, ui.style.font_size);
        let title_pos = Vec2::new(
            bounds.min.x() + ui.style.window_padding,
            header_bounds.center().y() - title_size.y() * 0.5,
        );
        ui.draw_text(
            self.title,
            title_pos,
            ui.style.window_title_text,
            ui.style.font_size,
        );

        let content_bounds = Rect2D::from_origin_size(
            Vec2::new(bounds.min.x(), bounds.min.y() + header_height),
            Vec2::new(bounds.width(), bounds.height() - header_height),
        );
        ui.push_clip(content_bounds);

        PanelGuard { ui, content_bounds }
    }
}

mod color_picker;
pub use color_picker::{ColorPickerButton, ColorPickerState, hsv_to_rgb, rgb_to_hsv};

mod draggable_panel;
pub use draggable_panel::{
    DraggablePanel, DraggablePanelConfig, DraggablePanelFrame, DraggablePanelState, PanelState,
};

mod list_view;
pub use list_view::ListView;

mod tree;
pub use tree::{RenderItemFn, RowInfo, TreeItem, TreeState, TreeView};

mod dock;
pub use dock::{
    DockArea, DockLayout, DockNode, DockPanelId, DockTabBar, DockTabBarResponse,
    FloatingDockWindow, SplitDirection,
};

// =============================================================================
// ResizeHandle Widget
// =============================================================================

/// Direction of resize for a [`ResizeHandle`].
pub enum ResizeDirection {
    Horizontal,
    Vertical,
}

/// A thin invisible hit-region that drives panel-edge resizing.
///
/// Returns the new clamped dimension after each frame. Cursor changes and
/// drag tracking are handled internally so callers only need to feed the
/// returned value back into their layout.
///
/// # Example
///
/// ```ignore
/// use katla_ui::widgets::ResizeHandle;
///
/// let new_width = ResizeHandle::horizontal(edge_bounds, panel_width)
///     .min_value(120.0)
///     .max_value(400.0)
///     .show(ui);
/// ```
pub struct ResizeHandle {
    bounds: Rect2D,
    direction: ResizeDirection,
    current_value: f32,
    min_value: f32,
    max_value: f32,
    inverted: bool,
}

impl ResizeHandle {
    /// Create a horizontal resize handle (left/right drag changes width).
    pub fn horizontal(bounds: Rect2D, current_value: f32) -> Self {
        Self {
            bounds,
            direction: ResizeDirection::Horizontal,
            current_value,
            min_value: 0.0,
            max_value: f32::MAX,
            inverted: false,
        }
    }

    /// Create a vertical resize handle (up/down drag changes height).
    pub fn vertical(bounds: Rect2D, current_value: f32) -> Self {
        Self {
            bounds,
            direction: ResizeDirection::Vertical,
            current_value,
            min_value: 0.0,
            max_value: f32::MAX,
            inverted: false,
        }
    }

    /// Set the minimum allowed value.
    pub fn min_value(mut self, min: f32) -> Self {
        self.min_value = min;
        self
    }

    /// Set the maximum allowed value.
    pub fn max_value(mut self, max: f32) -> Self {
        self.max_value = max;
        self
    }

    /// Negate the drag delta. Use for bottom or right edges where
    /// dragging against the axis should increase the dimension.
    pub fn inverted(mut self) -> Self {
        self.inverted = true;
        self
    }

    /// Process the resize interaction and return the new clamped dimension.
    pub fn show(self, ui: &mut UiContext) -> f32 {
        let id = ui.generate_id("resize_handle");
        let hovered = ui.input.is_hovered(self.bounds);

        if hovered {
            match self.direction {
                ResizeDirection::Horizontal => {
                    ui.set_mouse_cursor(crate::input::MouseCursor::ResizeHorizontal)
                }
                ResizeDirection::Vertical => {
                    ui.set_mouse_cursor(crate::input::MouseCursor::ResizeVertical)
                }
            }
        }

        let is_active = ui.active_id == Some(id);

        if hovered && ui.input.mouse_pressed[crate::input::mouse_button::LEFT] && !is_active {
            ui.active_id = Some(id);
        }

        if is_active {
            let raw_delta = match self.direction {
                ResizeDirection::Horizontal => ui.input.mouse_delta.x(),
                ResizeDirection::Vertical => ui.input.mouse_delta.y(),
            };
            let delta = if self.inverted { -raw_delta } else { raw_delta };
            let new_value = (self.current_value + delta).clamp(self.min_value, self.max_value);

            if !ui.input.mouse_down[crate::input::mouse_button::LEFT] {
                ui.active_id = None;
            }

            new_value
        } else {
            self.current_value
        }
    }
}

// =============================================================================
// MenuBar Widget
// =============================================================================

/// A horizontal menu bar widget drawn at the top of the screen.
///
/// Uses a show/end pattern instead of the `Widget` trait so that callers
/// can add left-aligned menu items, right-aligned content (title, status),
/// and then close the row layout.
///
/// # Example
///
/// ```ignore
/// use katla_ui::widgets::MenuBar;
///
/// let bar = MenuBar::new(screen_size.x(), 32.0);
/// bar.show(ui);
///
/// // Left-side menus
/// let file_bounds = Rect2D::from_origin_size(ui.cursor(), Vec2::new(50.0, 32.0));
/// ui.menu_bar_dropdown("file", "File", file_bounds, &mut file_open, |ui, open| {
///     if ui.menu_item_clicked("New") { *open = false; }
/// });
///
/// // Right-side content
/// bar.right_side(ui);
/// ui.draw_text("Katla Engine", ui.cursor(), text_color, font_size);
///
/// bar.end(ui);
/// ```
pub struct MenuBar {
    bounds: Rect2D,
}

impl MenuBar {
    /// Create a new menu bar spanning `width` with the given `height` at the top of the screen.
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            bounds: Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(width, height)),
        }
    }

    /// Override the bounds entirely.
    pub fn bounds(mut self, bounds: Rect2D) -> Self {
        self.bounds = bounds;
        self
    }

    /// Set the y-position while keeping width and height.
    pub fn y_position(mut self, y: f32) -> Self {
        self.bounds = Rect2D::from_origin_size(
            Vec2::new(self.bounds.min.x(), y),
            Vec2::new(self.bounds.width(), self.bounds.height()),
        );
        self
    }

    /// Override the height while keeping position and width.
    pub fn height(mut self, height: f32) -> Self {
        self.bounds =
            Rect2D::from_origin_size(self.bounds.min, Vec2::new(self.bounds.width(), height));
        self
    }

    /// Return the menu bar bounds.
    pub fn bounds_val(&self) -> Rect2D {
        self.bounds
    }

    /// Draw the menu bar background, border, begin a row layout, and position the cursor.
    ///
    /// After calling this, add left-aligned menu items via `menu_bar_dropdown()`.
    /// When ready for right-aligned content, call `right_side()`.
    /// When finished, call `end()`.
    pub fn show(&self, ui: &mut UiContext) {
        ui.draw_rect(self.bounds, ui.style.menu_bg);

        ui.draw_line(
            Vec2::new(self.bounds.min.x(), self.bounds.max.y()),
            Vec2::new(self.bounds.max.x(), self.bounds.max.y()),
            ui.style.separator,
            1.0,
        );

        ui.set_cursor(self.bounds.min);
        ui.begin_row();
    }

    /// Move the cursor to the right side of the menu bar for right-aligned content.
    ///
    /// Call this after adding left-side menu items. Subsequent draw calls will
    /// position content near the right edge of the bar. The `padding` parameter
    /// controls how far from the right edge the cursor is placed.
    pub fn right_side(&self, ui: &mut UiContext, padding: f32) {
        let right_x = (self.bounds.max.x() - padding).max(self.bounds.min.x());
        ui.set_cursor(Vec2::new(right_x, self.bounds.min.y()));
    }

    /// End the menu bar row layout.
    ///
    /// Must be called after `show()` when all menu items and right-side content
    /// have been drawn.
    pub fn end(&self, ui: &mut UiContext) {
        ui.end_row();
    }
}

// =============================================================================
// StatusBar Widget
// =============================================================================

/// A status bar widget drawn at the bottom (or top) of the screen.
///
/// Draws a background rect with a top border line and positions the cursor
/// for subsequent `status_label` / `status_separator` calls.
///
/// # Example
///
/// ```ignore
/// use katla_ui::widgets::StatusBar;
///
/// let bar = StatusBar::new(screen_size.x(), 24.0, screen_size.y() - 24.0);
/// bar.show(ui);
/// ui.status_label("FPS: 60", fps_color);
/// ui.status_separator();
/// ui.status_label("Frame: 1234", text_color);
/// ```
pub struct StatusBar {
    bounds: Rect2D,
}

impl StatusBar {
    /// Create a new status bar spanning `width` with the given `height` at `y_position`.
    pub fn new(width: f32, height: f32, y_position: f32) -> Self {
        Self {
            bounds: Rect2D::from_origin_size(Vec2::new(0.0, y_position), Vec2::new(width, height)),
        }
    }

    /// Override the bounds entirely.
    pub fn bounds(mut self, bounds: Rect2D) -> Self {
        self.bounds = bounds;
        self
    }

    /// Override the height while keeping position and width.
    pub fn height(mut self, height: f32) -> Self {
        self.bounds =
            Rect2D::from_origin_size(self.bounds.min, Vec2::new(self.bounds.width(), height));
        self
    }

    /// Draw the status bar background and top border, then position the cursor
    /// for left-aligned content items.
    pub fn show(&self, ui: &mut UiContext) {
        ui.draw_line(
            Vec2::new(self.bounds.min.x(), self.bounds.min.y()),
            Vec2::new(self.bounds.max.x(), self.bounds.min.y()),
            ui.style.separator,
            1.0,
        );
        ui.draw_rect(self.bounds, ui.style.window_bg);

        let padding = ui.style.window_padding;
        ui.set_cursor(Vec2::new(
            self.bounds.min.x() + padding,
            self.bounds.min.y() + (self.bounds.height() - ui.style.font_size) * 0.5,
        ));
    }

    /// Return the status bar bounds.
    pub fn bounds_val(&self) -> Rect2D {
        self.bounds
    }
}
