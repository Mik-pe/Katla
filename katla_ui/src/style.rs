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
    /// Border color for focused input fields.
    pub input_border_focused: Color,
    /// Selection background color.
    pub input_selection: Color,
    /// Rounding radius for input corners.
    pub input_rounding: f32,

    /// Default text color.
    pub text_color: Color,
    /// Disabled text color.
    pub text_disabled: Color,
    /// Hint text color (for placeholders).
    pub text_hint: Color,
    /// Default font size in pixels.
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
    /// Label column width for property rows.
    pub property_label_width: f32,

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

    // Scrollbar styling
    /// Scrollbar track background color.
    pub scrollbar_track: Color,
    /// Scrollbar handle color.
    pub scrollbar_handle: Color,
    /// Scrollbar handle color when hovered.
    pub scrollbar_handle_hovered: Color,
}

impl UiStyle {
    fn default_dimensions() -> Self {
        Self {
            window_rounding: 4.0,
            window_padding: 8.0,
            button_rounding: 4.0,
            input_rounding: 2.0,
            font_size: FontSize::Medium.to_pixels(),

            text_input_max_length: 256,
            text_area_max_length: 4096,

            menu_rounding: 4.0,
            menu_item_height: 24.0,
            menu_padding: 4.0,
            menu_min_width: 120.0,

            popup_rounding: 4.0,

            item_spacing: 8.0,
            item_inner_spacing: 4.0,
            indent_spacing: 20.0,

            slider_track_height: 4.0,
            slider_grab_size: 12.0,
            checkbox_size: 20.0,
            text_input_cursor_width: 2.0,
            text_input_padding: 4.0,
            panel_padding: 8.0,
            title_bar_height: 25.0,
            graph_label_height: 18.0,
            graph_padding: 3.0,
            separator_height: 8.0,
            tooltip_padding: 4.0,
            property_label_width: 60.0,

            button_height_small: 24.0,
            button_height_medium: 28.0,
            toolbar_height: 32.0,

            icon_size_small: 12.0,
            icon_size_medium: 16.0,
            icon_size_large: 28.0,

            thumbnail_size: 64.0,

            // Placeholder colors — must be overridden by theme functions
            window_bg: Color::BLACK,
            window_title_bg: Color::BLACK,
            window_title_bg_active: Color::BLACK,
            window_title_text: Color::BLACK,
            window_border: Color::BLACK,
            button_normal: Color::BLACK,
            button_hovered: Color::BLACK,
            button_active: Color::BLACK,
            button_text: Color::BLACK,
            input_bg: Color::BLACK,
            input_border: Color::BLACK,
            input_text: Color::BLACK,
            input_cursor: Color::BLACK,
            input_border_focused: Color::BLACK,
            input_selection: Color::TRANSPARENT,
            text_color: Color::BLACK,
            text_disabled: Color::BLACK,
            text_hint: Color::BLACK,
            checkbox_bg: Color::BLACK,
            checkbox_check: Color::BLACK,
            checkbox_border: Color::BLACK,
            slider_track: Color::BLACK,
            slider_grab: Color::BLACK,
            slider_grab_hovered: Color::BLACK,
            slider_grab_active: Color::BLACK,
            separator: Color::BLACK,
            border: Color::BLACK,
            menu_bg: Color::BLACK,
            menu_hovered: Color::BLACK,
            menu_active: Color::BLACK,
            menu_border: Color::BLACK,
            popup_bg: Color::BLACK,
            popup_border: Color::BLACK,
            popup_shadow: Color::TRANSPARENT,
            selectable_hovered: Color::BLACK,
            selectable_selected: Color::TRANSPARENT,
            combo_bg: Color::BLACK,
            combo_border: Color::BLACK,
            combo_hovered: Color::BLACK,
            combo_text: Color::BLACK,
            scrollbar_track: Color::BLACK,
            scrollbar_handle: Color::BLACK,
            scrollbar_handle_hovered: Color::BLACK,
        }
    }

    /// Apply common color defaults shared across all themes.
    ///
    /// Colors that are identical or follow the same pattern in dark/light/classic.
    fn apply_common_colors(&mut self) {
        self.input_border_focused = Color::from_rgb_hex(0x4a9eff);
    }

    /// Create a dark theme style.
    pub fn dark() -> Self {
        let mut s = Self::default_dimensions();
        s.apply_common_colors();
        s.window_bg = Color::from_rgb_hex(0x2a2a2a);
        s.window_title_bg = Color::from_rgb_hex(0x3a3a3a);
        s.window_title_bg_active = Color::from_rgb_hex(0x4a4a4a);
        s.window_title_text = Color::from_rgb_hex(0xeeeeee);
        s.window_border = Color::from_rgb_hex(0x404040);

        s.button_normal = Color::from_rgb_hex(0x404040);
        s.button_hovered = Color::from_rgb_hex(0x505050);
        s.button_active = Color::from_rgb_hex(0x606060);
        s.button_text = Color::from_rgb_hex(0xeeeeee);

        s.input_bg = Color::from_rgb_hex(0x282828);
        s.input_border = Color::from_rgb_hex(0x404040);
        s.input_text = Color::from_rgb_hex(0xeeeeee);
        s.input_cursor = Color::from_rgb_hex(0xffffff);
        s.input_selection = Color::new(0.3, 0.5, 0.8, 0.5);

        s.text_color = Color::from_rgb_hex(0xeeeeee);
        s.text_disabled = Color::from_rgb_hex(0x808080);
        s.text_hint = Color::from_rgb_hex(0x808080);

        s.checkbox_bg = Color::from_rgb_hex(0x282828);
        s.checkbox_check = Color::from_rgb_hex(0x4a9eff);
        s.checkbox_border = Color::from_rgb_hex(0x404040);

        s.slider_track = Color::from_rgb_hex(0x404040);
        s.slider_grab = Color::from_rgb_hex(0x4a9eff);
        s.slider_grab_hovered = Color::from_rgb_hex(0x5aa5ff);
        s.slider_grab_active = Color::from_rgb_hex(0x6ab0ff);

        s.separator = Color::from_rgb_hex(0x404040);
        s.border = Color::from_rgb_hex(0x404040);

        s.menu_bg = Color::from_rgb_hex(0x2d2d2d);
        s.menu_hovered = Color::from_rgb_hex(0x404040);
        s.menu_active = Color::from_rgb_hex(0x4a9eff);
        s.menu_border = Color::from_rgb_hex(0x404040);

        s.popup_bg = Color::from_rgb_hex(0x2d2d2d);
        s.popup_border = Color::from_rgb_hex(0x404040);
        s.popup_shadow = Color::new(0.0, 0.0, 0.0, 0.5);

        s.selectable_hovered = Color::from_rgb_hex(0x404040);
        s.selectable_selected = Color::new(0.29, 0.62, 1.0, 0.4);

        s.combo_bg = Color::from_rgb_hex(0x282828);
        s.combo_border = Color::from_rgb_hex(0x404040);
        s.combo_hovered = Color::from_rgb_hex(0x404040);
        s.combo_text = Color::from_rgb_hex(0xeeeeee);

        s.scrollbar_track = Color::from_rgb_hex(0x1a1a1a);
        s.scrollbar_handle = Color::from_rgb_hex(0x505050);
        s.scrollbar_handle_hovered = Color::from_rgb_hex(0x606060);

        s
    }

    /// Create a light theme style.
    pub fn light() -> Self {
        let mut s = Self::default_dimensions();
        s.apply_common_colors();
        s.window_bg = Color::from_rgb_hex(0xf0f0f0);
        s.window_title_bg = Color::from_rgb_hex(0xe0e0e0);
        s.window_title_bg_active = Color::from_rgb_hex(0xd0d0d0);
        s.window_title_text = Color::from_rgb_hex(0x222222);
        s.window_border = Color::from_rgb_hex(0xc0c0c0);

        s.button_normal = Color::from_rgb_hex(0xe0e0e0);
        s.button_hovered = Color::from_rgb_hex(0xd0d0d0);
        s.button_active = Color::from_rgb_hex(0xc0c0c0);
        s.button_text = Color::from_rgb_hex(0x222222);

        s.input_bg = Color::from_rgb_hex(0xffffff);
        s.input_border = Color::from_rgb_hex(0xc0c0c0);
        s.input_text = Color::from_rgb_hex(0x222222);
        s.input_cursor = Color::from_rgb_hex(0x222222);
        s.input_selection = Color::new(0.3, 0.5, 0.8, 0.3);

        s.text_color = Color::from_rgb_hex(0x222222);
        s.text_disabled = Color::from_rgb_hex(0x808080);
        s.text_hint = Color::from_rgb_hex(0x808080);

        s.checkbox_bg = Color::from_rgb_hex(0xffffff);
        s.checkbox_check = Color::from_rgb_hex(0x2070d0);
        s.checkbox_border = Color::from_rgb_hex(0xc0c0c0);

        s.slider_track = Color::from_rgb_hex(0xc0c0c0);
        s.slider_grab = Color::from_rgb_hex(0x2070d0);
        s.slider_grab_hovered = Color::from_rgb_hex(0x2880e0);
        s.slider_grab_active = Color::from_rgb_hex(0x3090f0);

        s.separator = Color::from_rgb_hex(0xc0c0c0);
        s.border = Color::from_rgb_hex(0xc0c0c0);

        s.menu_bg = Color::from_rgb_hex(0xfafafa);
        s.menu_hovered = Color::from_rgb_hex(0xe0e0e0);
        s.menu_active = Color::from_rgb_hex(0x2070d0);
        s.menu_border = Color::from_rgb_hex(0xc0c0c0);

        s.popup_bg = Color::from_rgb_hex(0xfafafa);
        s.popup_border = Color::from_rgb_hex(0xc0c0c0);
        s.popup_shadow = Color::new(0.0, 0.0, 0.0, 0.2);

        s.selectable_hovered = Color::from_rgb_hex(0xe0e0e0);
        s.selectable_selected = Color::new(0.13, 0.44, 0.82, 0.3);

        s.combo_bg = Color::from_rgb_hex(0xffffff);
        s.combo_border = Color::from_rgb_hex(0xc0c0c0);
        s.combo_hovered = Color::from_rgb_hex(0xe0e0e0);
        s.combo_text = Color::from_rgb_hex(0x222222);

        s.scrollbar_track = Color::from_rgb_hex(0xe0e0e0);
        s.scrollbar_handle = Color::from_rgb_hex(0xa0a0a0);
        s.scrollbar_handle_hovered = Color::from_rgb_hex(0x808080);

        s
    }

    /// Create a classic imgui-style theme.
    pub fn classic() -> Self {
        let mut s = Self::default_dimensions();
        s.apply_common_colors();
        s.window_rounding = 0.0;
        s.window_padding = 6.0;
        s.button_rounding = 0.0;
        s.input_rounding = 0.0;
        s.font_size = FontSize::Small.to_pixels();
        s.menu_rounding = 0.0;
        s.menu_item_height = 22.0;
        s.menu_padding = 2.0;
        s.menu_min_width = 100.0;
        s.popup_rounding = 0.0;
        s.item_spacing = 6.0;
        s.item_inner_spacing = 3.0;
        s.indent_spacing = 18.0;
        s.checkbox_size = 18.0;
        s.panel_padding = 6.0;
        s.title_bar_height = 22.0;
        s.graph_label_height = 16.0;
        s.separator_height = 6.0;
        s.button_height_small = 22.0;
        s.button_height_medium = 26.0;
        s.toolbar_height = 28.0;
        s.icon_size_large = 26.0;

        s.window_bg = Color::from_rgb_hex(0x2b2b2b);
        s.window_title_bg = Color::from_rgb_hex(0x1f1f1f);
        s.window_title_bg_active = Color::from_rgb_hex(0x3465a4);
        s.window_title_text = Color::from_rgb_hex(0xeeeeee);
        s.window_border = Color::from_rgb_hex(0x555555);

        s.button_normal = Color::from_rgb_hex(0x4a4a4a);
        s.button_hovered = Color::from_rgb_hex(0x5a5a5a);
        s.button_active = Color::from_rgb_hex(0x6a6a6a);
        s.button_text = Color::from_rgb_hex(0xeeeeee);

        s.input_bg = Color::from_rgb_hex(0x3a3a3a);
        s.input_border = Color::from_rgb_hex(0x555555);
        s.input_text = Color::from_rgb_hex(0xeeeeee);
        s.input_cursor = Color::from_rgb_hex(0xffffff);
        s.input_selection = Color::new(0.4, 0.6, 0.9, 0.4);

        s.text_color = Color::from_rgb_hex(0xeeeeee);
        s.text_disabled = Color::from_rgb_hex(0x777777);
        s.text_hint = Color::from_rgb_hex(0x777777);

        s.checkbox_bg = Color::from_rgb_hex(0x3a3a3a);
        s.checkbox_check = Color::from_rgb_hex(0x4a9eff);
        s.checkbox_border = Color::from_rgb_hex(0x555555);

        s.slider_track = Color::from_rgb_hex(0x3a3a3a);
        s.slider_grab = Color::from_rgb_hex(0x4a9eff);
        s.slider_grab_hovered = Color::from_rgb_hex(0x5aa5ff);
        s.slider_grab_active = Color::from_rgb_hex(0x6ab0ff);

        s.separator = Color::from_rgb_hex(0x555555);
        s.border = Color::from_rgb_hex(0x555555);

        s.menu_bg = Color::from_rgb_hex(0x1f1f1f);
        s.menu_hovered = Color::from_rgb_hex(0x4a4a4a);
        s.menu_active = Color::from_rgb_hex(0x3465a4);
        s.menu_border = Color::from_rgb_hex(0x555555);

        s.popup_bg = Color::from_rgb_hex(0x1f1f1f);
        s.popup_border = Color::from_rgb_hex(0x555555);
        s.popup_shadow = Color::new(0.0, 0.0, 0.0, 0.6);

        s.selectable_hovered = Color::from_rgb_hex(0x4a4a4a);
        s.selectable_selected = Color::new(0.2, 0.4, 0.64, 0.5);

        s.combo_bg = Color::from_rgb_hex(0x3a3a3a);
        s.combo_border = Color::from_rgb_hex(0x555555);
        s.combo_hovered = Color::from_rgb_hex(0x4a4a4a);
        s.combo_text = Color::from_rgb_hex(0xeeeeee);

        s.scrollbar_track = Color::from_rgb_hex(0x1f1f1f);
        s.scrollbar_handle = Color::from_rgb_hex(0x555555);
        s.scrollbar_handle_hovered = Color::from_rgb_hex(0x666666);

        s
    }
}

impl Default for UiStyle {
    fn default() -> Self {
        Self::dark()
    }
}
