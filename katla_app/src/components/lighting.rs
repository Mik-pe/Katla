use katla_ecs::Component;
use katla_math::Vec3;

/// Directional light (like the sun).
///
/// Lights objects from a specific direction with parallel rays.
/// Has no position, only direction.
#[derive(Component, Debug, Copy, Clone)]
pub struct DirectionalLight {
    /// Direction the light is shining (normalized)
    pub direction: Vec3,
    /// RGB color of the light (0.0 - 1.0)
    pub color: [f32; 3],
    /// Intensity multiplier
    pub intensity: f32,
}

impl DirectionalLight {
    /// Create a new directional light
    pub fn new(direction: Vec3, color: [f32; 3], intensity: f32) -> Self {
        Self {
            direction: direction.normalize(),
            color,
            intensity,
        }
    }

    /// Create a white light from a direction
    pub fn white(direction: Vec3) -> Self {
        Self::new(direction, [1.0, 1.0, 1.0], 1.0)
    }
}

impl Default for DirectionalLight {
    fn default() -> Self {
        Self::white(Vec3::new(0.0, -1.0, 0.0)) // Shining straight down
    }
}

/// Point light (like a light bulb).
///
/// Emits light in all directions from a specific point.
#[derive(Component, Debug, Copy, Clone)]
pub struct PointLight {
    /// RGB color of the light (0.0 - 1.0)
    pub color: [f32; 3],
    /// Intensity multiplier
    pub intensity: f32,
    /// Distance at which light intensity drops to zero
    pub range: f32,
    /// Constant attenuation factor
    pub constant: f32,
    /// Linear attenuation factor
    pub linear: f32,
    /// Quadratic attenuation factor
    pub quadratic: f32,
}

impl PointLight {
    /// Create a new point light
    pub fn new(color: [f32; 3], intensity: f32, range: f32) -> Self {
        Self {
            color,
            intensity,
            range,
            // Default attenuation for realistic falloff
            constant: 1.0,
            linear: 0.09,
            quadratic: 0.032,
        }
    }

    /// Create a white point light
    pub fn white(intensity: f32, range: f32) -> Self {
        Self::new([1.0, 1.0, 1.0], intensity, range)
    }

    /// Calculate attenuation at a given distance
    pub fn attenuation(&self, distance: f32) -> f32 {
        let d = distance.min(self.range);
        1.0 / (self.constant + self.linear * d + self.quadratic * d * d)
    }
}

impl Default for PointLight {
    fn default() -> Self {
        Self::white(1.0, 10.0)
    }
}

/// Spot light (like a flashlight).
///
/// Emits light in a cone from a specific position and direction.
#[derive(Component, Debug, Copy, Clone)]
pub struct SpotLight {
    /// RGB color of the light (0.0 - 1.0)
    pub color: [f32; 3],
    /// Intensity multiplier
    pub intensity: f32,
    /// Direction the light is shining (normalized)
    pub direction: Vec3,
    /// Distance at which light intensity drops to zero
    pub range: f32,
    /// Angle of the light cone (in radians)
    pub cutoff_angle: f32,
    /// Falloff between inner and outer cone
    pub outer_cutoff: f32,
    /// Constant attenuation factor
    pub constant: f32,
    /// Linear attenuation factor
    pub linear: f32,
    /// Quadratic attenuation factor
    pub quadratic: f32,
}

impl SpotLight {
    /// Create a new spot light
    pub fn new(
        color: [f32; 3],
        intensity: f32,
        direction: Vec3,
        range: f32,
        cutoff_angle: f32,
    ) -> Self {
        Self {
            color,
            intensity,
            direction: direction.normalize(),
            range,
            cutoff_angle,
            outer_cutoff: cutoff_angle + 0.1, // Slightly wider soft edge
            constant: 1.0,
            linear: 0.09,
            quadratic: 0.032,
        }
    }

    /// Create a white spot light
    pub fn white(intensity: f32, direction: Vec3, range: f32, cutoff_angle: f32) -> Self {
        Self::new([1.0, 1.0, 1.0], intensity, direction, range, cutoff_angle)
    }

    /// Calculate cosine of cutoff angle (for shader)
    pub fn cutoff_cos(&self) -> f32 {
        self.cutoff_angle.cos()
    }

    /// Calculate cosine of outer cutoff angle (for shader)
    pub fn outer_cutoff_cos(&self) -> f32 {
        self.outer_cutoff.cos()
    }
}

impl Default for SpotLight {
    fn default() -> Self {
        Self::white(1.0, Vec3::new(0.0, -1.0, 0.0), 10.0, std::f32::consts::FRAC_PI_4)
    }
}

/// Ambient light (global illumination).
///
/// This is a resource, not a component, since there's typically one ambient light per scene.
#[derive(Debug, Copy, Clone)]
pub struct AmbientLight {
    /// RGB color of the ambient light (0.0 - 1.0)
    pub color: [f32; 3],
    /// Intensity multiplier
    pub intensity: f32,
}

impl AmbientLight {
    /// Create a new ambient light
    pub fn new(color: [f32; 3], intensity: f32) -> Self {
        Self { color, intensity }
    }

    /// Create a gray ambient light
    pub fn gray(intensity: f32) -> Self {
        Self::new([intensity; 3], intensity)
    }
}

impl Default for AmbientLight {
    fn default() -> Self {
        Self::gray(0.1) // 10% gray ambient
    }
}
