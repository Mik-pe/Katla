//! Editor theme system with multiple color schemes.
//!
//! Provides semantic color names for UI elements, making it easy to
//! swap between different visual themes.

use katla_math::Color;

/// Editor color theme with semantic color names.
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: &'static str,
    /// Background colors
    pub background: Color,
    pub background_light: Color,
    pub background_dark: Color,
    /// Panel/Window colors
    pub panel_bg: Color,
    pub panel_header: Color,
    pub panel_border: Color,
    /// Text colors
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub text_accent: Color,
    /// Interactive elements
    pub button_bg: Color,
    pub button_hover: Color,
    pub button_active: Color,
    pub button_text: Color,
    /// Selection/highlight
    pub selection: Color,
    pub selection_hover: Color,
    pub highlight: Color,
    /// Separator/lines
    pub separator: Color,
    pub border: Color,
    /// Entity type colors (for hierarchy badges)
    pub entity_mesh: Color,
    pub entity_particle: Color,
    pub entity_light: Color,
    pub entity_empty: Color,
    /// Status colors
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub info: Color,
    /// Viewport
    pub viewport_border: Color,
    /// Popup/Dropdown colors
    pub popup_bg: Color,
    pub popup_border: Color,
    pub popup_shadow: Color,
}

/// Macro to create a theme from hex color values.
/// This significantly reduces boilerplate for theme definitions.
macro_rules! theme {
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
        Theme {
            name: $name,
            background: Color::from_rgb_hex($bg),
            background_light: Color::from_rgb_hex($bg_light),
            background_dark: Color::from_rgb_hex($bg_dark),
            panel_bg: Color::from_rgb_hex($panel_bg),
            panel_header: Color::from_rgb_hex($panel_header),
            panel_border: Color::from_rgb_hex($panel_border),
            text_primary: Color::from_rgb_hex($text_primary),
            text_secondary: Color::from_rgb_hex($text_secondary),
            text_muted: Color::from_rgb_hex($text_muted),
            text_accent: Color::from_rgb_hex($text_accent),
            button_bg: Color::from_rgb_hex($button_bg),
            button_hover: Color::from_rgb_hex($button_hover),
            button_active: Color::from_rgb_hex($button_active),
            button_text: Color::from_rgb_hex($button_text),
            selection: Color::from_rgb_hex($selection),
            selection_hover: Color::from_rgb_hex($selection_hover),
            highlight: Color::from_rgb_hex($highlight),
            separator: Color::from_rgb_hex($separator),
            border: Color::from_rgb_hex($border),
            entity_mesh: Color::from_rgb_hex($mesh),
            entity_particle: Color::from_rgb_hex($particle),
            entity_light: Color::from_rgb_hex($light),
            entity_empty: Color::from_rgb_hex($empty),
            success: Color::from_rgb_hex($success),
            warning: Color::from_rgb_hex($warning),
            error: Color::from_rgb_hex($error),
            info: Color::from_rgb_hex($info),
            viewport_border: Color::from_rgb_hex($viewport_border),
            popup_bg: Color::from_rgb_hex($popup_bg),
            popup_border: Color::from_rgb_hex($popup_border),
            popup_shadow: Color::new(0.0, 0.0, 0.0, 0.5),
        }
    };
}

impl Theme {
    /// Get all available theme names (keys for preferences).
    pub fn all_names() -> &'static [&'static str] {
        &[
            "catppuccin",
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

    /// Get a theme by name key.
    pub fn by_name(name: &str) -> Option<Theme> {
        match name {
            "catppuccin" => Some(Self::catppuccin()),
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

    /// Catppuccin Mocha - Soothing pastel theme for the high-spirited!
    pub fn catppuccin() -> Theme {
        theme!(
            name: "Catppuccin Mocha",
            bg: 0x1E1E2E, 0x313244, 0x181825,
            panel: 0x1E1E2E, 0x313244, 0x45475A,
            text: 0xC9CBFF, 0xBABCF2, 0x6C7086, 0xA6DA95,
            button: 0x313244, 0x45475A, 0x89B4FA, 0xC9CBFF,
            selection: 0x89B4FA, 0x74C7EC, 0xF5C2E7,
            misc: 0x45475A, 0x585B70,
            entity: 0xA6DA95, 0xFAB387, 0xF9E2AF, 0x6C7086,
            status: 0xA6DA95, 0xF9E2AF, 0xF38BA8, 0x89D9EB,
            viewport: 0x89B4FA,
            popup: 0x313244, 0x45475A,
        )
    }

    /// Nord - An arctic, north-bluish color palette
    pub fn nord() -> Theme {
        theme!(
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

    /// Tokyo Night - A clean, dark Neovim theme
    pub fn tokyo_night() -> Theme {
        theme!(
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

    /// Dracula - A dark theme for many editors
    pub fn dracula() -> Theme {
        theme!(
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

    /// Gruvbox - Retro groove color scheme
    pub fn gruvbox() -> Theme {
        theme!(
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

    /// One Dark - Atom's iconic dark theme
    pub fn one_dark() -> Theme {
        theme!(
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

    /// Material Palenight - Material Design inspired dark theme
    pub fn material_palenight() -> Theme {
        theme!(
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

    /// Ayu Dark - A simple and clean dark theme
    pub fn ayu_dark() -> Theme {
        theme!(
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

    /// GitHub Dark - GitHub's official dark theme
    pub fn github_dark() -> Theme {
        theme!(
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

    /// Monokai - Classic Monokai color scheme
    pub fn monokai() -> Theme {
        theme!(
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

    /// Rose Pine - Soho vibes for editor
    pub fn rose_pine() -> Theme {
        theme!(
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

    /// Kanagawa - A colorscheme inspired by The Great Wave off Kanagawa
    pub fn kanagawa() -> Theme {
        theme!(
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

    /// Solarized Dark - Precision color scheme
    pub fn solarized_dark() -> Theme {
        theme!(
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

    /// Apply theme colors to UI style.
    pub fn apply_to_style(&self, style: &mut katla_ui::UiStyle) {
        // Panel/window colors
        style.window_bg = self.panel_bg;
        style.window_title_bg = self.panel_header;
        style.window_title_bg_active = self.panel_header;
        style.window_title_text = self.text_primary;
        style.window_border = self.panel_border;

        // Button colors
        style.button_normal = self.button_bg;
        style.button_hovered = self.button_hover;
        style.button_active = self.button_active;
        style.button_text = self.button_text;

        // Text colors
        style.text_color = self.text_primary;
        style.text_disabled = self.text_muted;

        // Selection/highlight
        style.selectable_hovered = self.selection_hover;
        style.selectable_selected = self.selection;

        // Separator/lines
        style.separator = self.separator;
        style.border = self.border;

        // Menu colors
        style.menu_bg = self.popup_bg;
        style.menu_hovered = self.selection_hover;
        style.menu_active = self.selection;
        style.menu_border = self.popup_border;

        // Popup colors
        style.popup_bg = self.popup_bg;
        style.popup_border = self.popup_border;
        style.popup_shadow = self.popup_shadow;

        // Combo box colors
        style.combo_bg = self.button_bg;
        style.combo_hovered = self.button_hover;
        style.combo_border = self.border;
        style.combo_text = self.text_primary;

        // Input colors
        style.input_bg = self.panel_bg;
        style.input_border = self.border;
        style.input_text = self.text_primary;
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::catppuccin()
    }
}
