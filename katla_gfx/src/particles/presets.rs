//! Particle effect presets system.
//!
//! This module provides functionality for saving and loading particle emitter
//! configurations as JSON files, allowing easy reuse and sharing of effects.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use super::EmitterConfig;

/// Particle emitter preset with metadata.
///
/// Combines a human-readable name with emitter configuration
/// for serialization to JSON.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EmitterPreset {
    /// Human-readable name for this preset
    pub name: String,

    /// Emitter configuration data
    pub config: EmitterConfig,
}

impl EmitterPreset {
    /// Create a new preset from name and config.
    pub fn new(name: String, config: EmitterConfig) -> Self {
        Self { name, config }
    }

    /// Save preset to a JSON file.
    ///
    /// # Arguments
    /// * `path` - Destination file path (will create parent directories if needed)
    ///
    /// # Errors
    /// Returns error if file creation or JSON serialization fails
    pub fn save_to_file(&self, path: &Path) -> Result<(), String> {
        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory {}: {}", parent.display(), e))?;
        }

        // Serialize to JSON with pretty formatting
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize preset: {}", e))?;

        // Write to file
        fs::write(path, json)
            .map_err(|e| format!("Failed to write preset file {}: {}", path.display(), e))?;

        log::info!(
            "Saved particle preset '{}' to {}",
            self.name,
            path.display()
        );
        Ok(())
    }

    /// Load preset from a JSON file.
    ///
    /// # Arguments
    /// * `path` - Source file path
    ///
    /// # Errors
    /// Returns error if file read or JSON deserialization fails
    pub fn load_from_file(path: &Path) -> Result<Self, String> {
        // Read file
        let json = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read preset file {}: {}", path.display(), e))?;

        // Deserialize JSON
        let preset: Self = serde_json::from_str(&json).map_err(|e| {
            format!(
                "Failed to deserialize preset from {}: {}",
                path.display(),
                e
            )
        })?;

        log::info!(
            "Loaded particle preset '{}' from {}",
            preset.name,
            path.display()
        );
        Ok(preset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emitter_preset_serialization() {
        let config = EmitterConfig {
            position: [1.0, 2.0, 3.0],
            emit_rate: 500.0,
            base_lifetime: 3.0,
            ..Default::default()
        };

        let preset = EmitterPreset::new("test_preset".to_string(), config);

        // Test serialization
        let json = serde_json::to_string(&preset).unwrap();
        println!("Serialized preset: {}", json);

        // Test deserialization
        let deserialized: EmitterPreset = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.name, "test_preset");
        assert_eq!(deserialized.config.position, [1.0, 2.0, 3.0]);
        assert_eq!(deserialized.config.emit_rate, 500.0);
        assert_eq!(deserialized.config.base_lifetime, 3.0);
    }
}
