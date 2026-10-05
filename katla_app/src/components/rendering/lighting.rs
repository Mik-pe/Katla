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
}

impl PointLight {
    /// Create a new point light
    pub fn new(color: [f32; 3], intensity: f32, range: f32) -> Self {
        Self {
            color,
            intensity,
            range,
        }
    }

    /// Create a white point light
    pub fn white(intensity: f32, range: f32) -> Self {
        Self::new([1.0, 1.0, 1.0], intensity, range)
    }
}

impl Default for PointLight {
    fn default() -> Self {
        Self::white(1.0, 10.0)
    }
}
