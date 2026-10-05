/// Ambient light (global illumination).
///
/// This is an ECS resource, not a component, since there's typically one ambient light per scene.
/// Register it with the world: `world.insert_resource(AmbientLight::default())`.
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
        Self::gray(0.1)
    }
}
