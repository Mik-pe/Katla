//! GUI layout state with persistent storage.

use std::io;

use log::warn;

/// GUI layout state that persists between sessions.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct GuiState {
    /// Left panel (hierarchy) width in pixels.
    pub left_panel_width: f32,
    /// Right panel (inspector) width in pixels.
    pub right_panel_width: f32,
    /// Asset browser panel height in pixels.
    pub asset_browser_height: f32,
}

impl Default for GuiState {
    fn default() -> Self {
        Self {
            left_panel_width: 220.0,
            right_panel_width: 280.0,
            asset_browser_height: 200.0,
        }
    }
}

impl GuiState {
    /// Load GUI state from disk, or return defaults if not found.
    pub fn load() -> Self {
        let content = match crate::util::load_config_file("gui_state.toml") {
            Some(c) => c,
            None => {
                warn!("Could not load GUI state file");
                return Self::default();
            }
        };

        let mut state: Self = match toml::from_str(&content) {
            Ok(s) => s,
            Err(e) => {
                warn!("Failed to parse GUI state: {}", e);
                return Self::default();
            }
        };

        state.clamp();
        state
    }

    /// Save GUI state to disk.
    pub fn save(&self) -> io::Result<()> {
        let content = match toml::to_string_pretty(self) {
            Ok(c) => c,
            Err(e) => return Err(io::Error::new(io::ErrorKind::InvalidData, e)),
        };
        crate::util::save_config_file("gui_state.toml", &content)
    }

    fn clamp(&mut self) {
        self.left_panel_width = self.left_panel_width.clamp(100.0, 600.0);
        self.right_panel_width = self.right_panel_width.clamp(100.0, 600.0);
        self.asset_browser_height = self.asset_browser_height.clamp(100.0, 500.0);
    }
}
