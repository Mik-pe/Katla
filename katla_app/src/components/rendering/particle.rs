use katla_ecs::Component;
use katla_gfx::particles::EmitterConfig;
use katla_gfx::particles::EmitterHandle;

/// Particle emitter component for ECS entities.
///
/// This component attaches a particle emitter to an entity, allowing
/// particle effects to be positioned and controlled via the ECS.
#[derive(Component)]
pub struct ParticleEmitterComponent {
    /// Emitter configuration (position, emit rate, color, etc.)
    pub config: EmitterConfig,

    /// Handle to the emitter in the global particle system
    pub emitter_handle: Option<EmitterHandle>,

    /// Whether the emitter is currently active
    pub active: bool,
}

impl ParticleEmitterComponent {
    /// Create a new particle emitter component with default configuration.
    pub fn new() -> Self {
        Self {
            config: EmitterConfig::default(),
            emitter_handle: None,
            active: true,
        }
    }

    /// Create a new particle emitter component with specific configuration.
    pub fn with_config(config: EmitterConfig) -> Self {
        Self {
            config,
            emitter_handle: None,
            active: true,
        }
    }

    /// Create a new particle emitter component at a specific position.
    pub fn at_position(position: [f32; 3]) -> Self {
        let mut config = EmitterConfig::default();
        config.position = position;

        Self {
            config,
            emitter_handle: None,
            active: true,
        }
    }

    /// Create a fire effect emitter.
    pub fn fire_effect(position: [f32; 3]) -> Self {
        Self {
            config: EmitterConfig {
                position,
                emit_rate: 1000.0,
                base_lifetime: 2.0,
                lifetime_variation: 0.5,
                velocity_direction: [0.0, 1.0, 0.0],
                velocity_magnitude: 2.0,
                velocity_cone_angle: 0.3,
                base_scale: 0.15,
                scale_variation: 0.3,
                color: [1.0, 0.5, 0.0, 1.0], // Orange
                color_variation: 0.2,
                ..Default::default()
            },
            emitter_handle: None,
            active: true,
        }
    }

    /// Create a sparkle/magic effect emitter.
    pub fn sparkle_effect(position: [f32; 3]) -> Self {
        Self {
            config: EmitterConfig {
                position,
                emit_rate: 1000.0,
                base_lifetime: 3.0,
                lifetime_variation: 1.0,
                velocity_direction: [0.0, -1.0, 0.0], // Falling down
                velocity_magnitude: 0.5,
                velocity_cone_angle: 0.1,
                base_scale: 0.1,
                scale_variation: 0.5,
                color: [0.8, 0.9, 1.0, 1.0], // Light blue
                color_variation: 0.3,
                ..Default::default()
            },
            emitter_handle: None,
            active: true,
        }
    }

    /// Update emitter configuration.
    pub fn update_config(&mut self, config: EmitterConfig) {
        self.config = config;
    }

    /// Set whether the emitter is active.
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    /// Set the emitter position.
    pub fn set_position(&mut self, position: [f32; 3]) {
        self.config.position = position;
    }

    /// Set the emit rate (particles per second).
    pub fn set_emit_rate(&mut self, rate: f32) {
        self.config.emit_rate = rate;
    }

    /// Set the particle color.
    pub fn set_color(&mut self, color: [f32; 4]) {
        self.config.color = color;
    }

    /// Load configuration from a preset file.
    ///
    /// # Arguments
    /// * `name` - Preset name (filename without .json extension)
    ///
    /// # Errors
    /// Returns error if preset file not found or deserialization fails
    pub fn load_from_preset(&mut self, name: &str) -> Result<(), String> {
        let presets_dir = std::path::Path::new("assets/particles");
        let path = presets_dir.join(format!("{}.json", name));

        if !path.exists() {
            return Err(format!("Preset '{}' not found at {}", name, path.display()));
        }

        // Read and parse the preset file
        let json = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read preset file {}: {}", path.display(), e))?;

        let preset: katla_gfx::particles::EmitterPreset =
            serde_json::from_str(&json).map_err(|e| {
                format!(
                    "Failed to deserialize preset from {}: {}",
                    path.display(),
                    e
                )
            })?;

        self.config = preset.config;
        log::info!("Loaded particle preset '{}' for emitter", name);
        Ok(())
    }

    /// Save current configuration as a preset file.
    ///
    /// # Arguments
    /// * `name` - Preset name (will be saved as name.json)
    ///
    /// # Errors
    /// Returns error if file write or serialization fails
    pub fn save_as_preset(&self, name: &str) -> Result<(), String> {
        let preset = katla_gfx::particles::EmitterPreset::new(name.to_string(), self.config);
        let presets_dir = std::path::Path::new("assets/particles");
        let path = presets_dir.join(format!("{}.json", name));

        // Create directory if it doesn't exist
        if !presets_dir.exists() {
            std::fs::create_dir_all(presets_dir)
                .map_err(|e| format!("Failed to create presets directory: {}", e))?;
        }

        preset.save_to_file(&path)
    }
}

impl Default for ParticleEmitterComponent {
    fn default() -> Self {
        Self::new()
    }
}
