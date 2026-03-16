//! Particle effect presets system.
//!
//! This module provides functionality for saving and loading particle emitter
//! configurations as JSON files, allowing easy reuse and sharing of effects.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

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

/// Preset manager for particle system.
///
/// Handles scanning, loading, and saving particle effect presets.
pub struct PresetManager {
    /// Directory containing preset files
    presets_dir: PathBuf,

    /// Cached list of available preset names
    available_presets: Vec<String>,
}

impl PresetManager {
    /// Create a new preset manager.
    ///
    /// # Arguments
    /// * `presets_dir` - Directory containing JSON preset files
    ///
    /// # Note
    /// Creates the directory if it doesn't exist
    pub fn new<P: AsRef<Path>>(presets_dir: P) -> Result<Self, String> {
        let presets_dir = presets_dir.as_ref().to_path_buf();

        // Create directory if it doesn't exist
        if !presets_dir.exists() {
            fs::create_dir_all(&presets_dir).map_err(|e| {
                format!(
                    "Failed to create presets directory {}: {}",
                    presets_dir.display(),
                    e
                )
            })?;
            log::info!(
                "Created particle presets directory: {}",
                presets_dir.display()
            );
        }

        let mut manager = Self {
            presets_dir,
            available_presets: Vec::new(),
        };

        // Scan for existing presets
        manager.scan_presets()?;

        Ok(manager)
    }

    /// Scan presets directory for JSON files.
    ///
    /// Updates the internal list of available preset names.
    fn scan_presets(&mut self) -> Result<(), String> {
        self.available_presets.clear();

        let entries = fs::read_dir(&self.presets_dir).map_err(|e| {
            format!(
                "Failed to read presets directory {}: {}",
                self.presets_dir.display(),
                e
            )
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
            let path = entry.path();

            // Only process .json files
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                // Try to extract name from filename (without .json extension)
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    self.available_presets.push(stem.to_string());
                }
            }
        }

        log::info!(
            "Found {} particle presets in {}",
            self.available_presets.len(),
            self.presets_dir.display()
        );

        Ok(())
    }

    /// Get list of available preset names.
    pub fn get_available_presets(&self) -> &[String] {
        &self.available_presets
    }

    /// Load a preset by name.
    ///
    /// # Arguments
    /// * `name` - Preset name (filename without .json extension)
    ///
    /// # Errors
    /// Returns error if preset file not found or deserialization fails
    pub fn load_preset(&self, name: &str) -> Result<EmitterConfig, String> {
        let path = self.presets_dir.join(format!("{}.json", name));

        if !path.exists() {
            return Err(format!("Preset '{}' not found at {}", name, path.display()));
        }

        let preset = EmitterPreset::load_from_file(&path)?;
        Ok(preset.config)
    }

    /// Save a preset by name.
    ///
    /// # Arguments
    /// * `name` - Preset name (will be saved as name.json)
    /// * `config` - Emitter configuration to save
    ///
    /// # Errors
    /// Returns error if file write or serialization fails
    pub fn save_preset(&mut self, name: &str, config: &EmitterConfig) -> Result<(), String> {
        let preset = EmitterPreset::new(name.to_string(), *config);
        let path = self.presets_dir.join(format!("{}.json", name));

        preset.save_to_file(&path)?;

        // Update available presets list
        if !self.available_presets.contains(&name.to_string()) {
            self.available_presets.push(name.to_string());
        }

        Ok(())
    }

    /// Load all presets from directory.
    ///
    /// Returns a map of preset name to emitter configuration.
    ///
    /// # Errors
    /// Returns error if directory scan or any preset load fails
    pub fn load_all_presets(&self) -> Result<Vec<(String, EmitterConfig)>, String> {
        let mut presets = Vec::new();

        for name in &self.available_presets {
            match self.load_preset(name) {
                Ok(config) => {
                    presets.push((name.clone(), config));
                }
                Err(e) => {
                    log::warn!("Failed to load preset '{}': {}", name, e);
                    // Continue loading other presets
                }
            }
        }

        log::info!("Loaded {} particle presets", presets.len());
        Ok(presets)
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
