/// Interpolation methods for animation keyframes.
///
/// Defines how to interpolate between animation keyframes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Interpolation {
    /// Linear interpolation between keyframes
    ///
    /// Most common interpolation method. Good for most animations.
    #[default]
    Linear,

    /// Step interpolation (no blending)
    ///
    /// Instantly transitions between keyframes. Used for discrete animations.
    Step,

    /// Cubic spline interpolation
    ///
    /// Smooth interpolation with tangents. Highest quality but more expensive.
    CubicSpline,
}

impl Interpolation {
    /// Parse a GLTF interpolation string
    pub fn from_gltf(value: &str) -> Self {
        match value {
            "LINEAR" => Interpolation::Linear,
            "STEP" => Interpolation::Step,
            "CUBICSPLINE" => Interpolation::CubicSpline,
            _ => {
                eprintln!(
                    "Unknown interpolation type: {}, defaulting to LINEAR",
                    value
                );
                Interpolation::Linear
            }
        }
    }

    /// Convert to GLTF string representation
    pub fn to_gltf(&self) -> &'static str {
        match self {
            Interpolation::Linear => "LINEAR",
            Interpolation::Step => "STEP",
            Interpolation::CubicSpline => "CUBICSPLINE",
        }
    }
}
