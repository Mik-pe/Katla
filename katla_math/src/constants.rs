//! Mathematical constants
//!
//! Provides commonly used mathematical constants for graphics and physics applications.

/// Pi (π) - ratio of circle circumference to diameter
pub const PI: f32 = core::f32::consts::PI;

/// 2π - full circle in radians
pub const TAU: f32 = 2.0 * PI;

/// π/2 - quarter circle in radians (90 degrees)
pub const FRAC_PI_2: f32 = core::f32::consts::FRAC_PI_2;

/// π/3 - 60 degrees in radians
pub const FRAC_PI_3: f32 = core::f32::consts::FRAC_PI_3;

/// π/4 - 45 degrees in radians
pub const FRAC_PI_4: f32 = core::f32::consts::FRAC_PI_4;

/// π/6 - 30 degrees in radians
pub const FRAC_PI_6: f32 = core::f32::consts::FRAC_PI_6;

/// Degrees to radians conversion factor
pub const DEG_TO_RAD: f32 = PI / 180.0;

/// Radians to degrees conversion factor
pub const RAD_TO_DEG: f32 = 180.0 / PI;

/// Golden ratio (φ) - not available in core::f32::consts
pub const GOLDEN_RATIO: f32 = 1.618_034;

/// Square root of 3 - not available in core::f32::consts
pub const SQRT_3: f32 = 1.732_050_8;
