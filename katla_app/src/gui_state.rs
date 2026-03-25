//! GUI layout state with persistent storage.

use std::io;

use log::{debug, warn};

/// GUI layout state that persists between sessions.
#[derive(Debug, Clone)]
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

        Self::parse_toml(&content)
    }

    /// Save GUI state to disk.
    pub fn save(&self) -> io::Result<()> {
        crate::util::save_config_file("gui_state.toml", &self.to_toml())
    }

    /// Parse GUI state from TOML content.
    fn parse_toml(content: &str) -> Self {
        let mut state = Self::default();

        for line in content.lines() {
            let line = line.trim();

            // Skip comments and empty lines
            if line.starts_with('#') || line.is_empty() {
                continue;
            }

            // Parse key = value
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim();

                match key {
                    "left_panel_width" => {
                        let width: f32 = value.parse().unwrap_or(220.0);
                        state.left_panel_width = width.clamp(100.0, 600.0);
                    }
                    "right_panel_width" => {
                        let width: f32 = value.parse().unwrap_or(280.0);
                        state.right_panel_width = width.clamp(100.0, 600.0);
                    }
                    "asset_browser_height" => {
                        let height: f32 = value.parse().unwrap_or(200.0);
                        state.asset_browser_height = height.clamp(100.0, 500.0);
                    }
                    _ => {
                        debug!("Unknown GUI state key: {}", key);
                    }
                }
            }
        }

        state
    }

    /// Convert GUI state to TOML content.
    fn to_toml(&self) -> String {
        format!(
            r#"# Katla Engine GUI State
# This file stores UI layout preferences and is automatically generated.

# Left panel (hierarchy) width in pixels
left_panel_width = {}

# Right panel (inspector) width in pixels
right_panel_width = {}

# Asset browser panel height in pixels
asset_browser_height = {}
"#,
            self.left_panel_width as i32,
            self.right_panel_width as i32,
            self.asset_browser_height as i32
        )
    }
}
