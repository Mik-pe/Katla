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
    /// https://github.com/catppuccin/catppuccin
    pub fn catppuccin() -> Theme {
        Theme {
            name: "Catppuccin Mocha",
            // Base colors
            background: Color::from_rgb_hex(0x1E1E2E),       // Base
            background_light: Color::from_rgb_hex(0x313244), // Surface0
            background_dark: Color::from_rgb_hex(0x181825),  // Mantle
            // Panels
            panel_bg: Color::from_rgb_hex(0x1E1E2E),         // Base
            panel_header: Color::from_rgb_hex(0x313244),     // Surface0
            panel_border: Color::from_rgb_hex(0x45475A),     // Surface1
            // Text
            text_primary: Color::from_rgb_hex(0xC9CBFF),     // Text
            text_secondary: Color::from_rgb_hex(0xBABCF2),   // Subtext1
            text_muted: Color::from_rgb_hex(0x6C7086),       // Overlay0
            text_accent: Color::from_rgb_hex(0xA6DA95),      // Green accent
            // Buttons
            button_bg: Color::from_rgb_hex(0x313244),        // Surface0
            button_hover: Color::from_rgb_hex(0x45475A),     // Surface1
            button_active: Color::from_rgb_hex(0x89B4FA),    // Blue
            button_text: Color::from_rgb_hex(0xC9CBFF),      // Text
            // Selection
            selection: Color::from_rgb_hex(0x89B4FA),        // Blue
            selection_hover: Color::from_rgb_hex(0x74C7EC),  // Sapphire
            highlight: Color::from_rgb_hex(0xF5C2E7),        // Pink
            // Lines
            separator: Color::from_rgb_hex(0x45475A),        // Surface1
            border: Color::from_rgb_hex(0x585B70),           // Surface2
            // Entity types
            entity_mesh: Color::from_rgb_hex(0xA6DA95),      // Green
            entity_particle: Color::from_rgb_hex(0xFAB387),  // Peach
            entity_light: Color::from_rgb_hex(0xF9E2AF),     // Yellow
            entity_empty: Color::from_rgb_hex(0x6C7086),     // Overlay0
            // Status
            success: Color::from_rgb_hex(0xA6DA95),          // Green
            warning: Color::from_rgb_hex(0xF9E2AF),          // Yellow
            error: Color::from_rgb_hex(0xF38BA8),            // Red
            info: Color::from_rgb_hex(0x89D9EB),             // Sky
            // Viewport
            viewport_border: Color::from_rgb_hex(0x89B4FA),  // Blue
            // Popups
            popup_bg: Color::from_rgb_hex(0x313244),         // Surface0
            popup_border: Color::from_rgb_hex(0x45475A),     // Surface1
            popup_shadow: Color::new(0.0, 0.0, 0.0, 0.5),
        }
    }

    /// Nord - An arctic, north-bluish color palette
    /// https://github.com/nordtheme/nord
    pub fn nord() -> Theme {
        Theme {
            name: "Nord",
            background: Color::from_rgb_hex(0x2E3440),       // Nord0
            background_light: Color::from_rgb_hex(0x3B4252), // Nord1
            background_dark: Color::from_rgb_hex(0x242933),  // Nord0 darker
            panel_bg: Color::from_rgb_hex(0x2E3440),         // Nord0
            panel_header: Color::from_rgb_hex(0x3B4252),     // Nord1
            panel_border: Color::from_rgb_hex(0x4C566A),     // Nord3
            text_primary: Color::from_rgb_hex(0xECEFF4),     // Nord6
            text_secondary: Color::from_rgb_hex(0xE5E9F0),   // Nord5
            text_muted: Color::from_rgb_hex(0xD8DEE9),       // Nord4
            text_accent: Color::from_rgb_hex(0xA3BE8C),      // Nord14 (Green)
            button_bg: Color::from_rgb_hex(0x3B4252),        // Nord1
            button_hover: Color::from_rgb_hex(0x434C5E),     // Nord2
            button_active: Color::from_rgb_hex(0x81A1C1),    // Nord10 (Blue)
            button_text: Color::from_rgb_hex(0xECEFF4),      // Nord6
            selection: Color::from_rgb_hex(0x81A1C1),        // Nord10
            selection_hover: Color::from_rgb_hex(0x88C0D0),  // Nord8
            highlight: Color::from_rgb_hex(0xB48EAD),        // Nord15 (Magenta)
            separator: Color::from_rgb_hex(0x3B4252),        // Nord1
            border: Color::from_rgb_hex(0x4C566A),           // Nord3
            entity_mesh: Color::from_rgb_hex(0xA3BE8C),      // Nord14
            entity_particle: Color::from_rgb_hex(0xD08770),  // Nord12
            entity_light: Color::from_rgb_hex(0xEBCB8B),     // Nord13
            entity_empty: Color::from_rgb_hex(0x4C566A),     // Nord3
            success: Color::from_rgb_hex(0xA3BE8C),          // Nord14
            warning: Color::from_rgb_hex(0xEBCB8B),          // Nord13
            error: Color::from_rgb_hex(0xBF616A),            // Nord11
            info: Color::from_rgb_hex(0x88C0D0),             // Nord8
            viewport_border: Color::from_rgb_hex(0x81A1C1),  // Nord10
            // Popups
            popup_bg: Color::from_rgb_hex(0x3B4252),         // Nord1
            popup_border: Color::from_rgb_hex(0x4C566A),     // Nord3
            popup_shadow: Color::new(0.0, 0.0, 0.0, 0.5),
        }
    }

    /// Tokyo Night - A clean, dark Neovim theme
    /// https://github.com/folke/tokyonight.nvim
    pub fn tokyo_night() -> Theme {
        Theme {
            name: "Tokyo Night",
            background: Color::from_rgb_hex(0x1A1B26),       // bg
            background_light: Color::from_rgb_hex(0x242533), // bg_dark
            background_dark: Color::from_rgb_hex(0x161721),  // bg darker
            panel_bg: Color::from_rgb_hex(0x1A1B26),         // bg
            panel_header: Color::from_rgb_hex(0x242533),     // bg_dark
            panel_border: Color::from_rgb_hex(0x3B3E4D),     // border
            text_primary: Color::from_rgb_hex(0xC0CAF5),     // fg
            text_secondary: Color::from_rgb_hex(0xA9B1D6),   // fg_dark
            text_muted: Color::from_rgb_hex(0x565F89),       // comment
            text_accent: Color::from_rgb_hex(0x9ECE6A),      // green
            button_bg: Color::from_rgb_hex(0x242533),        // bg_dark
            button_hover: Color::from_rgb_hex(0x3B3E4D),     // border
            button_active: Color::from_rgb_hex(0x7AA2F7),    // blue
            button_text: Color::from_rgb_hex(0xC0CAF5),      // fg
            selection: Color::from_rgb_hex(0x364A8E),        // bg_visual
            selection_hover: Color::from_rgb_hex(0x3E59A6),  // bg_visual lighter
            highlight: Color::from_rgb_hex(0xBB9AF7),        // purple
            separator: Color::from_rgb_hex(0x3B3E4D),        // border
            border: Color::from_rgb_hex(0x3B3E4D),           // border
            entity_mesh: Color::from_rgb_hex(0x9ECE6A),      // green
            entity_particle: Color::from_rgb_hex(0xFF9E64),  // orange
            entity_light: Color::from_rgb_hex(0xE0AF68),     // yellow
            entity_empty: Color::from_rgb_hex(0x565F89),     // comment
            success: Color::from_rgb_hex(0x9ECE6A),          // green
            warning: Color::from_rgb_hex(0xE0AF68),          // yellow
            error: Color::from_rgb_hex(0xF7768E),            // red
            info: Color::from_rgb_hex(0x7DCFEF),             // cyan
            viewport_border: Color::from_rgb_hex(0x7AA2F7),  // blue
            // Popups
            popup_bg: Color::from_rgb_hex(0x242533),         // bg_dark
            popup_border: Color::from_rgb_hex(0x3B3E4D),     // border
            popup_shadow: Color::new(0.0, 0.0, 0.0, 0.5),
        }
    }

    /// Dracula - A dark theme for many editors
    /// https://draculatheme.com/
    pub fn dracula() -> Theme {
        Theme {
            name: "Dracula",
            background: Color::from_rgb_hex(0x282A36),       // Background
            background_light: Color::from_rgb_hex(0x44475A), // CurrentLine
            background_dark: Color::from_rgb_hex(0x21222E),  // Background darker
            panel_bg: Color::from_rgb_hex(0x282A36),         // Background
            panel_header: Color::from_rgb_hex(0x343748),     // Slight variation
            panel_border: Color::from_rgb_hex(0x44475A),     // CurrentLine
            text_primary: Color::from_rgb_hex(0xF8F8F2),     // Foreground
            text_secondary: Color::from_rgb_hex(0xE5E5E5),   // Foreground slight
            text_muted: Color::from_rgb_hex(0x6272A4),       // Comment
            text_accent: Color::from_rgb_hex(0x50FA7B),      // Green
            button_bg: Color::from_rgb_hex(0x44475A),        // CurrentLine
            button_hover: Color::from_rgb_hex(0x52576C),     // Slight lighter
            button_active: Color::from_rgb_hex(0xBD93F9),    // Purple
            button_text: Color::from_rgb_hex(0xF8F8F2),      // Foreground
            selection: Color::from_rgb_hex(0xBD93F9),        // Purple
            selection_hover: Color::from_rgb_hex(0xCFA6FC),  // Purple lighter
            highlight: Color::from_rgb_hex(0xFF79C6),        // Pink
            separator: Color::from_rgb_hex(0x44475A),        // CurrentLine
            border: Color::from_rgb_hex(0x52576C),           // Lighter
            entity_mesh: Color::from_rgb_hex(0x50FA7B),      // Green
            entity_particle: Color::from_rgb_hex(0xFFB86C),  // Orange
            entity_light: Color::from_rgb_hex(0xF1FA8C),     // Yellow
            entity_empty: Color::from_rgb_hex(0x6272A4),     // Comment
            success: Color::from_rgb_hex(0x50FA7B),          // Green
            warning: Color::from_rgb_hex(0xF1FA8C),          // Yellow
            error: Color::from_rgb_hex(0xFF5555),            // Red
            info: Color::from_rgb_hex(0x8BE9FD),             // Cyan
            viewport_border: Color::from_rgb_hex(0xBD93F9),  // Purple
            // Popups
            popup_bg: Color::from_rgb_hex(0x44475A),         // CurrentLine
            popup_border: Color::from_rgb_hex(0x6272A4),     // Comment
            popup_shadow: Color::new(0.0, 0.0, 0.0, 0.5),
        }
    }

    /// Gruvbox - Retro groove color scheme
    /// https://github.com/morhetz/gruvbox
    pub fn gruvbox() -> Theme {
        Theme {
            name: "Gruvbox Dark",
            background: Color::from_rgb_hex(0x282828),       // bg0
            background_light: Color::from_rgb_hex(0x3C3836), // bg1
            background_dark: Color::from_rgb_hex(0x1D2021),  // bg0 darker
            panel_bg: Color::from_rgb_hex(0x282828),         // bg0
            panel_header: Color::from_rgb_hex(0x3C3836),     // bg1
            panel_border: Color::from_rgb_hex(0x504A45),     // bg2
            text_primary: Color::from_rgb_hex(0xEBDBB2),     // fg
            text_secondary: Color::from_rgb_hex(0xD5C4A1),   // fg2
            text_muted: Color::from_rgb_hex(0x928374),       // gray
            text_accent: Color::from_rgb_hex(0xB8BB26),      // green
            button_bg: Color::from_rgb_hex(0x3C3836),        // bg1
            button_hover: Color::from_rgb_hex(0x504A45),     // bg2
            button_active: Color::from_rgb_hex(0xD79921),    // yellow
            button_text: Color::from_rgb_hex(0xEBDBB2),      // fg
            selection: Color::from_rgb_hex(0xD79921),        // yellow
            selection_hover: Color::from_rgb_hex(0xFABD2F),  // bright yellow
            highlight: Color::from_rgb_hex(0xFE8019),        // orange
            separator: Color::from_rgb_hex(0x3C3836),        // bg1
            border: Color::from_rgb_hex(0x504A45),           // bg2
            entity_mesh: Color::from_rgb_hex(0xB8BB26),      // green
            entity_particle: Color::from_rgb_hex(0xFE8019),  // orange
            entity_light: Color::from_rgb_hex(0xFABD2F),     // yellow
            entity_empty: Color::from_rgb_hex(0x928374),     // gray
            success: Color::from_rgb_hex(0xB8BB26),          // green
            warning: Color::from_rgb_hex(0xFABD2F),          // yellow
            error: Color::from_rgb_hex(0xFB4934),            // red
            info: Color::from_rgb_hex(0x83A598),             // blue
            viewport_border: Color::from_rgb_hex(0xD79921),  // yellow
            // Popups
            popup_bg: Color::from_rgb_hex(0x3C3836),         // bg1
            popup_border: Color::from_rgb_hex(0x504A45),     // bg2
            popup_shadow: Color::new(0.0, 0.0, 0.0, 0.5),
        }
    }

    /// Default dark theme - Simple and clean
    pub fn default_dark() -> Theme {
        Theme {
            name: "Default Dark",
            background: Color::from_rgb_hex(0x1E1E1E),
            background_light: Color::from_rgb_hex(0x2D2D2D),
            background_dark: Color::from_rgb_hex(0x121212),
            panel_bg: Color::from_rgb_hex(0x252525),
            panel_header: Color::from_rgb_hex(0x333333),
            panel_border: Color::from_rgb_hex(0x3C3C3C),
            text_primary: Color::from_rgb_hex(0xCCCCCC),
            text_secondary: Color::from_rgb_hex(0xAAAAAA),
            text_muted: Color::from_rgb_hex(0x6E6E6E),
            text_accent: Color::from_rgb_hex(0x6A9955),      // VS Code green
            button_bg: Color::from_rgb_hex(0x0E639C),        // VS Code blue
            button_hover: Color::from_rgb_hex(0x1172B8),
            button_active: Color::from_rgb_hex(0x0078D4),
            button_text: Color::from_rgb_hex(0xFFFFFF),
            selection: Color::from_rgb_hex(0x264F78),
            selection_hover: Color::from_rgb_hex(0x305E8E),
            highlight: Color::from_rgb_hex(0xFDB62D),        // VS Code orange
            separator: Color::from_rgb_hex(0x3C3C3C),
            border: Color::from_rgb_hex(0x474747),
            entity_mesh: Color::from_rgb_hex(0x4EC9B0),      // VS Code teal
            entity_particle: Color::from_rgb_hex(0xCE9178),  // VS Code orange-ish
            entity_light: Color::from_rgb_hex(0xDCDCAA),     // VS Code yellow-ish
            entity_empty: Color::from_rgb_hex(0x6E6E6E),
            success: Color::from_rgb_hex(0x4EC9B0),
            warning: Color::from_rgb_hex(0xDCDCAA),
            error: Color::from_rgb_hex(0xF14C4C),
            info: Color::from_rgb_hex(0x75BEFF),
            viewport_border: Color::from_rgb_hex(0x474747),
            // Popups
            popup_bg: Color::from_rgb_hex(0x2D2D2D),
            popup_border: Color::from_rgb_hex(0x3C3C3C),
            popup_shadow: Color::new(0.0, 0.0, 0.0, 0.5),
        }
    }

    /// One Dark - Atom's iconic dark theme
    /// https://github.com/atom/one-dark-ui
    pub fn one_dark() -> Theme {
        Theme {
            name: "One Dark",
            background: Color::from_rgb_hex(0x282C34),
            background_light: Color::from_rgb_hex(0x3E4451),
            background_dark: Color::from_rgb_hex(0x21252B),
            panel_bg: Color::from_rgb_hex(0x282C34),
            panel_header: Color::from_rgb_hex(0x3E4451),
            panel_border: Color::from_rgb_hex(0x4B5263),
            text_primary: Color::from_rgb_hex(0xABB2BF),
            text_secondary: Color::from_rgb_hex(0x9DA5B4),
            text_muted: Color::from_rgb_hex(0x5C6370),
            text_accent: Color::from_rgb_hex(0x98C379),
            button_bg: Color::from_rgb_hex(0x3E4451),
            button_hover: Color::from_rgb_hex(0x4B5263),
            button_active: Color::from_rgb_hex(0x61AFEF),
            button_text: Color::from_rgb_hex(0xABB2BF),
            selection: Color::from_rgb_hex(0x3E4451),
            selection_hover: Color::from_rgb_hex(0x4B5263),
            highlight: Color::from_rgb_hex(0xC678DD),
            separator: Color::from_rgb_hex(0x3E4451),
            border: Color::from_rgb_hex(0x4B5263),
            entity_mesh: Color::from_rgb_hex(0x98C379),
            entity_particle: Color::from_rgb_hex(0xD19A66),
            entity_light: Color::from_rgb_hex(0xE5C07B),
            entity_empty: Color::from_rgb_hex(0x5C6370),
            success: Color::from_rgb_hex(0x98C379),
            warning: Color::from_rgb_hex(0xE5C07B),
            error: Color::from_rgb_hex(0xE06C75),
            info: Color::from_rgb_hex(0x61AFEF),
            viewport_border: Color::from_rgb_hex(0x61AFEF),
            // Popups
            popup_bg: Color::from_rgb_hex(0x3E4451),
            popup_border: Color::from_rgb_hex(0x4B5263),
            popup_shadow: Color::new(0.0, 0.0, 0.0, 0.5),
        }
    }

    /// Material Palenight - Material Design inspired dark theme
    /// https://github.com/equinusocio/material-theme
    pub fn material_palenight() -> Theme {
        Theme {
            name: "Material Palenight",
            background: Color::from_rgb_hex(0x292D3E),
            background_light: Color::from_rgb_hex(0x3A3F5B),
            background_dark: Color::from_rgb_hex(0x1E2133),
            panel_bg: Color::from_rgb_hex(0x292D3E),
            panel_header: Color::from_rgb_hex(0x3A3F5B),
            panel_border: Color::from_rgb_hex(0x414763),
            text_primary: Color::from_rgb_hex(0xA6ACCD),
            text_secondary: Color::from_rgb_hex(0x8A93B5),
            text_muted: Color::from_rgb_hex(0x676E95),
            text_accent: Color::from_rgb_hex(0xC3E88D),
            button_bg: Color::from_rgb_hex(0x3A3F5B),
            button_hover: Color::from_rgb_hex(0x414763),
            button_active: Color::from_rgb_hex(0x82AAFF),
            button_text: Color::from_rgb_hex(0xA6ACCD),
            selection: Color::from_rgb_hex(0x676E95),
            selection_hover: Color::from_rgb_hex(0x7A819D),
            highlight: Color::from_rgb_hex(0xC792EA),
            separator: Color::from_rgb_hex(0x3A3F5B),
            border: Color::from_rgb_hex(0x414763),
            entity_mesh: Color::from_rgb_hex(0xC3E88D),
            entity_particle: Color::from_rgb_hex(0xF78C6C),
            entity_light: Color::from_rgb_hex(0xFFCB6B),
            entity_empty: Color::from_rgb_hex(0x676E95),
            success: Color::from_rgb_hex(0xC3E88D),
            warning: Color::from_rgb_hex(0xFFCB6B),
            error: Color::from_rgb_hex(0xFF5370),
            info: Color::from_rgb_hex(0x82AAFF),
            viewport_border: Color::from_rgb_hex(0x82AAFF),
            // Popups
            popup_bg: Color::from_rgb_hex(0x3A3F5B),
            popup_border: Color::from_rgb_hex(0x414763),
            popup_shadow: Color::new(0.0, 0.0, 0.0, 0.5),
        }
    }

    /// Ayu Dark - A simple and clean dark theme
    /// https://github.com/ayu-theme
    pub fn ayu_dark() -> Theme {
        Theme {
            name: "Ayu Dark",
            background: Color::from_rgb_hex(0x0D1017),
            background_light: Color::from_rgb_hex(0x1A1F29),
            background_dark: Color::from_rgb_hex(0x070A0F),
            panel_bg: Color::from_rgb_hex(0x0D1017),
            panel_header: Color::from_rgb_hex(0x1A1F29),
            panel_border: Color::from_rgb_hex(0x2D3440),
            text_primary: Color::from_rgb_hex(0xBFBDB6),
            text_secondary: Color::from_rgb_hex(0xA8A49D),
            text_muted: Color::from_rgb_hex(0x5C6773),
            text_accent: Color::from_rgb_hex(0xBED9F5),
            button_bg: Color::from_rgb_hex(0x1A1F29),
            button_hover: Color::from_rgb_hex(0x2D3440),
            button_active: Color::from_rgb_hex(0x39BAE6),
            button_text: Color::from_rgb_hex(0xBFBDB6),
            selection: Color::from_rgb_hex(0x1A1F29),
            selection_hover: Color::from_rgb_hex(0x2D3440),
            highlight: Color::from_rgb_hex(0xF07178),
            separator: Color::from_rgb_hex(0x1A1F29),
            border: Color::from_rgb_hex(0x2D3440),
            entity_mesh: Color::from_rgb_hex(0x7FD962),
            entity_particle: Color::from_rgb_hex(0xFF9940),
            entity_light: Color::from_rgb_hex(0xFFB454),
            entity_empty: Color::from_rgb_hex(0x5C6773),
            success: Color::from_rgb_hex(0x7FD962),
            warning: Color::from_rgb_hex(0xFFB454),
            error: Color::from_rgb_hex(0xF07178),
            info: Color::from_rgb_hex(0x39BAE6),
            viewport_border: Color::from_rgb_hex(0x39BAE6),
            // Popups
            popup_bg: Color::from_rgb_hex(0x1A1F29),
            popup_border: Color::from_rgb_hex(0x2D3440),
            popup_shadow: Color::new(0.0, 0.0, 0.0, 0.5),
        }
    }

    /// GitHub Dark - GitHub's official dark theme
    /// https://github.com/primer/github-vscode-theme
    pub fn github_dark() -> Theme {
        Theme {
            name: "GitHub Dark",
            background: Color::from_rgb_hex(0x0D1117),
            background_light: Color::from_rgb_hex(0x161B22),
            background_dark: Color::from_rgb_hex(0x010409),
            panel_bg: Color::from_rgb_hex(0x0D1117),
            panel_header: Color::from_rgb_hex(0x161B22),
            panel_border: Color::from_rgb_hex(0x30363D),
            text_primary: Color::from_rgb_hex(0xE6EDF3),
            text_secondary: Color::from_rgb_hex(0xC9D1D9),
            text_muted: Color::from_rgb_hex(0x7D8590),
            text_accent: Color::from_rgb_hex(0x3FB950),
            button_bg: Color::from_rgb_hex(0x21262D),
            button_hover: Color::from_rgb_hex(0x30363D),
            button_active: Color::from_rgb_hex(0x1F6FEB),
            button_text: Color::from_rgb_hex(0xE6EDF3),
            selection: Color::from_rgb_hex(0x1F6FEB),
            selection_hover: Color::from_rgb_hex(0x388BFD),
            highlight: Color::from_rgb_hex(0xF778BA),
            separator: Color::from_rgb_hex(0x21262D),
            border: Color::from_rgb_hex(0x30363D),
            entity_mesh: Color::from_rgb_hex(0x3FB950),
            entity_particle: Color::from_rgb_hex(0xDB6D28),
            entity_light: Color::from_rgb_hex(0xD29922),
            entity_empty: Color::from_rgb_hex(0x7D8590),
            success: Color::from_rgb_hex(0x3FB950),
            warning: Color::from_rgb_hex(0xD29922),
            error: Color::from_rgb_hex(0xF85149),
            info: Color::from_rgb_hex(0x58A6FF),
            viewport_border: Color::from_rgb_hex(0x30363D),
            // Popups
            popup_bg: Color::from_rgb_hex(0x161B22),
            popup_border: Color::from_rgb_hex(0x30363D),
            popup_shadow: Color::new(0.0, 0.0, 0.0, 0.5),
        }
    }

    /// Monokai - Classic Monokai color scheme
    /// https://monokai.pro/
    pub fn monokai() -> Theme {
        Theme {
            name: "Monokai",
            background: Color::from_rgb_hex(0x272822),
            background_light: Color::from_rgb_hex(0x3E3D32),
            background_dark: Color::from_rgb_hex(0x1E1F1C),
            panel_bg: Color::from_rgb_hex(0x272822),
            panel_header: Color::from_rgb_hex(0x3E3D32),
            panel_border: Color::from_rgb_hex(0x49483E),
            text_primary: Color::from_rgb_hex(0xF8F8F2),
            text_secondary: Color::from_rgb_hex(0xCFCFC2),
            text_muted: Color::from_rgb_hex(0x75715E),
            text_accent: Color::from_rgb_hex(0xA6E22E),
            button_bg: Color::from_rgb_hex(0x3E3D32),
            button_hover: Color::from_rgb_hex(0x49483E),
            button_active: Color::from_rgb_hex(0x66D9EF),
            button_text: Color::from_rgb_hex(0xF8F8F2),
            selection: Color::from_rgb_hex(0x49483E),
            selection_hover: Color::from_rgb_hex(0x5A5950),
            highlight: Color::from_rgb_hex(0xFD971F),
            separator: Color::from_rgb_hex(0x3E3D32),
            border: Color::from_rgb_hex(0x49483E),
            entity_mesh: Color::from_rgb_hex(0xA6E22E),
            entity_particle: Color::from_rgb_hex(0xFD971F),
            entity_light: Color::from_rgb_hex(0xE6DB74),
            entity_empty: Color::from_rgb_hex(0x75715E),
            success: Color::from_rgb_hex(0xA6E22E),
            warning: Color::from_rgb_hex(0xE6DB74),
            error: Color::from_rgb_hex(0xF92672),
            info: Color::from_rgb_hex(0x66D9EF),
            viewport_border: Color::from_rgb_hex(0x66D9EF),
            // Popups
            popup_bg: Color::from_rgb_hex(0x3E3D32),
            popup_border: Color::from_rgb_hex(0x49483E),
            popup_shadow: Color::new(0.0, 0.0, 0.0, 0.5),
        }
    }

    /// Rosé Pine - Soothing pastel theme for the high-spirited
    /// https://rosepinetheme.com/
    pub fn rose_pine() -> Theme {
        Theme {
            name: "Rosé Pine",
            background: Color::from_rgb_hex(0x191724),
            background_light: Color::from_rgb_hex(0x1F1D2E),
            background_dark: Color::from_rgb_hex(0x13111C),
            panel_bg: Color::from_rgb_hex(0x191724),
            panel_header: Color::from_rgb_hex(0x1F1D2E),
            panel_border: Color::from_rgb_hex(0x26233A),
            text_primary: Color::from_rgb_hex(0xE0DEF4),
            text_secondary: Color::from_rgb_hex(0xC4C7D7),
            text_muted: Color::from_rgb_hex(0x6E6A86),
            text_accent: Color::from_rgb_hex(0x9CCFD8),
            button_bg: Color::from_rgb_hex(0x1F1D2E),
            button_hover: Color::from_rgb_hex(0x26233A),
            button_active: Color::from_rgb_hex(0xEBBCBA),
            button_text: Color::from_rgb_hex(0xE0DEF4),
            selection: Color::from_rgb_hex(0x403D52),
            selection_hover: Color::from_rgb_hex(0x524F67),
            highlight: Color::from_rgb_hex(0xF6C177),
            separator: Color::from_rgb_hex(0x1F1D2E),
            border: Color::from_rgb_hex(0x26233A),
            entity_mesh: Color::from_rgb_hex(0x9CCFD8),
            entity_particle: Color::from_rgb_hex(0xEB6F92),
            entity_light: Color::from_rgb_hex(0xF6C177),
            entity_empty: Color::from_rgb_hex(0x6E6A86),
            success: Color::from_rgb_hex(0x9CCFD8),
            warning: Color::from_rgb_hex(0xF6C177),
            error: Color::from_rgb_hex(0xEB6F92),
            info: Color::from_rgb_hex(0xC4A7E7),
            viewport_border: Color::from_rgb_hex(0x524F67),
            // Popups
            popup_bg: Color::from_rgb_hex(0x1F1D2E),
            popup_border: Color::from_rgb_hex(0x26233A),
            popup_shadow: Color::new(0.0, 0.0, 0.0, 0.5),
        }
    }

    /// Kanagawa - Inspired by the colors of the famous painting by Katsushika Hokusai
    /// https://github.com/rebelot/kanagawa.nvim
    pub fn kanagawa() -> Theme {
        Theme {
            name: "Kanagawa",
            background: Color::from_rgb_hex(0x1F1F28),
            background_light: Color::from_rgb_hex(0x2A2A3C),
            background_dark: Color::from_rgb_hex(0x16161D),
            panel_bg: Color::from_rgb_hex(0x1F1F28),
            panel_header: Color::from_rgb_hex(0x2A2A3C),
            panel_border: Color::from_rgb_hex(0x3B3B4F),
            text_primary: Color::from_rgb_hex(0xDCD7BA),
            text_secondary: Color::from_rgb_hex(0xC8C0B8),
            text_muted: Color::from_rgb_hex(0x727169),
            text_accent: Color::from_rgb_hex(0x76946A),
            button_bg: Color::from_rgb_hex(0x2A2A3C),
            button_hover: Color::from_rgb_hex(0x3B3B4F),
            button_active: Color::from_rgb_hex(0x7E9CD8),
            button_text: Color::from_rgb_hex(0xDCD7BA),
            selection: Color::from_rgb_hex(0x2D4F67),
            selection_hover: Color::from_rgb_hex(0x3D5F77),
            highlight: Color::from_rgb_hex(0x957FB8),
            separator: Color::from_rgb_hex(0x2A2A3C),
            border: Color::from_rgb_hex(0x3B3B4F),
            entity_mesh: Color::from_rgb_hex(0x76946A),
            entity_particle: Color::from_rgb_hex(0xFFA066),
            entity_light: Color::from_rgb_hex(0xE6C384),
            entity_empty: Color::from_rgb_hex(0x727169),
            success: Color::from_rgb_hex(0x76946A),
            warning: Color::from_rgb_hex(0xE6C384),
            error: Color::from_rgb_hex(0xC34043),
            info: Color::from_rgb_hex(0x7E9CD8),
            viewport_border: Color::from_rgb_hex(0x7E9CD8),
            // Popups
            popup_bg: Color::from_rgb_hex(0x2A2A3C),
            popup_border: Color::from_rgb_hex(0x3B3B4F),
            popup_shadow: Color::new(0.0, 0.0, 0.0, 0.5),
        }
    }

    /// Solarized Dark - Precision color scheme for machines and people
    /// https://ethanschoonover.com/solarized/
    pub fn solarized_dark() -> Theme {
        Theme {
            name: "Solarized Dark",
            background: Color::from_rgb_hex(0x002B36),
            background_light: Color::from_rgb_hex(0x073642),
            background_dark: Color::from_rgb_hex(0x001E26),
            panel_bg: Color::from_rgb_hex(0x002B36),
            panel_header: Color::from_rgb_hex(0x073642),
            panel_border: Color::from_rgb_hex(0x094B5A),
            text_primary: Color::from_rgb_hex(0x839496),
            text_secondary: Color::from_rgb_hex(0x657B83),
            text_muted: Color::from_rgb_hex(0x586E75),
            text_accent: Color::from_rgb_hex(0x859900),
            button_bg: Color::from_rgb_hex(0x073642),
            button_hover: Color::from_rgb_hex(0x094B5A),
            button_active: Color::from_rgb_hex(0x268BD2),
            button_text: Color::from_rgb_hex(0x839496),
            selection: Color::from_rgb_hex(0x094B5A),
            selection_hover: Color::from_rgb_hex(0x0A5A6C),
            highlight: Color::from_rgb_hex(0xD33682),
            separator: Color::from_rgb_hex(0x073642),
            border: Color::from_rgb_hex(0x094B5A),
            entity_mesh: Color::from_rgb_hex(0x859900),
            entity_particle: Color::from_rgb_hex(0xCB4B16),
            entity_light: Color::from_rgb_hex(0xB58900),
            entity_empty: Color::from_rgb_hex(0x586E75),
            success: Color::from_rgb_hex(0x859900),
            warning: Color::from_rgb_hex(0xB58900),
            error: Color::from_rgb_hex(0xDC322F),
            info: Color::from_rgb_hex(0x268BD2),
            viewport_border: Color::from_rgb_hex(0x268BD2),
            // Popups
            popup_bg: Color::from_rgb_hex(0x073642),
            popup_border: Color::from_rgb_hex(0x094B5A),
            popup_shadow: Color::new(0.0, 0.0, 0.0, 0.5),
        }
    }

    /// Apply theme colors to a UiStyle.
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
