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

    /// Immediately kill all living particles when this emitter is destroyed
    pub kill_on_destroy: bool,
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
            kill_on_destroy: false,
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
            kill_on_destroy: false,
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
            kill_on_destroy: false,
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

    /// Burst particles immediately.
    ///
    /// Queues particles to be emitted immediately this frame, overriding
    /// the normal emit rate. Useful for explosions, impacts, and one-shot effects.
    ///
    /// # Arguments
    /// * `count` - Number of particles to burst
    ///
    /// # Example
    /// ```
    /// use katla_app::components::rendering::particle::ParticleEmitterComponent;
    ///
    /// let mut explosion_emitter = ParticleEmitterComponent::new();
    /// explosion_emitter.burst(1000);
    /// assert_eq!(explosion_emitter.burst_queue.len(), 1);
    /// assert_eq!(explosion_emitter.burst_queue[0], 1000);
    ///
    /// let mut impact_emitter = ParticleEmitterComponent::new();
    /// impact_emitter.burst(50);
    /// assert_eq!(impact_emitter.burst_queue[0], 50);
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
    /// ```
    /// use katla_app::components::rendering::particle::ParticleEmitterComponent;
    ///
    /// let mut spell_emitter = ParticleEmitterComponent::new();
    /// spell_emitter.emit_for(2.0);
    /// assert_eq!(spell_emitter.timed_emission, Some(2.0));
    /// assert!(spell_emitter.active);
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

    /// Set line emitter shape (Y-axis).
    ///
    /// Particles spawn along a vertical line. Useful for rain, beams, etc.
    ///
    /// # Arguments
    /// * `length` - Length of the line in world units
    ///
    /// # Example
    /// ```
    /// use katla_app::components::rendering::particle::ParticleEmitterComponent;
    ///
    /// let mut rain_emitter = ParticleEmitterComponent::new();
    /// rain_emitter.with_line_shape(10.0); // 10-unit vertical line (Y-axis)
    /// assert_eq!(rain_emitter.config.shape_params[0], 10.0);
    /// ```
    pub fn with_line_shape(&mut self, length: f32) -> &mut Self {
        self.config.set_shape(EmitterShape::Line);
        self.config.shape_params = [length, 0.0, 0.0, 0.0];
        log::debug!("Set line shape with length {}", length);
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
    /// ```
    /// use katla_app::components::rendering::particle::ParticleEmitterComponent;
    ///
    /// let mut aura_emitter = ParticleEmitterComponent::new();
    /// aura_emitter.with_circle_shape(2.0); // 2-unit radius circle
    /// assert_eq!(aura_emitter.config.shape_params[0], 2.0);
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
    /// ```
    /// use katla_app::components::rendering::particle::ParticleEmitterComponent;
    ///
    /// let mut explosion_emitter = ParticleEmitterComponent::new();
    /// explosion_emitter.with_sphere_shape(5.0); // 5-unit radius sphere
    /// assert_eq!(explosion_emitter.config.shape_params[0], 5.0);
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
    /// ```
    /// use katla_app::components::rendering::particle::ParticleEmitterComponent;
    ///
    /// let mut volume_emitter = ParticleEmitterComponent::new();
    /// volume_emitter.with_box_shape(4.0, 3.0, 2.0); // 4x3x2 box
    /// assert_eq!(volume_emitter.config.shape_params[0], 4.0);
    /// assert_eq!(volume_emitter.config.shape_params[1], 3.0);
    /// assert_eq!(volume_emitter.config.shape_params[2], 2.0);
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
