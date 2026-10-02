//! Application preferences with persistent storage.

use std::io;

use log::{debug, error, warn};

use crate::ui::ColorScheme;

/// Editor viewport settings, persisted with application preferences.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct EditorSettings {
    pub snap_to_grid: bool,
    pub camera_speed: f32,
    pub grid_size: f32,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            snap_to_grid: true,
            camera_speed: 50.0,
            grid_size: 1.0,
        }
    }
}

/// Audio volume settings.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    pub master_volume: f32,
    pub sfx_volume: f32,
    pub music_volume: f32,
    pub ambient_volume: f32,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            master_volume: 1.0,
            sfx_volume: 1.0,
            music_volume: 1.0,
            ambient_volume: 1.0,
        }
    }
}

/// Local attachment to an existing external conversation; contains no credentials.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ExternalChatPreferences {
    /// Private control socket published by the existing host.
    pub socket: String,
    /// Already existing conversation to rejoin in that host.
    pub thread_id: String,
}

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
    /// Show physics debug wireframe overlay.
    pub show_physics_debug: bool,
    /// Show reverb zone wireframe overlay.
    pub show_reverb_debug: bool,
    /// Font scale multiplier (1.0 = 100%, 1.25 = 125%, etc.)
    pub font_scale: f32,
    #[serde(default)]
    pub audio: AudioSettings,
    pub editor: EditorSettings,
    pub external_chat: ExternalChatPreferences,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            show_grid: true,
            show_stats: true,
            show_physics_debug: false,
            show_reverb_debug: false,
            font_scale: 1.0,
            audio: AudioSettings::default(),
            editor: EditorSettings::default(),
            external_chat: ExternalChatPreferences::default(),
        }
    }
}

impl Preferences {
    /// Load preferences from disk, or return defaults if not found.
    pub fn load() -> Self {
        let content = match crate::util::load_config_file("preferences.toml") {
            Some(c) => c,
            None => {
                debug!("Using default preferences");
                return Self::default();
            }
        };

        let mut prefs: Self = match toml::from_str(&content) {
            Ok(p) => p,
            Err(e) => {
                error!("Failed to parse preferences: {}", e);
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
            warn!("Unknown theme '{}', using rcp", self.theme);
            self.theme = "rcp".to_string();
        }
        self.font_scale = finite_clamp(self.font_scale, 1.0, 0.5, 3.0);
        self.editor.camera_speed = finite_clamp(self.editor.camera_speed, 50.0, 1.0, 200.0);
        self.editor.grid_size = finite_clamp(self.editor.grid_size, 1.0, 0.01, 100.0);
        self.audio.master_volume = finite_clamp(self.audio.master_volume, 1.0, 0.0, 1.0);
        self.audio.sfx_volume = finite_clamp(self.audio.sfx_volume, 1.0, 0.0, 1.0);
        self.audio.music_volume = finite_clamp(self.audio.music_volume, 1.0, 0.0, 1.0);
        self.audio.ambient_volume = finite_clamp(self.audio.ambient_volume, 1.0, 0.0, 1.0);
    }
}

pub(crate) fn finite_clamp(value: f32, default: f32, min: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        default
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
            show_physics_debug: false,
            show_reverb_debug: false,
            font_scale: 1.5,
            editor: EditorSettings::default(),
            external_chat: ExternalChatPreferences::default(),
            audio: AudioSettings {
                master_volume: 0.8,
                sfx_volume: 1.0,
                music_volume: 0.5,
                ambient_volume: 1.0,
            },
        };
        let toml = toml::to_string_pretty(&prefs).unwrap();
        assert!(toml.contains("theme = \"dracula\""));
        assert!(toml.contains("show_grid = false"));
        assert!(toml.contains("font_scale = 1.5"));
        assert!(toml.contains("master_volume = 0.8"));
    }

    #[test]
    fn test_invalid_theme_uses_default() {
        let content = "theme = \"nonexistent\"";
        let mut prefs: Preferences = toml::from_str(content).unwrap();
        prefs.validate();
        assert_eq!(prefs.theme, "rcp");
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn test_viewport_settings_round_trip_and_nonfinite_inputs_use_defaults() {
        let mut preferences = Preferences::default();
        preferences.editor.snap_to_grid = false;
        preferences.editor.camera_speed = 75.0;
        preferences.editor.grid_size = 0.5;
        preferences.external_chat.socket = "/tmp/codex-test-owner.sock".into();
        preferences.external_chat.thread_id = "existing-owner".into();
        let encoded = toml::to_string(&preferences).unwrap();
        let restored: Preferences = toml::from_str(&encoded).unwrap();
        assert_eq!(preferences.editor, restored.editor);
        assert_eq!(
            preferences.external_chat.socket,
            restored.external_chat.socket
        );
        assert_eq!(
            preferences.external_chat.thread_id,
            restored.external_chat.thread_id
        );
        preferences.font_scale = f32::NAN;
        preferences.audio.master_volume = f32::INFINITY;
        preferences.editor.grid_size = f32::NEG_INFINITY;
        preferences.validate();
        assert_eq!(preferences.font_scale, 1.0);
        assert_eq!(preferences.audio.master_volume, 1.0);
        assert_eq!(preferences.editor.grid_size, 1.0);
    }
}
