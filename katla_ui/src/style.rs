//! UI styling and theming.
//!
//! This module provides styling options for UI widgets.

use katla_math::Color;

/// Predefined font sizes in points.
///
/// Points are converted to pixels using the standard 96 DPI ratio: 1pt = 4/3 px
/// These sizes are designed to work well together in a UI hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontSize {
    /// Extra small - 7.5pt (10px) - badges, tiny labels
    XSmall,
    /// Small - 8.25pt (11px) - status bar, secondary text, tabs
    Small,
    /// Medium - 9pt (12px) - body text, labels (default)
    Medium,
    /// Large - 10.5pt (14px) - headings, emphasis, panel titles
    Large,
    /// Extra large - 12pt (16px) - window titles, important headings
    XLarge,
    /// Extra extra large - 15pt (20px) - large headings
    XXLarge,
    /// Huge - 18pt (24px) - hero text, big numbers
    Huge,
}

impl FontSize {
    /// Convert font size to pixels at 96 DPI.
    ///
    /// Formula: pixels = points * 4/3 (since 72pt = 96px at 96 DPI)
    #[inline]
    pub fn to_pixels(self) -> f32 {
        self.to_points() * (4.0 / 3.0)
    }

    /// Convert font size to pixels with a scale multiplier.
    ///
    /// Use this for accessibility/UI scaling.
    #[inline]
    pub fn to_pixels_scaled(self, scale: f32) -> f32 {
        self.to_pixels() * scale
    }

    /// Get font size in points.
    #[inline]
    pub fn to_points(self) -> f32 {
        match self {
            FontSize::XSmall => 7.5,
            FontSize::Small => 8.25,
            FontSize::Medium => 9.0,
            FontSize::Large => 10.5,
            FontSize::XLarge => 12.0,
            FontSize::XXLarge => 15.0,
            FontSize::Huge => 18.0,
        }
    }

    /// Get the default font size (Medium).
    #[inline]
    pub fn default_size() -> Self {
        FontSize::Medium
    }
}

impl Default for FontSize {
    fn default() -> Self {
        Self::default_size()
    }
}

impl From<FontSize> for f32 {
    fn from(size: FontSize) -> f32 {
        size.to_pixels()
    }
}

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
    /// Default font size (deprecated - use FontSize enum).
    pub font_size: f32,

    /// Background color for checkboxes.
    pub checkbox_bg: Color,
    /// Check mark color.
    pub checkbox_check: Color,
    /// Checkbox border color.
    pub checkbox_border: Color,

    /// Slider track color.
    pub slider_track: Color,
    /// Slider grab color (normal state).
    pub slider_grab: Color,
    /// Slider grab color when hovered.
    pub slider_grab_hovered: Color,
    /// Slider grab color when active (dragging).
    pub slider_grab_active: Color,

    /// Separator line color.
    pub separator: Color,
    /// Border color for general elements.
    pub border: Color,

    // Menu styling
    /// Background color for menus.
    pub menu_bg: Color,
    /// Menu item color when hovered.
    pub menu_hovered: Color,
    /// Menu item color when active/pressed.
    pub menu_active: Color,
    /// Menu border color.
    pub menu_border: Color,
    /// Rounding radius for menu corners.
    pub menu_rounding: f32,
    /// Height of each menu item.
    pub menu_item_height: f32,
    /// Padding inside menus.
    pub menu_padding: f32,
    /// Minimum width for menus.
    pub menu_min_width: f32,

    // Popup styling
    /// Background color for popups.
    pub popup_bg: Color,
    /// Border color for popups.
    pub popup_border: Color,
    /// Shadow color for popups (drawn behind).
    pub popup_shadow: Color,
    /// Rounding radius for popup corners.
    pub popup_rounding: f32,

    // Selectable styling
    /// Selectable item background when hovered.
    pub selectable_hovered: Color,
    /// Selectable item background when selected.
    pub selectable_selected: Color,

    // Combo box styling
    /// Combo box background color.
    pub combo_bg: Color,
    /// Combo box border color.
    pub combo_border: Color,
    /// Combo box button color when hovered.
    pub combo_hovered: Color,
    /// Combo box preview text color.
    pub combo_text: Color,

    // Text input limits
    /// Maximum characters for single-line text input.
    pub text_input_max_length: usize,
    /// Maximum characters for multi-line text area.
    pub text_area_max_length: usize,

    // Spacing
    /// Default spacing between items.
    pub item_spacing: f32,
    /// Inner spacing within items.
    pub item_inner_spacing: f32,
    /// Indent for nested items.
    pub indent_spacing: f32,

    // Widget dimensions
    /// Slider track height.
    pub slider_track_height: f32,
    /// Slider grab handle size.
    pub slider_grab_size: f32,
    /// Default checkbox size.
    pub checkbox_size: f32,
    /// Text input cursor width.
    pub text_input_cursor_width: f32,
    /// Text input padding.
    pub text_input_padding: f32,
    /// Panel padding.
    pub panel_padding: f32,
    /// Window title bar height.
    pub title_bar_height: f32,
    /// Graph label area height.
    pub graph_label_height: f32,
    /// Graph padding.
    pub graph_padding: f32,
    /// Separator height.
    pub separator_height: f32,
    /// Tooltip padding.
    pub tooltip_padding: f32,

    // Button heights
    /// Small button height (compact UI).
    pub button_height_small: f32,
    /// Medium button height (standard).
    pub button_height_medium: f32,
    /// Toolbar height.
    pub toolbar_height: f32,

    // Icon sizes
    /// Small icon size (12px) - navigation, inline icons.
    pub icon_size_small: f32,
    /// Medium icon size (16px) - standard icons.
    pub icon_size_medium: f32,
    /// Large icon size (28px) - asset icons, emphasis.
    pub icon_size_large: f32,

    // Asset browser
    /// Thumbnail size for asset grid items.
    pub thumbnail_size: f32,
}

impl UiStyle {
    /// Create a dark theme style.
    pub fn dark() -> Self {
        Self {
            window_bg: Color::from_rgb_hex(0x2a2a2a), // Slightly brighter for visibility
            window_title_bg: Color::from_rgb_hex(0x3a3a3a),
            window_title_bg_active: Color::from_rgb_hex(0x4a4a4a),
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
            font_size: FontSize::Medium.to_pixels(),

            checkbox_bg: Color::from_rgb_hex(0x282828),
            checkbox_check: Color::from_rgb_hex(0x4a9eff),
            checkbox_border: Color::from_rgb_hex(0x404040),

            slider_track: Color::from_rgb_hex(0x404040),
            slider_grab: Color::from_rgb_hex(0x4a9eff),
            slider_grab_hovered: Color::from_rgb_hex(0x5aa5ff),
            slider_grab_active: Color::from_rgb_hex(0x6ab0ff),

            separator: Color::from_rgb_hex(0x404040),
            border: Color::from_rgb_hex(0x404040),

            // Menu styling (dark theme)
            menu_bg: Color::from_rgb_hex(0x2d2d2d),
            menu_hovered: Color::from_rgb_hex(0x404040),
            menu_active: Color::from_rgb_hex(0x4a9eff),
            menu_border: Color::from_rgb_hex(0x404040),
            menu_rounding: 4.0,
            menu_item_height: 24.0,
            menu_padding: 4.0,
            menu_min_width: 120.0,

            // Popup styling (dark theme)
            popup_bg: Color::from_rgb_hex(0x2d2d2d),
            popup_border: Color::from_rgb_hex(0x404040),
            popup_shadow: Color::new(0.0, 0.0, 0.0, 0.5),
            popup_rounding: 4.0,

            // Selectable styling (dark theme)
            selectable_hovered: Color::from_rgb_hex(0x404040),
            selectable_selected: Color::new(0.29, 0.62, 1.0, 0.4),

            // Combo styling (dark theme)
            combo_bg: Color::from_rgb_hex(0x282828),
            combo_border: Color::from_rgb_hex(0x404040),
            combo_hovered: Color::from_rgb_hex(0x404040),
            combo_text: Color::from_rgb_hex(0xeeeeee),

            text_input_max_length: 256,
            text_area_max_length: 4096,

            item_spacing: 8.0,
            item_inner_spacing: 4.0,
            indent_spacing: 20.0,

            // Widget dimensions (dark theme)
            slider_track_height: 4.0,
            slider_grab_size: 12.0,
            checkbox_size: 20.0,
            text_input_cursor_width: 1.0,
            text_input_padding: 4.0,
            panel_padding: 8.0,
            title_bar_height: 25.0,
            graph_label_height: 18.0,
            graph_padding: 3.0,
            separator_height: 8.0,
            tooltip_padding: 4.0,

            // Button heights (dark theme)
            button_height_small: 24.0,
            button_height_medium: 28.0,
            toolbar_height: 32.0,

            // Icon sizes (dark theme)
            icon_size_small: 12.0,
            icon_size_medium: 16.0,
            icon_size_large: 28.0,

            // Asset browser (dark theme)
            thumbnail_size: 64.0,
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
            font_size: FontSize::Medium.to_pixels(),

            checkbox_bg: Color::from_rgb_hex(0xffffff),
            checkbox_check: Color::from_rgb_hex(0x2070d0),
            checkbox_border: Color::from_rgb_hex(0xc0c0c0),

            slider_track: Color::from_rgb_hex(0xc0c0c0),
            slider_grab: Color::from_rgb_hex(0x2070d0),
            slider_grab_hovered: Color::from_rgb_hex(0x2880e0),
            slider_grab_active: Color::from_rgb_hex(0x3090f0),

            separator: Color::from_rgb_hex(0xc0c0c0),
            border: Color::from_rgb_hex(0xc0c0c0),

            // Menu styling (light theme)
            menu_bg: Color::from_rgb_hex(0xfafafa),
            menu_hovered: Color::from_rgb_hex(0xe0e0e0),
            menu_active: Color::from_rgb_hex(0x2070d0),
            menu_border: Color::from_rgb_hex(0xc0c0c0),
            menu_rounding: 4.0,
            menu_item_height: 24.0,
            menu_padding: 4.0,
            menu_min_width: 120.0,

            // Popup styling (light theme)
            popup_bg: Color::from_rgb_hex(0xfafafa),
            popup_border: Color::from_rgb_hex(0xc0c0c0),
            popup_shadow: Color::new(0.0, 0.0, 0.0, 0.2),
            popup_rounding: 4.0,

            // Selectable styling (light theme)
            selectable_hovered: Color::from_rgb_hex(0xe0e0e0),
            selectable_selected: Color::new(0.13, 0.44, 0.82, 0.3),

            // Combo styling (light theme)
            combo_bg: Color::from_rgb_hex(0xffffff),
            combo_border: Color::from_rgb_hex(0xc0c0c0),
            combo_hovered: Color::from_rgb_hex(0xe0e0e0),
            combo_text: Color::from_rgb_hex(0x222222),

            text_input_max_length: 256,
            text_area_max_length: 4096,

            item_spacing: 8.0,
            item_inner_spacing: 4.0,
            indent_spacing: 20.0,

            // Widget dimensions (light theme)
            slider_track_height: 4.0,
            slider_grab_size: 12.0,
            checkbox_size: 20.0,
            text_input_cursor_width: 1.0,
            text_input_padding: 4.0,
            panel_padding: 8.0,
            title_bar_height: 25.0,
            graph_label_height: 18.0,
            graph_padding: 3.0,
            separator_height: 8.0,
            tooltip_padding: 4.0,

            // Button heights (light theme)
            button_height_small: 24.0,
            button_height_medium: 28.0,
            toolbar_height: 32.0,

            // Icon sizes (light theme)
            icon_size_small: 12.0,
            icon_size_medium: 16.0,
            icon_size_large: 28.0,

            // Asset browser (light theme)
            thumbnail_size: 64.0,
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
            font_size: FontSize::Small.to_pixels(),

            checkbox_bg: Color::from_rgb_hex(0x3a3a3a),
            checkbox_check: Color::from_rgb_hex(0x4a9eff),
            checkbox_border: Color::from_rgb_hex(0x555555),

            slider_track: Color::from_rgb_hex(0x3a3a3a),
            slider_grab: Color::from_rgb_hex(0x4a9eff),
            slider_grab_hovered: Color::from_rgb_hex(0x5aa5ff),
            slider_grab_active: Color::from_rgb_hex(0x6ab0ff),

            separator: Color::from_rgb_hex(0x555555),
            border: Color::from_rgb_hex(0x555555),

            // Menu styling (classic theme)
            menu_bg: Color::from_rgb_hex(0x1f1f1f),
            menu_hovered: Color::from_rgb_hex(0x4a4a4a),
            menu_active: Color::from_rgb_hex(0x3465a4),
            menu_border: Color::from_rgb_hex(0x555555),
            menu_rounding: 0.0,
            menu_item_height: 22.0,
            menu_padding: 2.0,
            menu_min_width: 100.0,

            // Popup styling (classic theme)
            popup_bg: Color::from_rgb_hex(0x1f1f1f),
            popup_border: Color::from_rgb_hex(0x555555),
            popup_shadow: Color::new(0.0, 0.0, 0.0, 0.6),
            popup_rounding: 0.0,

            // Selectable styling (classic theme)
            selectable_hovered: Color::from_rgb_hex(0x4a4a4a),
            selectable_selected: Color::new(0.2, 0.4, 0.64, 0.5),

            // Combo styling (classic theme)
            combo_bg: Color::from_rgb_hex(0x3a3a3a),
            combo_border: Color::from_rgb_hex(0x555555),
            combo_hovered: Color::from_rgb_hex(0x4a4a4a),
            combo_text: Color::from_rgb_hex(0xeeeeee),

            text_input_max_length: 256,
            text_area_max_length: 4096,

            item_spacing: 6.0,
            item_inner_spacing: 3.0,
            indent_spacing: 18.0,

            // Widget dimensions (classic theme)
            slider_track_height: 4.0,
            slider_grab_size: 12.0,
            checkbox_size: 18.0,
            text_input_cursor_width: 1.0,
            text_input_padding: 4.0,
            panel_padding: 6.0,
            title_bar_height: 22.0,
            graph_label_height: 16.0,
            graph_padding: 3.0,
            separator_height: 6.0,
            tooltip_padding: 4.0,

            // Button heights (classic theme)
            button_height_small: 22.0,
            button_height_medium: 26.0,
            toolbar_height: 28.0,

            // Icon sizes (classic theme)
            icon_size_small: 12.0,
            icon_size_medium: 16.0,
            icon_size_large: 26.0,

            // Asset browser (classic theme)
            thumbnail_size: 64.0,
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
