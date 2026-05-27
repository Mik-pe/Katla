//! Particle effect presets — save and load emitter configurations as JSON.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use super::{EmitterConfig, EmitterShape};

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

    pub fn fire() -> EmitterConfig {
        EmitterConfig::builder()
            .emit_rate(80.0)
            .base_lifetime(1.5)
            .lifetime_variation(0.3)
            .velocity_direction(0.0, 1.0, 0.0)
            .velocity_magnitude(2.0)
            .velocity_cone_angle(0.3)
            .base_scale(0.15)
            .scale_variation(0.4)
            .color(1.0, 0.4, 0.05, 1.0)
            .color_end(0.2, 0.0, 0.0, 1.0)
            .scale_end(0.1)
            .color_variation(0.3)
            .gravity(-2.0)
            .turbulence_strength(0.5)
            .turbulence_frequency(4.0)
            .build()
    }

    pub fn smoke() -> EmitterConfig {
        EmitterConfig::builder()
            .emit_rate(25.0)
            .base_lifetime(4.0)
            .lifetime_variation(0.4)
            .velocity_direction(0.0, 1.0, 0.0)
            .velocity_magnitude(0.5)
            .velocity_cone_angle(0.6)
            .base_scale(0.3)
            .scale_variation(0.5)
            .color(0.5, 0.5, 0.5, 0.6)
            .color_end(0.3, 0.3, 0.3, 1.0)
            .scale_end(2.0)
            .color_variation(0.15)
            .gravity(0.3)
            .turbulence_strength(0.8)
            .turbulence_frequency(2.0)
            .build()
    }

    pub fn sparks() -> EmitterConfig {
        EmitterConfig::builder()
            .emit_rate(200.0)
            .base_lifetime(0.5)
            .lifetime_variation(0.5)
            .velocity_direction(0.0, 1.0, 0.0)
            .velocity_magnitude(5.0)
            .velocity_cone_angle(0.8)
            .base_scale(0.03)
            .scale_variation(0.3)
            .color(1.0, 0.9, 0.4, 1.0)
            .color_end(1.0, 0.3, 0.0, 1.0)
            .scale_end(0.0)
            .color_variation(0.2)
            .gravity(-9.8)
            .build()
    }

    pub fn rain() -> EmitterConfig {
        EmitterConfig::builder()
            .shape(EmitterShape::Box)
            .emit_rate(300.0)
            .base_lifetime(2.0)
            .lifetime_variation(0.1)
            .velocity_direction(0.0, -1.0, 0.0)
            .velocity_magnitude(8.0)
            .velocity_cone_angle(0.05)
            .base_scale(0.02)
            .scale_variation(0.1)
            .color(0.7, 0.8, 1.0, 0.4)
            .color_end(0.5, 0.6, 0.9, 1.0)
            .scale_end(1.0)
            .color_variation(0.05)
            .gravity(-9.8)
            .shape_params([10.0, 1.0, 10.0, 0.0])
            .build()
    }

    pub fn snow() -> EmitterConfig {
        EmitterConfig::builder()
            .shape(EmitterShape::Box)
            .emit_rate(60.0)
            .base_lifetime(6.0)
            .lifetime_variation(0.3)
            .velocity_direction(0.0, -1.0, 0.0)
            .velocity_magnitude(0.5)
            .velocity_cone_angle(0.2)
            .base_scale(0.05)
            .scale_variation(0.4)
            .color(1.0, 1.0, 1.0, 0.9)
            .color_end(0.9, 0.9, 1.0, 1.0)
            .scale_end(0.5)
            .color_variation(0.05)
            .gravity(-0.5)
            .turbulence_strength(0.6)
            .turbulence_frequency(1.5)
            .shape_params([10.0, 1.0, 10.0, 0.0])
            .build()
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
