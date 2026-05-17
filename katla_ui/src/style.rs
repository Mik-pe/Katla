//! UI styling and theming.
//!
//! This module provides styling options for UI widgets.

use katla_math::Color;

/// Color scheme for UI theming.
///
/// Holds all color-related fields from [`UiStyle`], allowing themes to be defined
/// independently from dimensions and spacing. Use with [`UiStyle::with_colors`] to
/// build a complete style.
#[derive(Debug, Clone)]
pub struct ColorScheme {
    /// Human-readable display name (e.g. "Catppuccin Mocha").
    pub name: &'static str,

    pub window_bg: Color,
    pub window_title_bg: Color,
    pub window_title_bg_active: Color,
    pub window_title_text: Color,
    pub window_border: Color,

    pub button_normal: Color,
    pub button_hovered: Color,
    pub button_active: Color,
    pub button_text: Color,

    pub input_bg: Color,
    pub input_border: Color,
    pub input_text: Color,
    pub input_cursor: Color,
    pub input_border_focused: Color,
    pub input_selection: Color,

    pub text_color: Color,
    pub text_disabled: Color,
    pub text_hint: Color,

    pub checkbox_bg: Color,
    pub checkbox_check: Color,
    pub checkbox_border: Color,

    pub slider_track: Color,
    pub slider_grab: Color,
    pub slider_grab_hovered: Color,
    pub slider_grab_active: Color,

    pub separator: Color,
    pub border: Color,

    pub menu_bg: Color,
    pub menu_hovered: Color,
    pub menu_active: Color,

    pub popup_bg: Color,
    pub popup_border: Color,
    pub popup_shadow: Color,

    pub selectable_hovered: Color,
    pub selectable_selected: Color,

    pub combo_bg: Color,
    pub combo_border: Color,
    pub combo_hovered: Color,

    pub scrollbar_track: Color,
    pub scrollbar_handle: Color,
    pub scrollbar_handle_hovered: Color,

    /// Focus ring color drawn around the focused widget during Tab navigation.
    pub focus_ring_color: Color,

    // Editor-specific semantic colors
    /// Status: success (green).
    pub success: Color,
    /// Status: warning (yellow/amber).
    pub warning: Color,
    /// Status: error (red).
    pub error: Color,
    /// Status: info (blue/cyan).
    pub info: Color,

    /// Entity type color for mesh entities.
    pub entity_mesh: Color,
    /// Entity type color for light entities.
    pub entity_light: Color,
    /// Entity type color for particle entities.
    pub entity_particle: Color,
    /// Entity type color for empty entities.
    pub entity_empty: Color,

    /// Accent color for emphasized elements.
    pub accent: Color,
    /// Highlight color for focused/important elements.
    pub highlight: Color,

    /// Selection background color.
    pub selection: Color,
    /// Selection background when hovered.
    pub selection_hover: Color,

    /// Viewport border color.
    pub viewport_border: Color,

    /// Base background color.
    pub background: Color,
    /// Darker background variant.
    pub background_dark: Color,
    /// Lighter background variant.
    pub background_light: Color,

    /// Primary text color (high contrast).
    pub text_primary: Color,
    /// Secondary text color (medium contrast).
    pub text_secondary: Color,
    /// Muted text color (low contrast).
    pub text_muted: Color,
    /// Accent text color (for labels, material names).
    pub text_accent: Color,

    /// Panel background color.
    pub panel_bg: Color,
    /// Panel border color.
    pub panel_border: Color,
    /// Panel header background color.
    pub panel_header: Color,

    /// Button background (alias for button_normal, used by editor panels).
    pub button_bg: Color,
    /// Button hover (alias for button_hovered, used by editor panels).
    pub button_hover: Color,
}

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
    /// Scrollbar width.
    pub scrollbar_width: f32,

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

    // Widget default sizes
    /// Default button width.
    pub button_default_width: f32,
    /// Default button height.
    pub button_default_height: f32,
    /// Default icon button size (square).
    pub icon_button_size: f32,
    /// Default checkbox width.
    pub checkbox_default_width: f32,
    /// Default checkbox height.
    pub checkbox_default_height: f32,
    /// Default slider width.
    pub slider_default_width: f32,
    /// Default slider height.
    pub slider_default_height: f32,
    /// Default text input width.
    pub text_input_default_width: f32,
    /// Default text input height.
    pub text_input_default_height: f32,
    /// Default label width.
    pub label_default_width: f32,
    /// Default label height.
    pub label_default_height: f32,
    /// Default radio button width.
    pub radio_button_default_width: f32,
    /// Default radio button height.
    pub radio_button_default_height: f32,
    /// Default progress bar width.
    pub progress_bar_default_width: f32,
    /// Default progress bar height.
    pub progress_bar_default_height: f32,
    /// Default collapsible header width.
    pub collapsible_default_width: f32,
    /// Default collapsible header height.
    pub collapsible_default_height: f32,
    /// Default badge width.
    pub badge_default_width: f32,
    /// Default badge height.
    pub badge_default_height: f32,
    /// Default combo box width.
    pub combo_default_width: f32,
    /// Default combo box height.
    pub combo_default_height: f32,

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

    /// Focus ring color drawn around the focused widget during Tab navigation.
    pub focus_ring_color: Color,

    // Widget state colors
    /// Background color for hovered widgets.
    pub widget_hovered_bg: Color,
    /// Background color for active/focused widgets.
    pub widget_active_bg: Color,
    /// Background color for pressed widgets.
    pub widget_pressed_bg: Color,
    /// Width of the focus ring drawn around focused widgets.
    pub focus_ring_width: f32,

    // Menu colors
    /// Background color for hovered menu items.
    pub menu_item_hover_bg: Color,
    /// Color for check marks in checkboxes and menus.
    pub check_mark_color: Color,

    // Tab bar colors
    /// Height of the tab bar.
    pub tab_bar_height: f32,
    /// Background color for inactive tabs.
    pub tab_inactive_bg: Color,
    /// Background color for the active tab.
    pub tab_active_bg: Color,
    /// Background color for hovered tabs.
    pub tab_hover_bg: Color,
    /// Text color for inactive tabs.
    pub tab_text: Color,
    /// Text color for the active tab.
    pub tab_active_text: Color,
    /// Border color for tabs.
    pub tab_border: Color,
}

impl ColorScheme {
    /// Returns the color scheme for the dark theme.
    fn dark() -> Self {
        Self {
            name: "Dark",
            window_bg: Color::from_rgb_hex(0x2a2a2a),
            window_title_bg: Color::from_rgb_hex(0x3a3a3a),
            window_title_bg_active: Color::from_rgb_hex(0x4a4a4a),
            window_title_text: Color::from_rgb_hex(0xeeeeee),
            window_border: Color::from_rgb_hex(0x404040),

            button_normal: Color::from_rgb_hex(0x404040),
            button_hovered: Color::from_rgb_hex(0x505050),
            button_active: Color::from_rgb_hex(0x353535),
            button_text: Color::from_rgb_hex(0xeeeeee),

            input_bg: Color::from_rgb_hex(0x282828),
            input_border: Color::from_rgb_hex(0x404040),
            input_text: Color::from_rgb_hex(0xeeeeee),
            input_cursor: Color::from_rgb_hex(0xffffff),
            input_border_focused: Color::from_rgb_hex(0x4a9eff),
            input_selection: Color::new(0.29, 0.62, 1.0, 0.35),

            text_color: Color::from_rgb_hex(0xeeeeee),
            text_disabled: Color::from_rgb_hex(0x808080),
            text_hint: Color::from_rgb_hex(0x808080),

            checkbox_bg: Color::from_rgb_hex(0x282828),
            checkbox_check: Color::from_rgb_hex(0x4a9eff),
            checkbox_border: Color::from_rgb_hex(0x404040),

            slider_track: Color::from_rgb_hex(0x404040),
            slider_grab: Color::from_rgb_hex(0x4a9eff),
            slider_grab_hovered: Color::from_rgb_hex(0x5aa5ff),
            slider_grab_active: Color::from_rgb_hex(0x6ab0ff),

            separator: Color::from_rgb_hex(0x404040),
            border: Color::from_rgb_hex(0x404040),

            menu_bg: Color::from_rgb_hex(0x2d2d2d),
            menu_hovered: Color::from_rgb_hex(0x404040),
            menu_active: Color::from_rgb_hex(0x4a9eff),

            popup_bg: Color::from_rgb_hex(0x2d2d2d),
            popup_border: Color::from_rgb_hex(0x404040),
            popup_shadow: Color::new(0.0, 0.0, 0.0, 0.5),

            selectable_hovered: Color::from_rgb_hex(0x404040),
            selectable_selected: Color::new(0.29, 0.62, 1.0, 0.4),

            combo_bg: Color::from_rgb_hex(0x282828),
            combo_border: Color::from_rgb_hex(0x404040),
            combo_hovered: Color::from_rgb_hex(0x404040),

            scrollbar_track: Color::from_rgb_hex(0x1a1a1a),
            scrollbar_handle: Color::from_rgb_hex(0x505050),
            scrollbar_handle_hovered: Color::from_rgb_hex(0x606060),

            focus_ring_color: Color::from_rgb_hex(0x4a9eff),

            success: Color::from_rgb_hex(0xa6da95),
            warning: Color::from_rgb_hex(0xf9e2af),
            error: Color::from_rgb_hex(0xf38ba8),
            info: Color::from_rgb_hex(0x89d9eb),

            entity_mesh: Color::from_rgb_hex(0xa6da95),
            entity_light: Color::from_rgb_hex(0xf9e2af),
            entity_particle: Color::from_rgb_hex(0xfab387),
            entity_empty: Color::from_rgb_hex(0x6c7086),

            accent: Color::from_rgb_hex(0xa6da95),
            highlight: Color::from_rgb_hex(0xf5c2e7),

            selection: Color::new(0.29, 0.62, 1.0, 0.4),
            selection_hover: Color::from_rgb_hex(0x505050),

            viewport_border: Color::from_rgb_hex(0x4a9eff),

            background: Color::from_rgb_hex(0x2a2a2a),
            background_dark: Color::from_rgb_hex(0x181825),
            background_light: Color::from_rgb_hex(0x313244),

            text_primary: Color::from_rgb_hex(0xcdd6f4),
            text_secondary: Color::from_rgb_hex(0xbac2de),
            text_muted: Color::from_rgb_hex(0x6c7086),
            text_accent: Color::from_rgb_hex(0xa6da95),

            panel_bg: Color::from_rgb_hex(0x1e1e2e),
            panel_border: Color::from_rgb_hex(0x45475a),
            panel_header: Color::from_rgb_hex(0x313244),

            button_bg: Color::from_rgb_hex(0x404040),
            button_hover: Color::from_rgb_hex(0x505050),
        }
    }

    /// Returns the color scheme for the light theme.
    fn light() -> Self {
        Self {
            name: "Light",
            window_bg: Color::from_rgb_hex(0xf0f0f0),
            window_title_bg: Color::from_rgb_hex(0xe0e0e0),
            window_title_bg_active: Color::from_rgb_hex(0xd0d0d0),
            window_title_text: Color::from_rgb_hex(0x222222),
            window_border: Color::from_rgb_hex(0xc0c0c0),

            button_normal: Color::from_rgb_hex(0xe0e0e0),
            button_hovered: Color::from_rgb_hex(0xd0d0d0),
            button_active: Color::from_rgb_hex(0xc0c0c0),
            button_text: Color::from_rgb_hex(0x222222),

            input_bg: Color::from_rgb_hex(0xffffff),
            input_border: Color::from_rgb_hex(0xc0c0c0),
            input_text: Color::from_rgb_hex(0x222222),
            input_cursor: Color::from_rgb_hex(0x222222),
            input_border_focused: Color::from_rgb_hex(0x4a9eff),
            input_selection: Color::new(0.13, 0.44, 0.82, 0.3),

            text_color: Color::from_rgb_hex(0x222222),
            text_disabled: Color::from_rgb_hex(0x808080),
            text_hint: Color::from_rgb_hex(0x808080),

            checkbox_bg: Color::from_rgb_hex(0xffffff),
            checkbox_check: Color::from_rgb_hex(0x2070d0),
            checkbox_border: Color::from_rgb_hex(0xc0c0c0),

            slider_track: Color::from_rgb_hex(0xc0c0c0),
            slider_grab: Color::from_rgb_hex(0x2070d0),
            slider_grab_hovered: Color::from_rgb_hex(0x2880e0),
            slider_grab_active: Color::from_rgb_hex(0x3090f0),

            separator: Color::from_rgb_hex(0xc0c0c0),
            border: Color::from_rgb_hex(0xc0c0c0),

            menu_bg: Color::from_rgb_hex(0xfafafa),
            menu_hovered: Color::from_rgb_hex(0xe0e0e0),
            menu_active: Color::from_rgb_hex(0x2070d0),

            popup_bg: Color::from_rgb_hex(0xfafafa),
            popup_border: Color::from_rgb_hex(0xc0c0c0),
            popup_shadow: Color::new(0.0, 0.0, 0.0, 0.2),

            selectable_hovered: Color::from_rgb_hex(0xe0e0e0),
            selectable_selected: Color::new(0.13, 0.44, 0.82, 0.3),

            combo_bg: Color::from_rgb_hex(0xffffff),
            combo_border: Color::from_rgb_hex(0xc0c0c0),
            combo_hovered: Color::from_rgb_hex(0xe0e0e0),

            scrollbar_track: Color::from_rgb_hex(0xe0e0e0),
            scrollbar_handle: Color::from_rgb_hex(0xa0a0a0),
            scrollbar_handle_hovered: Color::from_rgb_hex(0x808080),

            focus_ring_color: Color::from_rgb_hex(0x4a9eff),

            success: Color::from_rgb_hex(0x40a02b),
            warning: Color::from_rgb_hex(0xdf8e1d),
            error: Color::from_rgb_hex(0xd20f39),
            info: Color::from_rgb_hex(0x4a9eff),

            entity_mesh: Color::from_rgb_hex(0x40a02b),
            entity_light: Color::from_rgb_hex(0xdf8e1d),
            entity_particle: Color::from_rgb_hex(0xfe640b),
            entity_empty: Color::from_rgb_hex(0x9ca0b0),

            accent: Color::from_rgb_hex(0x40a02b),
            highlight: Color::from_rgb_hex(0xea76cb),

            selection: Color::new(0.13, 0.44, 0.82, 0.3),
            selection_hover: Color::from_rgb_hex(0xd0d0d0),

            viewport_border: Color::from_rgb_hex(0x4a9eff),

            background: Color::from_rgb_hex(0xf0f0f0),
            background_dark: Color::from_rgb_hex(0xe0e0e0),
            background_light: Color::from_rgb_hex(0xe8e8e8),

            text_primary: Color::from_rgb_hex(0x222222),
            text_secondary: Color::from_rgb_hex(0x555555),
            text_muted: Color::from_rgb_hex(0x888888),
            text_accent: Color::from_rgb_hex(0x40a02b),

            panel_bg: Color::from_rgb_hex(0xf0f0f0),
            panel_border: Color::from_rgb_hex(0xc0c0c0),
            panel_header: Color::from_rgb_hex(0xe0e0e0),

            button_bg: Color::from_rgb_hex(0xe0e0e0),
            button_hover: Color::from_rgb_hex(0xd0d0d0),
        }
    }

    /// Returns the color scheme for the classic theme.
    fn classic() -> Self {
        Self {
            name: "Classic",
            window_bg: Color::from_rgb_hex(0x2b2b2b),
            window_title_bg: Color::from_rgb_hex(0x1f1f1f),
            window_title_bg_active: Color::from_rgb_hex(0x3465a4),
            window_title_text: Color::from_rgb_hex(0xeeeeee),
            window_border: Color::from_rgb_hex(0x555555),

            button_normal: Color::from_rgb_hex(0x4a4a4a),
            button_hovered: Color::from_rgb_hex(0x5a5a5a),
            button_active: Color::from_rgb_hex(0x6a6a6a),
            button_text: Color::from_rgb_hex(0xeeeeee),

            input_bg: Color::from_rgb_hex(0x3a3a3a),
            input_border: Color::from_rgb_hex(0x555555),
            input_text: Color::from_rgb_hex(0xeeeeee),
            input_cursor: Color::from_rgb_hex(0xffffff),
            input_border_focused: Color::from_rgb_hex(0x4a9eff),
            input_selection: Color::new(0.29, 0.62, 1.0, 0.35),

            text_color: Color::from_rgb_hex(0xeeeeee),
            text_disabled: Color::from_rgb_hex(0x777777),
            text_hint: Color::from_rgb_hex(0x777777),

            checkbox_bg: Color::from_rgb_hex(0x3a3a3a),
            checkbox_check: Color::from_rgb_hex(0x4a9eff),
            checkbox_border: Color::from_rgb_hex(0x555555),

            slider_track: Color::from_rgb_hex(0x3a3a3a),
            slider_grab: Color::from_rgb_hex(0x4a9eff),
            slider_grab_hovered: Color::from_rgb_hex(0x5aa5ff),
            slider_grab_active: Color::from_rgb_hex(0x6ab0ff),

            separator: Color::from_rgb_hex(0x555555),
            border: Color::from_rgb_hex(0x555555),

            menu_bg: Color::from_rgb_hex(0x1f1f1f),
            menu_hovered: Color::from_rgb_hex(0x4a4a4a),
            menu_active: Color::from_rgb_hex(0x3465a4),

            popup_bg: Color::from_rgb_hex(0x1f1f1f),
            popup_border: Color::from_rgb_hex(0x555555),
            popup_shadow: Color::new(0.0, 0.0, 0.0, 0.6),

            selectable_hovered: Color::from_rgb_hex(0x4a4a4a),
            selectable_selected: Color::new(0.2, 0.4, 0.64, 0.5),

            combo_bg: Color::from_rgb_hex(0x3a3a3a),
            combo_border: Color::from_rgb_hex(0x555555),
            combo_hovered: Color::from_rgb_hex(0x4a4a4a),

            scrollbar_track: Color::from_rgb_hex(0x1f1f1f),
            scrollbar_handle: Color::from_rgb_hex(0x555555),
            scrollbar_handle_hovered: Color::from_rgb_hex(0x666666),

            focus_ring_color: Color::from_rgb_hex(0x4a9eff),

            success: Color::from_rgb_hex(0x4ec9b0),
            warning: Color::from_rgb_hex(0xdcdcaa),
            error: Color::from_rgb_hex(0xf44747),
            info: Color::from_rgb_hex(0x569cd6),

            entity_mesh: Color::from_rgb_hex(0x4ec9b0),
            entity_light: Color::from_rgb_hex(0xdcdcaa),
            entity_particle: Color::from_rgb_hex(0xce9178),
            entity_empty: Color::from_rgb_hex(0x6a9955),

            accent: Color::from_rgb_hex(0x4ec9b0),
            highlight: Color::from_rgb_hex(0x569cd6),

            selection: Color::new(0.2, 0.4, 0.64, 0.5),
            selection_hover: Color::from_rgb_hex(0x4a4a4a),

            viewport_border: Color::from_rgb_hex(0x3465a4),

            background: Color::from_rgb_hex(0x2b2b2b),
            background_dark: Color::from_rgb_hex(0x1a1a1a),
            background_light: Color::from_rgb_hex(0x3a3a3a),

            text_primary: Color::from_rgb_hex(0xeeeeee),
            text_secondary: Color::from_rgb_hex(0xbbbbbb),
            text_muted: Color::from_rgb_hex(0x777777),
            text_accent: Color::from_rgb_hex(0x4ec9b0),

            panel_bg: Color::from_rgb_hex(0x2b2b2b),
            panel_border: Color::from_rgb_hex(0x555555),
            panel_header: Color::from_rgb_hex(0x1f1f1f),

            button_bg: Color::from_rgb_hex(0x4a4a4a),
            button_hover: Color::from_rgb_hex(0x5a5a5a),
        }
    }
}

macro_rules! color_scheme {
    (
            name: $name:expr,
            bg: $bg:expr, $bg_light:expr, $bg_dark:expr,
            panel: $panel_bg:expr, $panel_header:expr, $panel_border:expr,
            text: $text_primary:expr, $text_secondary:expr, $text_muted:expr, $text_accent:expr,
            button: $button_bg:expr, $button_hover:expr, $button_active:expr, $button_text:expr,
            selection: $selection:expr, $selection_hover:expr, $highlight:expr,
            misc: $separator:expr, $border:expr,
            entity: $mesh:expr, $particle:expr, $light:expr, $empty:expr,
            status: $success:expr, $warning:expr, $error:expr, $info:expr,
            viewport: $viewport_border:expr,
            popup: $popup_bg:expr, $popup_border:expr,
        ) => {
        ColorScheme {
            name: $name,
            window_bg: Color::from_rgb_hex($panel_bg),
            window_title_bg: Color::from_rgb_hex($panel_header),
            window_title_bg_active: Color::from_rgb_hex($panel_header),
            window_title_text: Color::from_rgb_hex($text_primary),
            window_border: Color::from_rgb_hex($panel_border),

            button_normal: Color::from_rgb_hex($button_bg),
            button_hovered: Color::from_rgb_hex($button_hover),
            button_active: Color::from_rgb_hex($button_active),
            button_text: Color::from_rgb_hex($button_text),

            input_bg: Color::from_rgb_hex($panel_bg),
            input_border: Color::from_rgb_hex($border),
            input_text: Color::from_rgb_hex($text_primary),
            input_cursor: Color::from_rgb_hex($text_primary),
            input_border_focused: Color::from_rgb_hex($highlight),
            input_selection: Color::new(0.29, 0.62, 1.0, 0.35),

            text_color: Color::from_rgb_hex($text_primary),
            text_disabled: Color::from_rgb_hex($text_muted),
            text_hint: Color::from_rgb_hex($text_muted),

            checkbox_bg: Color::from_rgb_hex($panel_bg),
            checkbox_check: Color::from_rgb_hex($selection),
            checkbox_border: Color::from_rgb_hex($border),

            slider_track: Color::from_rgb_hex($border),
            slider_grab: Color::from_rgb_hex($selection),
            slider_grab_hovered: Color::from_rgb_hex($selection_hover),
            slider_grab_active: Color::from_rgb_hex($selection),

            separator: Color::from_rgb_hex($separator),
            border: Color::from_rgb_hex($border),

            menu_bg: Color::from_rgb_hex($popup_bg),
            menu_hovered: Color::from_rgb_hex($selection_hover),
            menu_active: Color::from_rgb_hex($selection),

            popup_bg: Color::from_rgb_hex($popup_bg),
            popup_border: Color::from_rgb_hex($popup_border),
            popup_shadow: Color::new(0.0, 0.0, 0.0, 0.5),

            selectable_hovered: Color::from_rgb_hex($selection_hover),
            selectable_selected: Color::from_rgb_hex($selection),

            combo_bg: Color::from_rgb_hex($button_bg),
            combo_border: Color::from_rgb_hex($border),
            combo_hovered: Color::from_rgb_hex($button_hover),

            scrollbar_track: Color::from_rgb_hex($bg_dark),
            scrollbar_handle: Color::from_rgb_hex($border),
            scrollbar_handle_hovered: Color::from_rgb_hex($panel_border),

            focus_ring_color: Color::from_rgb_hex($selection),

            success: Color::from_rgb_hex($success),
            warning: Color::from_rgb_hex($warning),
            error: Color::from_rgb_hex($error),
            info: Color::from_rgb_hex($info),

            entity_mesh: Color::from_rgb_hex($mesh),
            entity_light: Color::from_rgb_hex($light),
            entity_particle: Color::from_rgb_hex($particle),
            entity_empty: Color::from_rgb_hex($empty),

            accent: Color::from_rgb_hex($text_accent),
            highlight: Color::from_rgb_hex($highlight),

            selection: Color::from_rgb_hex($selection),
            selection_hover: Color::from_rgb_hex($selection_hover),

            viewport_border: Color::from_rgb_hex($viewport_border),

            background: Color::from_rgb_hex($bg),
            background_dark: Color::from_rgb_hex($bg_dark),
            background_light: Color::from_rgb_hex($bg_light),

            text_primary: Color::from_rgb_hex($text_primary),
            text_secondary: Color::from_rgb_hex($text_secondary),
            text_muted: Color::from_rgb_hex($text_muted),
            text_accent: Color::from_rgb_hex($text_accent),

            panel_bg: Color::from_rgb_hex($panel_bg),
            panel_border: Color::from_rgb_hex($panel_border),
            panel_header: Color::from_rgb_hex($panel_header),

            button_bg: Color::from_rgb_hex($button_bg),
            button_hover: Color::from_rgb_hex($button_hover),
        }
    };
}

impl ColorScheme {
    pub fn default_theme() -> Self {
        color_scheme!(
            name: "Default",
            bg: 0x1E1E2E, 0x313244, 0x181825,
            panel: 0x1E1E2E, 0x313244, 0x45475A,
            text: 0xC9CBFF, 0xBABCF2, 0x6C7086, 0xA6DA95,
            button: 0x313244, 0x45475A, 0x3B3B52, 0xC9CBFF,
            selection: 0x89B4FA, 0xA8C8FF, 0xF5C2E7,
            misc: 0x45475A, 0x585B70,
            entity: 0xA6DA95, 0xFAB387, 0xF9E2AF, 0x6C7086,
            status: 0xA6DA95, 0xF9E2AF, 0xF38BA8, 0x89D9EB,
            viewport: 0x89B4FA,
            popup: 0x313244, 0x45475A,
        )
    }

    pub fn nord() -> Self {
        color_scheme!(
            name: "Nord",
            bg: 0x2E3440, 0x3B4252, 0x242933,
            panel: 0x2E3440, 0x3B4252, 0x4C566A,
            text: 0xECEFF4, 0xE5E9F0, 0xD8DEE9, 0xA3BE8C,
            button: 0x3B4252, 0x434C5E, 0x81A1C1, 0xECEFF4,
            selection: 0x81A1C1, 0x88C0D0, 0xB48EAD,
            misc: 0x3B4252, 0x4C566A,
            entity: 0xA3BE8C, 0xD08770, 0xEBCB8B, 0x4C566A,
            status: 0xA3BE8C, 0xEBCB8B, 0xBF616A, 0x88C0D0,
            viewport: 0x81A1C1,
            popup: 0x3B4252, 0x4C566A,
        )
    }

    pub fn tokyo_night() -> Self {
        color_scheme!(
            name: "Tokyo Night",
            bg: 0x1A1B26, 0x242533, 0x161721,
            panel: 0x1A1B26, 0x242533, 0x3B3E4D,
            text: 0xC0CAF5, 0xA9B1D6, 0x565F89, 0x9ECE6A,
            button: 0x242533, 0x3B3E4D, 0x7AA2F7, 0xC0CAF5,
            selection: 0x364A8E, 0x3E59A6, 0xBB9AF7,
            misc: 0x3B3E4D, 0x3B3E4D,
            entity: 0x9ECE6A, 0xFF9E64, 0xE0AF68, 0x565F89,
            status: 0x9ECE6A, 0xE0AF68, 0xF7768E, 0x7DCFEF,
            viewport: 0x7AA2F7,
            popup: 0x242533, 0x3B3E4D,
        )
    }

    pub fn dracula() -> Self {
        color_scheme!(
            name: "Dracula",
            bg: 0x282A36, 0x44475A, 0x21222E,
            panel: 0x282A36, 0x343748, 0x44475A,
            text: 0xF8F8F2, 0xE5E5E5, 0x6272A4, 0x50FA7B,
            button: 0x44475A, 0x52576C, 0xBD93F9, 0xF8F8F2,
            selection: 0xBD93F9, 0xCFA6FC, 0xFF79C6,
            misc: 0x44475A, 0x52576C,
            entity: 0x50FA7B, 0xFFB86C, 0xF1FA8C, 0x6272A4,
            status: 0x50FA7B, 0xF1FA8C, 0xFF5555, 0x8BE9FD,
            viewport: 0xBD93F9,
            popup: 0x44475A, 0x6272A4,
        )
    }

    pub fn gruvbox() -> Self {
        color_scheme!(
            name: "Gruvbox Dark",
            bg: 0x282828, 0x3C3836, 0x1D2021,
            panel: 0x282828, 0x3C3836, 0x504A45,
            text: 0xEBDBB2, 0xD5C4A1, 0x928374, 0xB8BB26,
            button: 0x3C3836, 0x504A45, 0xD79921, 0xEBDBB2,
            selection: 0xD79921, 0xFABD2F, 0xFE8019,
            misc: 0x3C3836, 0x504A45,
            entity: 0xB8BB26, 0xFE8019, 0xFABD2F, 0x928374,
            status: 0xB8BB26, 0xFABD2F, 0xFB4934, 0x83A598,
            viewport: 0xD79921,
            popup: 0x3C3836, 0x504A45,
        )
    }

    pub fn one_dark() -> Self {
        color_scheme!(
            name: "One Dark",
            bg: 0x282C34, 0x3E4451, 0x21252B,
            panel: 0x282C34, 0x3E4451, 0x4B5263,
            text: 0xABB2BF, 0x9DA5B4, 0x5C6370, 0x98C379,
            button: 0x3E4451, 0x4B5263, 0x61AFEF, 0xABB2BF,
            selection: 0x3E4451, 0x4B5263, 0xC678DD,
            misc: 0x3E4451, 0x4B5263,
            entity: 0x98C379, 0xD19A66, 0xE5C07B, 0x5C6370,
            status: 0x98C379, 0xE5C07B, 0xE06C75, 0x61AFEF,
            viewport: 0x61AFEF,
            popup: 0x3E4451, 0x4B5263,
        )
    }

    pub fn material_palenight() -> Self {
        color_scheme!(
            name: "Material Palenight",
            bg: 0x292D3E, 0x3A3F5B, 0x1E2133,
            panel: 0x292D3E, 0x3A3F5B, 0x414763,
            text: 0xA6ACCD, 0x8A93B5, 0x676E95, 0xC3E88D,
            button: 0x3A3F5B, 0x414763, 0x82AAFF, 0xA6ACCD,
            selection: 0x676E95, 0x7A819D, 0xC792EA,
            misc: 0x3A3F5B, 0x414763,
            entity: 0xC3E88D, 0xF78C6C, 0xFFCB6B, 0x676E95,
            status: 0xC3E88D, 0xFFCB6B, 0xFF5370, 0x82AAFF,
            viewport: 0x82AAFF,
            popup: 0x3A3F5B, 0x414763,
        )
    }

    pub fn ayu_dark() -> Self {
        color_scheme!(
            name: "Ayu Dark",
            bg: 0x0D1017, 0x1A1F29, 0x070A0F,
            panel: 0x0D1017, 0x1A1F29, 0x2D3440,
            text: 0xBFBDB6, 0xA8A49D, 0x5C6773, 0xBED9F5,
            button: 0x1A1F29, 0x2D3440, 0x39BAE6, 0xBFBDB6,
            selection: 0x1A1F29, 0x2D3440, 0xF07178,
            misc: 0x1A1F29, 0x2D3440,
            entity: 0x7FD962, 0xFF9940, 0xFFB454, 0x5C6773,
            status: 0x7FD962, 0xFFB454, 0xF07178, 0x39BAE6,
            viewport: 0x39BAE6,
            popup: 0x1A1F29, 0x2D3440,
        )
    }

    pub fn github_dark() -> Self {
        color_scheme!(
            name: "GitHub Dark",
            bg: 0x0D1117, 0x161B22, 0x010409,
            panel: 0x0D1117, 0x161B22, 0x30363D,
            text: 0xE6EDF3, 0xC9D1D9, 0x7D8590, 0x3FB950,
            button: 0x21262D, 0x30363D, 0x1F6FEB, 0xE6EDF3,
            selection: 0x1F6FEB, 0x388BFD, 0xF778BA,
            misc: 0x21262D, 0x30363D,
            entity: 0x3FB950, 0xDB6D28, 0xD29922, 0x7D8590,
            status: 0x3FB950, 0xD29922, 0xF85149, 0x58A6FF,
            viewport: 0x30363D,
            popup: 0x161B22, 0x30363D,
        )
    }

    pub fn monokai() -> Self {
        color_scheme!(
            name: "Monokai",
            bg: 0x272822, 0x3E3D32, 0x1E1F1C,
            panel: 0x272822, 0x3E3D32, 0x49483E,
            text: 0xF8F8F2, 0xCFCFC2, 0x75715E, 0xA6E22E,
            button: 0x3E3D32, 0x49483E, 0x66D9EF, 0xF8F8F2,
            selection: 0x49483E, 0x5A5950, 0xFD971F,
            misc: 0x3E3D32, 0x49483E,
            entity: 0xA6E22E, 0xFD971F, 0xE6DB74, 0x75715E,
            status: 0xA6E22E, 0xE6DB74, 0xF92672, 0x66D9EF,
            viewport: 0x66D9EF,
            popup: 0x3E3D32, 0x49483E,
        )
    }

    pub fn rose_pine() -> Self {
        color_scheme!(
            name: "Rosé Pine",
            bg: 0x191724, 0x1F1D2E, 0x13111B,
            panel: 0x191724, 0x1F1D2E, 0x26233A,
            text: 0xE0DEF4, 0xC9C8D3, 0x6E6A86, 0x9CCFD8,
            button: 0x1F1D2E, 0x26233A, 0xC4A7E7, 0xE0DEF4,
            selection: 0x403D52, 0x524F67, 0xEBBCBA,
            misc: 0x1F1D2E, 0x26233A,
            entity: 0x9CCFD8, 0xF6C177, 0xEBBCBA, 0x6E6A86,
            status: 0x9CCFD8, 0xF6C177, 0xEB6F92, 0x31748F,
            viewport: 0xC4A7E7,
            popup: 0x1F1D2E, 0x26233A,
        )
    }

    pub fn kanagawa() -> Self {
        color_scheme!(
            name: "Kanagawa",
            bg: 0x1F1F28, 0x2A2A3C, 0x16161D,
            panel: 0x1F1F28, 0x2A2A3C, 0x363646,
            text: 0xDCD7BA, 0xC8C093, 0x727169, 0x76946A,
            button: 0x2A2A3C, 0x363646, 0x7E9CD8, 0xDCD7BA,
            selection: 0x2D4F67, 0x3E5F7A, 0x957FB8,
            misc: 0x2A2A3C, 0x363646,
            entity: 0x76946A, 0xFFA066, 0xDCA561, 0x727169,
            status: 0x76946A, 0xDCA561, 0xC34043, 0x7E9CD8,
            viewport: 0x7E9CD8,
            popup: 0x2A2A3C, 0x363646,
        )
    }

    pub fn solarized_dark() -> Self {
        color_scheme!(
            name: "Solarized Dark",
            bg: 0x002B36, 0x073642, 0x001E26,
            panel: 0x002B36, 0x073642, 0x094959,
            text: 0x839496, 0x657B83, 0x586E75, 0x859900,
            button: 0x073642, 0x094959, 0x268BD2, 0x839496,
            selection: 0x073642, 0x094959, 0xD33682,
            misc: 0x073642, 0x094959,
            entity: 0x859900, 0xCB4B16, 0xB58900, 0x586E75,
            status: 0x859900, 0xB58900, 0xDC322F, 0x268BD2,
            viewport: 0x268BD2,
            popup: 0x073642, 0x094959,
        )
    }

    pub fn rcp() -> Self {
        let mut scheme = color_scheme!(
            name: "Reality Composer Pro",
            bg: 0x1E1E1E, 0x2A2A2E, 0x141414,
            panel: 0x1E1E1E, 0x2A2A2E, 0x38383A,
            text: 0xD9D9D9, 0x8C8C8C, 0x5A5A5A, 0x0A84FF,
            button: 0x3A3A3C, 0x48484A, 0x3D3D52, 0xD9D9D9,
            selection: 0x0A84FF, 0x4DA6FF, 0x0058D0,
            misc: 0x2A2A2E, 0x38383A,
            entity: 0x30D158, 0xFF9F0A, 0xFFD60A, 0x636366,
            status: 0x30D158, 0xFF9F0A, 0xFF453A, 0x64D2FF,
            viewport: 0x38383A,
            popup: 0x2C2C2E, 0x38383A,
        );

        let subtle_border = Color::new(1.0, 1.0, 1.0, 0.08);
        let panel_border = Color::new(1.0, 1.0, 1.0, 0.06);

        scheme.window_border = panel_border;
        scheme.input_border = subtle_border;
        scheme.checkbox_border = subtle_border;
        scheme.combo_border = subtle_border;
        scheme.popup_border = subtle_border;
        scheme.panel_border = panel_border;
        scheme.separator = Color::new(1.0, 1.0, 1.0, 0.06);
        scheme.border = subtle_border;

        scheme
    }
}

impl ColorScheme {
    /// Extract a `ColorScheme` from an existing [`UiStyle`].
    pub fn from_style(style: &UiStyle) -> Self {
        Self {
            name: "From Style",
            window_bg: style.window_bg,
            window_title_bg: style.window_title_bg,
            window_title_bg_active: style.window_title_bg_active,
            window_title_text: style.window_title_text,
            window_border: style.window_border,

            button_normal: style.button_normal,
            button_hovered: style.button_hovered,
            button_active: style.button_active,
            button_text: style.button_text,

            input_bg: style.input_bg,
            input_border: style.input_border,
            input_text: style.input_text,
            input_cursor: style.input_cursor,
            input_border_focused: style.input_border_focused,
            input_selection: style.input_selection,

            text_color: style.text_color,
            text_disabled: style.text_disabled,
            text_hint: style.text_hint,

            checkbox_bg: style.checkbox_bg,
            checkbox_check: style.checkbox_check,
            checkbox_border: style.checkbox_border,

            slider_track: style.slider_track,
            slider_grab: style.slider_grab,
            slider_grab_hovered: style.slider_grab_hovered,
            slider_grab_active: style.slider_grab_active,

            separator: style.separator,
            border: style.border,

            menu_bg: style.menu_bg,
            menu_hovered: style.menu_hovered,
            menu_active: style.menu_active,

            popup_bg: style.popup_bg,
            popup_border: style.popup_border,
            popup_shadow: style.popup_shadow,

            selectable_hovered: style.selectable_hovered,
            selectable_selected: style.selectable_selected,

            combo_bg: style.combo_bg,
            combo_border: style.combo_border,
            combo_hovered: style.combo_hovered,

            scrollbar_track: style.scrollbar_track,
            scrollbar_handle: style.scrollbar_handle,
            scrollbar_handle_hovered: style.scrollbar_handle_hovered,

            focus_ring_color: style.focus_ring_color,

            success: Color::from_rgb_hex(0x30D158),
            warning: Color::from_rgb_hex(0xFF9F0A),
            error: Color::from_rgb_hex(0xFF453A),
            info: Color::from_rgb_hex(0x64D2FF),

            entity_mesh: Color::from_rgb_hex(0x30D158),
            entity_light: Color::from_rgb_hex(0xFFD60A),
            entity_particle: Color::from_rgb_hex(0xFF9F0A),
            entity_empty: Color::from_rgb_hex(0x636366),

            accent: Color::from_rgb_hex(0x0A84FF),
            highlight: Color::from_rgb_hex(0x0058D0),

            selection: style.selectable_selected,
            selection_hover: style.selectable_hovered,

            viewport_border: Color::from_rgb_hex(0x38383A),

            background: style.window_bg,
            background_dark: Color::from_rgb_hex(0x141414),
            background_light: Color::from_rgb_hex(0x2A2A2E),

            text_primary: style.text_color,
            text_secondary: style.text_disabled,
            text_muted: style.text_hint,
            text_accent: Color::from_rgb_hex(0x0A84FF),

            panel_bg: style.window_bg,
            panel_border: style.window_border,
            panel_header: style.window_title_bg,

            button_bg: style.button_normal,
            button_hover: style.button_hovered,
        }
    }

    /// Write this color scheme back into a [`UiStyle`], overwriting all color fields.
    pub fn apply_to_style(&self, style: &mut UiStyle) {
        style.apply_colors(self.clone());
    }

    pub fn all_names() -> &'static [&'static str] {
        &[
            "dark",
            "light",
            "classic",
            "rcp",
            "default",
            "nord",
            "tokyo_night",
            "dracula",
            "gruvbox",
            "one_dark",
            "material_palenight",
            "ayu_dark",
            "github_dark",
            "monokai",
            "rose_pine",
            "kanagawa",
            "solarized_dark",
        ]
    }

    pub fn by_name(name: &str) -> Option<ColorScheme> {
        match name {
            "dark" => Some(Self::dark()),
            "light" => Some(Self::light()),
            "classic" => Some(Self::classic()),
            "rcp" => Some(Self::rcp()),
            "default" => Some(Self::default_theme()),
            "catppuccin" => Some(Self::default_theme()),
            "nord" => Some(Self::nord()),
            "tokyo_night" => Some(Self::tokyo_night()),
            "dracula" => Some(Self::dracula()),
            "gruvbox" => Some(Self::gruvbox()),
            "one_dark" => Some(Self::one_dark()),
            "material_palenight" => Some(Self::material_palenight()),
            "ayu_dark" => Some(Self::ayu_dark()),
            "github_dark" => Some(Self::github_dark()),
            "monokai" => Some(Self::monokai()),
            "rose_pine" => Some(Self::rose_pine()),
            "kanagawa" => Some(Self::kanagawa()),
            "solarized_dark" => Some(Self::solarized_dark()),
            _ => None,
        }
    }
}

impl Default for ColorScheme {
    fn default() -> Self {
        Self::rcp()
    }
}

impl UiStyle {
    fn default_dimensions() -> Self {
        Self {
            window_rounding: 6.0,
            window_padding: 10.0,
            button_rounding: 4.0,
            input_rounding: 3.0,
            font_size: FontSize::Medium.to_pixels(),

            text_input_max_length: 256,
            text_area_max_length: 4096,

            menu_rounding: 5.0,
            menu_item_height: 24.0,
            menu_padding: 4.0,
            menu_min_width: 120.0,

            popup_rounding: 6.0,

            item_spacing: 8.0,
            item_inner_spacing: 4.0,
            indent_spacing: 20.0,
            scrollbar_width: 10.0,

            slider_track_height: 3.0,
            slider_grab_size: 12.0,
            checkbox_size: 20.0,
            text_input_cursor_width: 2.0,
            text_input_padding: 4.0,
            panel_padding: 12.0,
            title_bar_height: 25.0,
            graph_label_height: 18.0,
            graph_padding: 3.0,
            separator_height: 4.0,
            tooltip_padding: 4.0,
            property_label_width: 70.0,

            button_default_width: 100.0,
            button_default_height: 30.0,
            icon_button_size: 30.0,
            checkbox_default_width: 150.0,
            checkbox_default_height: 24.0,
            slider_default_width: 150.0,
            slider_default_height: 20.0,
            text_input_default_width: 200.0,
            text_input_default_height: 24.0,
            label_default_width: 100.0,
            label_default_height: 20.0,
            radio_button_default_width: 150.0,
            radio_button_default_height: 20.0,
            progress_bar_default_width: 200.0,
            progress_bar_default_height: 20.0,
            collapsible_default_width: 200.0,
            collapsible_default_height: 24.0,
            badge_default_width: 60.0,
            badge_default_height: 20.0,
            combo_default_width: 150.0,
            combo_default_height: 24.0,

            icon_size_small: 12.0,
            icon_size_medium: 16.0,
            icon_size_large: 28.0,

            thumbnail_size: 64.0,

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
            popup_bg: Color::BLACK,
            popup_border: Color::BLACK,
            popup_shadow: Color::TRANSPARENT,
            selectable_hovered: Color::BLACK,
            selectable_selected: Color::TRANSPARENT,
            combo_bg: Color::BLACK,
            combo_border: Color::BLACK,
            combo_hovered: Color::BLACK,
            scrollbar_track: Color::BLACK,
            scrollbar_handle: Color::BLACK,
            scrollbar_handle_hovered: Color::BLACK,
            focus_ring_color: Color::BLACK,

            widget_hovered_bg: Color::from_rgb_hex(0x48484A),
            widget_active_bg: Color::from_rgb_hex(0x505050),
            widget_pressed_bg: Color::from_rgb_hex(0x2A2A2E),
            focus_ring_width: 2.0,

            menu_item_hover_bg: Color::from_rgb_hex(0x0A84FF),
            check_mark_color: Color::from_rgb_hex(0x0A84FF),

            tab_bar_height: 28.0,
            tab_inactive_bg: Color::from_rgb_hex(0x1E1E1E),
            tab_active_bg: Color::from_rgb_hex(0x2A2A2E),
            tab_hover_bg: Color::from_rgb_hex(0x38383A),
            tab_text: Color::from_rgb_hex(0x8C8C8C),
            tab_active_text: Color::from_rgb_hex(0xD9D9D9),
            tab_border: Color::from_rgb_hex(0x38383A),
        }
    }

    /// Build a style from default dimensions and a color scheme.
    pub fn with_colors(colors: ColorScheme) -> Self {
        let mut s = Self::default_dimensions();
        s.apply_colors(colors);
        s
    }

    /// Apply a [`ColorScheme`] to this style, overwriting all color fields.
    pub fn apply_colors(&mut self, c: ColorScheme) {
        self.window_bg = c.window_bg;
        self.window_title_bg = c.window_title_bg;
        self.window_title_bg_active = c.window_title_bg_active;
        self.window_title_text = c.window_title_text;
        self.window_border = c.window_border;

        self.button_normal = c.button_normal;
        self.button_hovered = c.button_hovered;
        self.button_active = c.button_active;
        self.button_text = c.button_text;

        self.input_bg = c.input_bg;
        self.input_border = c.input_border;
        self.input_text = c.input_text;
        self.input_cursor = c.input_cursor;
        self.input_border_focused = c.input_border_focused;
        self.input_selection = c.input_selection;

        self.text_color = c.text_color;
        self.text_disabled = c.text_disabled;
        self.text_hint = c.text_hint;

        self.checkbox_bg = c.checkbox_bg;
        self.checkbox_check = c.checkbox_check;
        self.checkbox_border = c.checkbox_border;

        self.slider_track = c.slider_track;
        self.slider_grab = c.slider_grab;
        self.slider_grab_hovered = c.slider_grab_hovered;
        self.slider_grab_active = c.slider_grab_active;

        self.separator = c.separator;
        self.border = c.border;

        self.menu_bg = c.menu_bg;
        self.menu_hovered = c.menu_hovered;
        self.menu_active = c.menu_active;

        self.popup_bg = c.popup_bg;
        self.popup_border = c.popup_border;
        self.popup_shadow = c.popup_shadow;

        self.selectable_hovered = c.selectable_hovered;
        self.selectable_selected = c.selectable_selected;

        self.combo_bg = c.combo_bg;
        self.combo_border = c.combo_border;
        self.combo_hovered = c.combo_hovered;

        self.scrollbar_track = c.scrollbar_track;
        self.scrollbar_handle = c.scrollbar_handle;
        self.scrollbar_handle_hovered = c.scrollbar_handle_hovered;
        self.focus_ring_color = c.focus_ring_color;
    }

    /// Create a dark theme style.
    pub fn dark() -> Self {
        Self::with_colors(ColorScheme::dark())
    }

    /// Create a light theme style.
    pub fn light() -> Self {
        Self::with_colors(ColorScheme::light())
    }

    /// Create a classic imgui-style theme.
    pub fn classic() -> Self {
        let mut s = Self::with_colors(ColorScheme::classic());
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
        s.scrollbar_width = 10.0;
        s.checkbox_size = 18.0;
        s.panel_padding = 6.0;
        s.title_bar_height = 22.0;
        s.graph_label_height = 16.0;
        s.separator_height = 6.0;
        s.icon_size_large = 26.0;
        s.button_default_width = 100.0;
        s.button_default_height = 28.0;
        s.checkbox_default_height = 22.0;
        s.slider_default_height = 18.0;
        s.text_input_default_height = 22.0;
        s.radio_button_default_height = 18.0;
        s.collapsible_default_height = 22.0;
        s
    }
}

impl Default for UiStyle {
    fn default() -> Self {
        Self::dark()
    }
}

/// Default widget dimensions.
///
/// Used by widget constructors when no style context is available (e.g., in `new()`).
/// When a `UiContext` is available, prefer reading from `ui.style` instead.
pub const DEFAULTS: WidgetDefaults = WidgetDefaults {
    button_default_width: 100.0,
    button_default_height: 30.0,
    icon_button_size: 30.0,
    checkbox_default_width: 150.0,
    checkbox_default_height: 24.0,
    slider_default_width: 150.0,
    slider_default_height: 20.0,
    text_input_default_width: 200.0,
    text_input_default_height: 24.0,
    label_default_width: 100.0,
    label_default_height: 20.0,
    radio_button_default_width: 150.0,
    radio_button_default_height: 20.0,
    progress_bar_default_width: 200.0,
    progress_bar_default_height: 20.0,
    collapsible_default_width: 200.0,
    collapsible_default_height: 24.0,
    badge_default_width: 60.0,
    badge_default_height: 20.0,
    combo_default_width: 150.0,
    combo_default_height: 24.0,
};

/// Widget default dimensions.
///
/// These values match the defaults in [`UiStyle::dark()`].
pub struct WidgetDefaults {
    pub button_default_width: f32,
    pub button_default_height: f32,
    pub icon_button_size: f32,
    pub checkbox_default_width: f32,
    pub checkbox_default_height: f32,
    pub slider_default_width: f32,
    pub slider_default_height: f32,
    pub text_input_default_width: f32,
    pub text_input_default_height: f32,
    pub label_default_width: f32,
    pub label_default_height: f32,
    pub radio_button_default_width: f32,
    pub radio_button_default_height: f32,
    pub progress_bar_default_width: f32,
    pub progress_bar_default_height: f32,
    pub collapsible_default_width: f32,
    pub collapsible_default_height: f32,
    pub badge_default_width: f32,
    pub badge_default_height: f32,
    pub combo_default_width: f32,
    pub combo_default_height: f32,
}
