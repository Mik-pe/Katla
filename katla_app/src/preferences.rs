//! Application preferences with persistent storage.

use std::io;

use log::warn;

use crate::ui::ColorScheme;

/// Application preferences that persist between sessions.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Preferences {
    /// Currently selected theme name.
    pub theme: String,
    /// Show the grid in the viewport.
    pub show_grid: bool,
    /// Show the stats panel.
    pub show_stats: bool,
    /// Font scale multiplier (1.0 = 100%, 1.25 = 125%, etc.)
    pub font_scale: f32,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: "catppuccin".to_string(),
            show_grid: true,
            show_stats: true,
            font_scale: 1.0,
        }
    }
}

impl Preferences {
    /// Load preferences from disk, or return defaults if not found.
    pub fn load() -> Self {
        let content = match crate::util::load_config_file("preferences.toml") {
            Some(c) => c,
            None => {
                warn!("Could not load preferences file");
                return Self::default();
            }
        };

        let mut prefs: Self = match toml::from_str(&content) {
            Ok(p) => p,
            Err(e) => {
                warn!("Failed to parse preferences: {}", e);
                return Self::default();
            }
        };

        prefs.validate();
        prefs
    }

    /// Save preferences to disk.
    pub fn save(&self) -> io::Result<()> {
        let content = match toml::to_string_pretty(self) {
            Ok(c) => c,
            Err(e) => return Err(io::Error::new(io::ErrorKind::InvalidData, e)),
        };
        crate::util::save_config_file("preferences.toml", &content)
    }

    fn validate(&mut self) {
        if ColorScheme::by_name(&self.theme).is_none() {
            warn!("Unknown theme '{}', using default", self.theme);
            self.theme = "catppuccin".to_string();
        }
        self.font_scale = self.font_scale.clamp(0.5, 3.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_toml() {
        let content = r#"
theme = "nord"
show_grid = false
show_stats = true
font_scale = 1.25
"#;
        let mut prefs: Preferences = toml::from_str(content).unwrap();
        prefs.validate();
        assert_eq!(prefs.theme, "nord");
        assert!(!prefs.show_grid);
        assert!(prefs.show_stats);
        assert_eq!(prefs.font_scale, 1.25);
    }

    #[test]
    fn test_to_toml() {
        let prefs = Preferences {
            theme: "dracula".to_string(),
            show_grid: false,
            show_stats: true,
            font_scale: 1.5,
        };
        let toml = toml::to_string_pretty(&prefs).unwrap();
        assert!(toml.contains("theme = \"dracula\""));
        assert!(toml.contains("show_grid = false"));
        assert!(toml.contains("font_scale = 1.5"));
    }

    #[test]
    fn test_invalid_theme_uses_default() {
        let content = "theme = \"nonexistent\"";
        let mut prefs: Preferences = toml::from_str(content).unwrap();
        prefs.validate();
        assert_eq!(prefs.theme, "catppuccin");
    }
}
