use serde::{Deserialize, Serialize};

/// Emitter shape for particle spawn positions.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum EmitterShape {
    #[default]
    Point = 0,
    Line = 1,
    Circle = 2,
    Sphere = 3,
    Box = 4,
}

impl EmitterShape {
    /// Convert to u32 for GPU
    pub fn as_u32(self) -> u32 {
        self as u32
    }

    /// Convert from u32 from GPU
    pub fn from_u32(val: u32) -> Self {
        match val {
            0 => EmitterShape::Point,
            1 => EmitterShape::Line,
            2 => EmitterShape::Circle,
            3 => EmitterShape::Sphere,
            4 => EmitterShape::Box,
            _ => EmitterShape::Point,
        }
    }
}

/// 16-byte aligned `[f32; 4]` to match WGSL `vec4f` alignment.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Default)]
pub struct Align16Vec4(pub [f32; 4]);

// Safety: Align16Vec4 is repr(C) with align(16), contains only f32 (Pod).
unsafe impl bytemuck::Pod for Align16Vec4 {}
unsafe impl bytemuck::Zeroable for Align16Vec4 {}

/// Per-emitter configuration uploaded to a GPU storage buffer.
///
/// Must match WGSL `EmitterConfig` exactly. WGSL `vec3f` has 16-byte alignment
/// while Rust `[f32; 3]` has 4-byte alignment in `repr(C)`, so explicit padding
/// fields bridge the gap.
#[repr(C)]
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct EmitterConfig {
    #[serde(default = "default_position")]
    pub position: [f32; 3],

    #[serde(skip)]
    pub _pad_position: f32,

    #[serde(default)]
    pub shape: u32,

    #[serde(default = "default_emit_rate")]
    pub emit_rate: f32,

    #[serde(default = "default_base_lifetime")]
    pub base_lifetime: f32,

    /// Random variation in lifetime (±percentage)
    #[serde(default = "default_lifetime_variation")]
    pub lifetime_variation: f32,

    #[serde(default = "default_velocity_direction")]
    pub velocity_direction: [f32; 3],

    #[serde(skip)]
    pub _pad_velocity: f32,

    #[serde(default = "default_velocity_magnitude")]
    pub velocity_magnitude: f32,

    /// Velocity spread cone angle (0 = straight, PI/2 = hemisphere)
    #[serde(default = "default_velocity_cone_angle")]
    pub velocity_cone_angle: f32,

    #[serde(default = "default_base_scale")]
    pub base_scale: f32,

    /// Scale variation (±percentage)
    #[serde(default = "default_scale_variation")]
    pub scale_variation: f32,

    #[serde(default = "default_color")]
    pub color: [f32; 4],

    /// Color variation (±percentage per channel)
    #[serde(default = "default_color_variation")]
    pub color_variation: f32,

    #[serde(skip)]
    pub _pad_color: Align16Vec4,

    /// Shape parameters (length/radius for Line/Circle/Sphere, dimensions for Box)
    #[serde(default)]
    pub shape_params: [f32; 4],

    /// Gravity acceleration applied each frame (negative = downward, 0 = none, positive = upward)
    #[serde(default)]
    pub gravity: f32,

    /// Turbulence strength (amplitude of sinusoidal force applied perpendicular to velocity)
    #[serde(default)]
    pub turbulence_strength: f32,

    /// Turbulence frequency (how fast the sine wave oscillates)
    #[serde(default = "default_turbulence_frequency")]
    pub turbulence_frequency: f32,
}

// Safety: EmitterConfig is repr(C), all fields are Pod or padding from Align16Vec4 alignment.
// The 12 bytes of padding between color_variation and _pad_color are never read uninitialized
// because the struct is always created via Default or explicit field init.
unsafe impl bytemuck::Pod for EmitterConfig {}
unsafe impl bytemuck::Zeroable for EmitterConfig {}

impl EmitterConfig {
    /// Get the emitter shape as an enum
    pub fn get_shape(&self) -> EmitterShape {
        EmitterShape::from_u32(self.shape)
    }

    /// Set the emitter shape from an enum
    pub fn set_shape(&mut self, shape: EmitterShape) {
        self.shape = shape.as_u32();
    }
}

// Serde default functions
fn default_position() -> [f32; 3] {
    [0.0; 3]
}
fn default_emit_rate() -> f32 {
    50.0
}
fn default_base_lifetime() -> f32 {
    5.0
}
fn default_lifetime_variation() -> f32 {
    0.2
}
fn default_velocity_direction() -> [f32; 3] {
    [0.0, 1.0, 0.0]
}
fn default_velocity_magnitude() -> f32 {
    1.0
}
fn default_velocity_cone_angle() -> f32 {
    0.5
}
fn default_base_scale() -> f32 {
    0.1
}
fn default_scale_variation() -> f32 {
    0.5
}
fn default_color() -> [f32; 4] {
    [1.0, 1.0, 1.0, 1.0]
}
fn default_color_variation() -> f32 {
    0.1
}
fn default_turbulence_frequency() -> f32 {
    3.0
}

impl Default for EmitterConfig {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            _pad_position: 0.0,
            shape: EmitterShape::Point.as_u32(),
            emit_rate: 50.0,
            base_lifetime: 5.0,
            lifetime_variation: 0.2,
            velocity_direction: [0.0, 1.0, 0.0],
            _pad_velocity: 0.0,
            velocity_magnitude: 1.0,
            velocity_cone_angle: 0.5,
            base_scale: 0.1,
            scale_variation: 0.5,
            color: [1.0, 1.0, 1.0, 1.0],
            color_variation: 0.1,
            _pad_color: Align16Vec4([0.0; 4]),
            shape_params: [0.0; 4],
            gravity: -9.8,
            turbulence_strength: 0.0,
            turbulence_frequency: 3.0,
        }
    }
}

/// Handle to an emitter in the global particle system.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EmitterHandle {
    index: u32,
}

impl EmitterHandle {
    /// Invalid emitter handle
    pub const NONE: Self = Self { index: u32::MAX };

    /// Create a new emitter handle from index
    pub fn new(index: u32) -> Self {
        Self { index }
    }

    /// Get the emitter index
    pub fn index(&self) -> u32 {
        self.index
    }
}

/// Per-emitter runtime state (not uploaded to GPU).
#[derive(Clone, Default)]
pub(super) struct EmitterState {
    /// Burst particles to emit this frame
    pub burst_count: u32,
    /// Accumulated fractional emit time for rate-based emission
    pub emit_accumulator: f32,
}
