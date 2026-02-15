//! UI styling and theming.
//!
//! This module provides styling options for UI widgets.

use katla_math::Color;

/// Style configuration for UI widgets.
#[derive(Debug, Clone)]
pub struct UiStyle {
    // Colors
    /// Background color for windows.
    pub window_bg: Color,
    /// Background color for window title bars.
    pub window_title_bg: Color,
    /// Background color for window title bars when active.
    pub window_title_bg_active: Color,
    /// Text color in window titles.
    pub window_title_text: Color,
    /// Border color for windows.
    pub window_border: Color,
    /// Rounding radius for window corners.
    pub window_rounding: f32,
    /// Padding inside windows.
    pub window_padding: f32,

    /// Default button color.
    pub button_normal: Color,
    /// Button color when hovered.
    pub button_hovered: Color,
    /// Button color when pressed/active.
    pub button_active: Color,
    /// Text color for buttons.
    pub button_text: Color,
    /// Rounding radius for button corners.
    pub button_rounding: f32,

    /// Background color for input fields.
    pub input_bg: Color,
    /// Border color for input fields.
    pub input_border: Color,
    /// Text color for input fields.
    pub input_text: Color,
    /// Cursor color for text input.
    pub input_cursor: Color,
    /// Selection background color.
    pub input_selection: Color,
    /// Rounding radius for input corners.
    pub input_rounding: f32,

    /// Default text color.
    pub text_color: Color,
    /// Disabled text color.
    pub text_disabled: Color,
    /// Default font size.
    pub font_size: f32,

    /// Background color for checkboxes.
    pub checkbox_bg: Color,
    /// Check mark color.
    pub checkbox_check: Color,
    /// Checkbox border color.
    pub checkbox_border: Color,

    /// Slider track color.
    pub slider_track: Color,
    /// Slider grab color.
    pub slider_grab: Color,
    /// Slider grab color when active.
    pub slider_grab_active: Color,

    /// Separator line color.
    pub separator: Color,
    /// Border color for general elements.
    pub border: Color,

    // Spacing
    /// Default spacing between items.
    pub item_spacing: f32,
    /// Inner spacing within items.
    pub item_inner_spacing: f32,
    /// Indent for nested items.
    pub indent_spacing: f32,
}

impl UiStyle {
    /// Create a dark theme style.
    pub fn dark() -> Self {
        Self {
            window_bg: Color::from_rgb_hex(0x1e1e1e),
            window_title_bg: Color::from_rgb_hex(0x2d2d2d),
            window_title_bg_active: Color::from_rgb_hex(0x3d3d3d),
            window_title_text: Color::from_rgb_hex(0xeeeeee),
            window_border: Color::from_rgb_hex(0x404040),
            window_rounding: 4.0,
            window_padding: 8.0,

            button_normal: Color::from_rgb_hex(0x404040),
            button_hovered: Color::from_rgb_hex(0x505050),
            button_active: Color::from_rgb_hex(0x606060),
            button_text: Color::from_rgb_hex(0xeeeeee),
            button_rounding: 4.0,

            input_bg: Color::from_rgb_hex(0x282828),
            input_border: Color::from_rgb_hex(0x404040),
            input_text: Color::from_rgb_hex(0xeeeeee),
            input_cursor: Color::from_rgb_hex(0xffffff),
            input_selection: Color::new(0.3, 0.5, 0.8, 0.5),
            input_rounding: 2.0,

            text_color: Color::from_rgb_hex(0xeeeeee),
            text_disabled: Color::from_rgb_hex(0x808080),
            font_size: 16.0,

            checkbox_bg: Color::from_rgb_hex(0x282828),
            checkbox_check: Color::from_rgb_hex(0x4a9eff),
            checkbox_border: Color::from_rgb_hex(0x404040),

            slider_track: Color::from_rgb_hex(0x404040),
            slider_grab: Color::from_rgb_hex(0x4a9eff),
            slider_grab_active: Color::from_rgb_hex(0x6ab0ff),

            separator: Color::from_rgb_hex(0x404040),
            border: Color::from_rgb_hex(0x404040),

            item_spacing: 8.0,
            item_inner_spacing: 4.0,
            indent_spacing: 20.0,
        }
    }

    /// Create a light theme style.
    pub fn light() -> Self {
        Self {
            window_bg: Color::from_rgb_hex(0xf0f0f0),
            window_title_bg: Color::from_rgb_hex(0xe0e0e0),
            window_title_bg_active: Color::from_rgb_hex(0xd0d0d0),
            window_title_text: Color::from_rgb_hex(0x222222),
            window_border: Color::from_rgb_hex(0xc0c0c0),
            window_rounding: 4.0,
            window_padding: 8.0,

            button_normal: Color::from_rgb_hex(0xe0e0e0),
            button_hovered: Color::from_rgb_hex(0xd0d0d0),
            button_active: Color::from_rgb_hex(0xc0c0c0),
            button_text: Color::from_rgb_hex(0x222222),
            button_rounding: 4.0,

            input_bg: Color::from_rgb_hex(0xffffff),
            input_border: Color::from_rgb_hex(0xc0c0c0),
            input_text: Color::from_rgb_hex(0x222222),
            input_cursor: Color::from_rgb_hex(0x222222),
            input_selection: Color::new(0.3, 0.5, 0.8, 0.3),
            input_rounding: 2.0,

            text_color: Color::from_rgb_hex(0x222222),
            text_disabled: Color::from_rgb_hex(0x808080),
            font_size: 16.0,

            checkbox_bg: Color::from_rgb_hex(0xffffff),
            checkbox_check: Color::from_rgb_hex(0x2070d0),
            checkbox_border: Color::from_rgb_hex(0xc0c0c0),

            slider_track: Color::from_rgb_hex(0xc0c0c0),
            slider_grab: Color::from_rgb_hex(0x2070d0),
            slider_grab_active: Color::from_rgb_hex(0x3090f0),

            separator: Color::from_rgb_hex(0xc0c0c0),
            border: Color::from_rgb_hex(0xc0c0c0),

            item_spacing: 8.0,
            item_inner_spacing: 4.0,
            indent_spacing: 20.0,
        }
    }
}

impl Default for UiStyle {
    fn default() -> Self {
        Self::dark()
    }
}

/// Color theme preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiTheme {
    /// Dark theme.
    Dark,
    /// Light theme.
    Light,
    /// Classic imgui-style theme.
    Classic,
}

impl UiTheme {
    /// Get the style for this theme.
    pub fn style(&self) -> UiStyle {
        match self {
            UiTheme::Dark => UiStyle::dark(),
            UiTheme::Light => UiStyle::light(),
            UiTheme::Classic => UiStyle::classic(),
        }
    }
}

impl UiStyle {
    /// Create a classic imgui-style theme.
    pub fn classic() -> Self {
        Self {
            window_bg: Color::from_rgb_hex(0x2b2b2b),
            window_title_bg: Color::from_rgb_hex(0x1f1f1f),
            window_title_bg_active: Color::from_rgb_hex(0x3465a4),
            window_title_text: Color::from_rgb_hex(0xeeeeee),
            window_border: Color::from_rgb_hex(0x555555),
            window_rounding: 0.0,
            window_padding: 6.0,

            button_normal: Color::from_rgb_hex(0x4a4a4a),
            button_hovered: Color::from_rgb_hex(0x5a5a5a),
            button_active: Color::from_rgb_hex(0x6a6a6a),
            button_text: Color::from_rgb_hex(0xeeeeee),
            button_rounding: 0.0,

            input_bg: Color::from_rgb_hex(0x3a3a3a),
            input_border: Color::from_rgb_hex(0x555555),
            input_text: Color::from_rgb_hex(0xeeeeee),
            input_cursor: Color::from_rgb_hex(0xffffff),
            input_selection: Color::new(0.4, 0.6, 0.9, 0.4),
            input_rounding: 0.0,

            text_color: Color::from_rgb_hex(0xeeeeee),
            text_disabled: Color::from_rgb_hex(0x777777),
            font_size: 13.0,

            checkbox_bg: Color::from_rgb_hex(0x3a3a3a),
            checkbox_check: Color::from_rgb_hex(0x4a9eff),
            checkbox_border: Color::from_rgb_hex(0x555555),

            slider_track: Color::from_rgb_hex(0x3a3a3a),
            slider_grab: Color::from_rgb_hex(0x4a9eff),
            slider_grab_active: Color::from_rgb_hex(0x6ab0ff),

            separator: Color::from_rgb_hex(0x555555),
            border: Color::from_rgb_hex(0x555555),

            item_spacing: 6.0,
            item_inner_spacing: 3.0,
            indent_spacing: 18.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dark_theme() {
        let style = UiStyle::dark();
        assert!(style.window_bg.r < 0.5); // Dark background
        assert!(style.text_color.r > 0.5); // Light text
    }

    #[test]
    fn test_light_theme() {
        let style = UiStyle::light();
        assert!(style.window_bg.r > 0.5); // Light background
        assert!(style.text_color.r < 0.5); // Dark text
    }

    #[test]
    fn test_theme_to_style() {
        let style = UiTheme::Dark.style();
        assert!(style.window_bg.r < 0.5);
    }
}
