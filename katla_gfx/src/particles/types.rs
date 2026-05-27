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

// Safety: EmitterShape is repr(u32), guaranteed 4 bytes with no padding.
unsafe impl bytemuck::Pod for EmitterShape {}
unsafe impl bytemuck::Zeroable for EmitterShape {}

/// 16-byte aligned `[f32; 4]` to match WGSL `vec4f` alignment.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
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
    pub shape: EmitterShape,

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

    /// Target color at end of particle lifetime (linearly interpolated from `color`)
    #[serde(default = "default_color_end")]
    pub color_end: Align16Vec4,

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

    /// When non-zero, the simulate shader immediately kills all particles belonging to this emitter.
    #[serde(skip)]
    pub kill_all: u32,

    /// Scale multiplier at end of particle lifetime (1.0 = no change, 0.0 = shrink to nothing)
    #[serde(default = "default_scale_end")]
    pub scale_end: f32,

    #[serde(skip)]
    pub _pad2: [f32; 3],
}

// Safety: EmitterConfig is repr(C), all fields are Pod (EmitterShape is repr(u32), f32, u32, Align16Vec4).
// The 12 bytes of implicit padding between color_variation and color_end are never read uninitialized
// because the struct is always created via Default or explicit field init.
unsafe impl bytemuck::Pod for EmitterConfig {}
unsafe impl bytemuck::Zeroable for EmitterConfig {}

impl EmitterConfig {
    pub fn builder() -> EmitterConfigBuilder {
        EmitterConfigBuilder::new()
    }
}

pub struct EmitterConfigBuilder {
    config: EmitterConfig,
}

impl EmitterConfigBuilder {
    pub fn new() -> Self {
        Self {
            config: EmitterConfig::default(),
        }
    }

    pub fn position(mut self, x: f32, y: f32, z: f32) -> Self {
        self.config.position = [x, y, z];
        self
    }

    pub fn shape(mut self, shape: EmitterShape) -> Self {
        self.config.shape = shape;
        self
    }

    pub fn emit_rate(mut self, rate: f32) -> Self {
        self.config.emit_rate = rate;
        self
    }

    pub fn base_lifetime(mut self, lifetime: f32) -> Self {
        self.config.base_lifetime = lifetime;
        self
    }

    pub fn lifetime_variation(mut self, variation: f32) -> Self {
        self.config.lifetime_variation = variation;
        self
    }

    pub fn velocity_direction(mut self, x: f32, y: f32, z: f32) -> Self {
        self.config.velocity_direction = [x, y, z];
        self
    }

    pub fn velocity_magnitude(mut self, magnitude: f32) -> Self {
        self.config.velocity_magnitude = magnitude;
        self
    }

    pub fn velocity_cone_angle(mut self, angle: f32) -> Self {
        self.config.velocity_cone_angle = angle;
        self
    }

    pub fn base_scale(mut self, scale: f32) -> Self {
        self.config.base_scale = scale;
        self
    }

    pub fn scale_variation(mut self, variation: f32) -> Self {
        self.config.scale_variation = variation;
        self
    }

    pub fn color(mut self, r: f32, g: f32, b: f32, a: f32) -> Self {
        self.config.color = [r, g, b, a];
        self
    }

    pub fn color_variation(mut self, variation: f32) -> Self {
        self.config.color_variation = variation;
        self
    }

    pub fn shape_params(mut self, params: [f32; 4]) -> Self {
        self.config.shape_params = params;
        self
    }

    pub fn gravity(mut self, gravity: f32) -> Self {
        self.config.gravity = gravity;
        self
    }

    pub fn turbulence_strength(mut self, strength: f32) -> Self {
        self.config.turbulence_strength = strength;
        self
    }

    pub fn turbulence_frequency(mut self, frequency: f32) -> Self {
        self.config.turbulence_frequency = frequency;
        self
    }

    pub fn color_end(mut self, r: f32, g: f32, b: f32, a: f32) -> Self {
        self.config.color_end = Align16Vec4([r, g, b, a]);
        self
    }

    pub fn scale_end(mut self, scale: f32) -> Self {
        self.config.scale_end = scale;
        self
    }

    pub fn build(self) -> EmitterConfig {
        self.config
    }
}

impl Default for EmitterConfigBuilder {
    fn default() -> Self {
        Self::new()
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
fn default_color_end() -> Align16Vec4 {
    Align16Vec4([1.0, 1.0, 1.0, 1.0])
}
fn default_scale_end() -> f32 {
    1.0
}

impl Default for EmitterConfig {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            _pad_position: 0.0,
            shape: EmitterShape::Point,
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
            color_end: Align16Vec4([1.0, 1.0, 1.0, 1.0]),
            shape_params: [0.0; 4],
            gravity: -9.8,
            turbulence_strength: 0.0,
            turbulence_frequency: 3.0,
            kill_all: 0,
            scale_end: 1.0,
            _pad2: [0.0; 3],
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
pub(crate) struct EmitterState {
    /// Burst particles to emit this frame
    pub burst_count: u32,
    /// Accumulated fractional emit time for rate-based emission
    pub emit_accumulator: f32,
}
