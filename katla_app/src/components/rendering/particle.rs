use katla_ecs::Component;
use katla_gfx::particles::{EmitterConfig, EmitterHandle, EmitterShape};

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

    /// Queue of burst counts to emit (particles to emit immediately)
    pub burst_queue: Vec<u32>,

    /// Timed emission duration (remaining seconds). None = infinite
    pub timed_emission: Option<f32>,
}

impl ParticleEmitterComponent {
    /// Create a new particle emitter component with default configuration.
    pub fn new() -> Self {
        Self {
            config: EmitterConfig::default(),
            emitter_handle: None,
            active: true,
            burst_queue: Vec::new(),
            timed_emission: None,
        }
    }

    /// Create a new particle emitter component with specific configuration.
    pub fn with_config(config: EmitterConfig) -> Self {
        Self {
            config,
            emitter_handle: None,
            active: true,
            burst_queue: Vec::new(),
            timed_emission: None,
        }
    }

    /// Create a new particle emitter component at a specific position.
    pub fn at_position(position: [f32; 3]) -> Self {
        let config = EmitterConfig {
            position,
            ..Default::default()
        };

        Self {
            config,
            emitter_handle: None,
            active: true,
            burst_queue: Vec::new(),
            timed_emission: None,
        }
    }

    /// Create a fire effect emitter.
    pub fn fire_effect(position: [f32; 3]) -> Self {
        Self {
            config: EmitterConfig {
                position,
                emit_rate: 1500.0,
                base_lifetime: 2.5,
                lifetime_variation: 0.3,
                velocity_direction: [0.0, 1.0, 0.0],
                velocity_magnitude: 8.0,
                velocity_cone_angle: 0.05,
                base_scale: 0.08,
                scale_variation: 0.2,
                color: [1.0, 0.5, 0.0, 1.0], // Orange
                color_variation: 0.1,
                gravity: 0.0,
                ..Default::default()
            },
            emitter_handle: None,
            active: true,
            burst_queue: Vec::new(),
            timed_emission: None,
        }
    }

    /// Create an ethereal/spiritual particle effect with wavy turbulence.
    ///
    /// Particles spawn from a circle plane and rise with sinusoidal wave motion.
    pub fn ethereal_effect(position: [f32; 3]) -> Self {
        let mut component = Self {
            config: EmitterConfig {
                position,
                emit_rate: 800.0,
                base_lifetime: 4.0,
                lifetime_variation: 0.5,
                velocity_direction: [0.0, 1.0, 0.0],
                velocity_magnitude: 1.5,
                velocity_cone_angle: 0.1,
                base_scale: 0.12,
                scale_variation: 0.4,
                color: [0.6, 0.8, 1.0, 0.8], // Pale blue-white
                color_variation: 0.2,
                gravity: -0.5,
                turbulence_strength: 4.0,
                turbulence_frequency: 3.0,
                ..Default::default()
            },
            emitter_handle: None,
            active: true,
            burst_queue: Vec::new(),
            timed_emission: None,
        };
        component.with_circle_shape(2.0);
        component
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
            burst_queue: Vec::new(),
            timed_emission: None,
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

    /// Burst particles immediately.
    ///
    /// Queues particles to be emitted immediately this frame, overriding
    /// the normal emit rate. Useful for explosions, impacts, and one-shot effects.
    ///
    /// # Arguments
    /// * `count` - Number of particles to burst
    ///
    /// # Example
    /// ```ignore
    /// // Explosion effect
    /// explosion_emitter.burst(1000);
    ///
    /// // Bullet impact
    /// impact_emitter.burst(50);
    /// ```
    pub fn burst(&mut self, count: u32) {
        self.burst_queue.push(count);
        log::debug!("Queued burst of {} particles", count);
    }

    /// Emit particles for a limited duration.
    ///
    /// Sets a timer for the emitter to automatically deactivate after the
    /// specified duration. Useful for temporary effects.
    ///
    /// # Arguments
    /// * `duration` - Duration in seconds to emit particles
    ///
    /// # Example
    /// ```ignore
    /// // Spell effect that lasts 2 seconds
    /// spell_emitter.emit_for(2.0);
    /// ```
    pub fn emit_for(&mut self, duration: f32) {
        self.timed_emission = Some(duration);
        self.active = true;
        log::debug!("Set timed emission for {} seconds", duration);
    }

    /// Set point emitter shape (default).
    ///
    /// All particles spawn from a single point at the emitter position.
    pub fn with_point_shape(&mut self) -> &mut Self {
        self.config.set_shape(EmitterShape::Point);
        self.config.shape_params = [0.0; 4];
        self
    }

    /// Set line emitter shape.
    ///
    /// Particles spawn along a line. Useful for rain, beams, etc.
    ///
    /// # Arguments
    /// * `length` - Length of the line in world units
    /// * `axis` - Axis direction (0=X, 1=Y, 2=Z)
    ///
    /// # Example
    /// ```ignore
    /// rain_emitter.with_line_shape(10.0, 1); // 10-unit vertical line (Y-axis)
    /// ```
    pub fn with_line_shape(&mut self, length: f32, axis: u32) -> &mut Self {
        self.config.set_shape(EmitterShape::Line);
        self.config.shape_params = [length, 0.0, 0.0, 0.0];
        // Note: Shader currently only supports Y-axis lines
        // For other axes, you'd need to modify the shader sampling logic
        log::debug!("Set line shape with length {} on axis {}", length, axis);
        self
    }

    /// Set circle emitter shape.
    ///
    /// Particles spawn in a circle on the XZ plane. Useful for area effects.
    ///
    /// # Arguments
    /// * `radius` - Radius of the circle in world units
    ///
    /// # Example
    /// ```ignore
    /// aura_emitter.with_circle_shape(2.0); // 2-unit radius circle
    /// ```
    pub fn with_circle_shape(&mut self, radius: f32) -> &mut Self {
        self.config.set_shape(EmitterShape::Circle);
        self.config.shape_params = [radius, 0.0, 0.0, 0.0];
        log::debug!("Set circle shape with radius {}", radius);
        self
    }

    /// Set sphere emitter shape.
    ///
    /// Particles spawn within a sphere volume. Useful for explosions.
    ///
    /// # Arguments
    /// * `radius` - Radius of the sphere in world units
    ///
    /// # Example
    /// ```ignore
    /// explosion_emitter.with_sphere_shape(5.0); // 5-unit radius sphere
    /// ```
    pub fn with_sphere_shape(&mut self, radius: f32) -> &mut Self {
        self.config.set_shape(EmitterShape::Sphere);
        self.config.shape_params = [radius, 0.0, 0.0, 0.0];
        log::debug!("Set sphere shape with radius {}", radius);
        self
    }

    /// Set box emitter shape.
    ///
    /// Particles spawn within a box volume. Useful for volumes and areas.
    ///
    /// # Arguments
    /// * `width` - Width of the box (X-axis) in world units
    /// * `height` - Height of the box (Y-axis) in world units
    /// * `depth` - Depth of the box (Z-axis) in world units
    ///
    /// # Example
    /// ```ignore
    /// volume_emitter.with_box_shape(4.0, 3.0, 2.0); // 4x3x2 box
    /// ```
    pub fn with_box_shape(&mut self, width: f32, height: f32, depth: f32) -> &mut Self {
        self.config.set_shape(EmitterShape::Box);
        self.config.shape_params = [width, height, depth, 0.0];
        log::debug!(
            "Set box shape with dimensions {}x{}x{}",
            width,
            height,
            depth
        );
        self
    }
}

impl Default for ParticleEmitterComponent {
    fn default() -> Self {
        Self::new()
    }
}
