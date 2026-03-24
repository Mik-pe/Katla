//! Mathematical utility functions
//!
//! Provides commonly used mathematical utility functions.

use crate::Vec3;

/// Clamp a value between a minimum and maximum
/// If min > max, the bounds are swapped to handle inverted ranges
#[inline]
pub fn clamp(value: f32, min: f32, max: f32) -> f32 {
    let (min, max) = if min > max { (max, min) } else { (min, max) };
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

/// Compute axis-aligned bounding min/max from a set of 3D vertices.
/// Returns (min, max) where min/max are Vec3 of the component-wise extremes.
pub fn compute_bounds(verts: &[Vec3]) -> (Vec3, Vec3) {
    let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);

    for vert in verts {
        if vert[0] > max[0] {
            max[0] = vert[0];
        }
        if vert[1] > max[1] {
            max[1] = vert[1];
        }
        if vert[2] > max[2] {
            max[2] = vert[2];
        }
        if vert[0] < min[0] {
            min[0] = vert[0];
        }
        if vert[1] < min[1] {
            min[1] = vert[1];
        }
        if vert[2] < min[2] {
            min[2] = vert[2];
        }
    }

    (min, max)
}

/// Linear interpolation between two values
#[inline]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Smoothstep interpolation (smooth Hermite interpolation)
/// Returns 0 for x <= 0, 1 for x >= 1, and smooth interpolation in between
#[inline]
pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Smootherstep interpolation (smoother than smoothstep)
/// Uses Ken Perlin's improved smoothstep
#[inline]
pub fn smootherstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// Inverse smoothstep - finds the input that would produce the given smoothstep output
/// Solves y = 3t² - 2t³ for t using Newton-Raphson iteration
#[inline]
pub fn inverse_smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    // Clamp input to [0, 1]
    let y = clamp(x, 0.0, 1.0);

    // Initial guess using linear approximation
    let mut t = y;

    // Newton-Raphson iterations to solve y = 3t² - 2t³
    // f(t) = 3t² - 2t³ - y = 0
    // f'(t) = 6t - 6t² = 6t(1 - t)
    for _ in 0..4 {
        let t2 = t * t;
        let t3 = t2 * t;
        let f = 3.0 * t2 - 2.0 * t3 - y;
        let fp = 6.0 * t - 6.0 * t2;
        if fp.abs() < 1e-6 {
            break;
        }
        t -= f / fp;
        t = clamp(t, 0.0, 1.0);
    }

    // Map back to [edge0, edge1] range
    lerp(edge0, edge1, t)
}

/// Degrees to radians conversion
#[inline]
pub fn deg_to_rad(degrees: f32) -> f32 {
    degrees * crate::constants::DEG_TO_RAD
}

/// Radians to degrees conversion
#[inline]
pub fn rad_to_deg(radians: f32) -> f32 {
    radians * crate::constants::RAD_TO_DEG
}

/// Check if a value is approximately zero (within EPSILON threshold)
#[inline]
pub fn approx_zero(value: f32) -> bool {
    value.abs() < f32::EPSILON
}

/// Check if a value is approximately zero (within a custom threshold)
#[inline]
pub fn approx_zero_eps(value: f32, epsilon: f32) -> bool {
    value.abs() < epsilon
}

/// Check if two values are approximately equal (within EPSILON threshold)
#[inline]
pub fn approx_equal(a: f32, b: f32) -> bool {
    (a - b).abs() < f32::EPSILON
}

/// Check if two values are approximately equal (within a custom threshold)
#[inline]
pub fn approx_equal_eps(a: f32, b: f32, epsilon: f32) -> bool {
    (a - b).abs() < epsilon
}

/// Map a value from one range to another
#[inline]
pub fn map_range(value: f32, in_min: f32, in_max: f32, out_min: f32, out_max: f32) -> f32 {
    out_min + (value - in_min) / (in_max - in_min) * (out_max - out_min)
}

/// Remap a value from one range to another, clamping the result to the target range
#[inline]
pub fn remap_clamp(value: f32, in_min: f32, in_max: f32, out_min: f32, out_max: f32) -> f32 {
    let result = map_range(value, in_min, in_max, out_min, out_max);
    clamp(result, out_min, out_max)
}

/// Sign function - returns -1.0 if negative, 1.0 if positive, 0.0 if zero
#[inline]
pub fn sign(value: f32) -> f32 {
    if value > 0.0 {
        1.0
    } else if value < 0.0 {
        -1.0
    } else {
        0.0
    }
}

/// Absolute difference between two values
#[inline]
pub fn abs_diff(a: f32, b: f32) -> f32 {
    (a - b).abs()
}

/// Minimum of two values
#[inline]
pub fn min(a: f32, b: f32) -> f32 {
    a.min(b)
}

/// Maximum of two values
#[inline]
pub fn max(a: f32, b: f32) -> f32 {
    a.max(b)
}

/// Next power of two >= value
#[inline]
pub fn next_power_of_two(value: f32) -> f32 {
    if value <= 0.0 {
        return 1.0;
    }
    if value.is_infinite() {
        return value;
    }
    if value >= f32::MAX {
        return f32::MAX;
    }

    // For values that fit in i32, use integer arithmetic
    if value <= (i32::MAX as f32) {
        let val_int = value as i32;
        if val_int <= 0 {
            return 1.0;
        }
        let next = if val_int > 0 && (val_int & (val_int - 1)) == 0 {
            val_int
        } else {
            1i32 << (32 - val_int.leading_zeros())
        };
        return next as f32;
    }

    // For larger values, use bit manipulation on f32 representation
    let bits = value.to_bits();
    let exponent = ((bits >> 23) & 0xFF) as i32 - 127;

    // If already a power of two, return it
    let mantissa = bits & 0x007FFFFF;
    if mantissa == 0 {
        return value;
    }

    // Next power of two has mantissa = 0 and exponent + 1
    let new_exponent = exponent + 1;
    if new_exponent >= 128 {
        f32::MAX
    } else {
        f32::from_bits(((new_exponent + 127) as u32) << 23)
    }
}

/// Previous power of two <= value
#[inline]
pub fn prev_power_of_two(value: f32) -> f32 {
    if value <= 1.0 {
        return 1.0;
    }
    if value.is_infinite() {
        return f32::MAX;
    }

    // For values that fit in i32, use integer arithmetic
    if value <= (i32::MAX as f32) {
        let val_int = value as i32;
        if val_int <= 1 {
            return 1.0;
        }
        let prev = 1i32 << (31 - val_int.leading_zeros());
        return prev as f32;
    }

    // For larger values, use bit manipulation on f32 representation
    let bits = value.to_bits();
    let exponent = ((bits >> 23) & 0xFF) as i32 - 127;

    // If already a power of two, return it
    let mantissa = bits & 0x007FFFFF;
    if mantissa == 0 {
        return value;
    }

    // Previous power of two has mantissa = 0 and same exponent
    f32::from_bits(((exponent + 127) as u32) << 23)
}

/// Check if a value is a power of two
#[inline]
pub fn is_power_of_two(value: f32) -> bool {
    if value <= 0.0 || !value.is_finite() {
        return false;
    }

    let bits = value.to_bits();
    // A power of two has only the exponent bit set (mantissa = 0)
    let mantissa = bits & 0x007FFFFF;
    mantissa == 0 && bits != 0
}

/// Round to nearest integer
#[inline]
pub fn round(value: f32) -> f32 {
    value.round()
}

/// Round up to nearest integer
#[inline]
pub fn ceil(value: f32) -> f32 {
    value.ceil()
}

/// Round down to nearest integer
#[inline]
pub fn floor(value: f32) -> f32 {
    value.floor()
}

/// Truncate towards zero
#[inline]
pub fn trunc(value: f32) -> f32 {
    value.trunc()
}

/// Fractional part of a value
#[inline]
pub fn fract(value: f32) -> f32 {
    value.abs() % 1.0
}

/// Modulo operation (remainder with same sign as dividend)
#[inline]
pub fn mod_f32(a: f32, b: f32) -> f32 {
    a % b
}

/// Safe division that returns 0 for division by zero
#[inline]
pub fn safe_div(a: f32, b: f32) -> f32 {
    if b.abs() < f32::EPSILON { 0.0 } else { a / b }
}

/// Reciprocal (1/x) with protection against division by zero
#[inline]
pub fn reciprocal(value: f32) -> f32 {
    if value.abs() < f32::EPSILON {
        0.0
    } else {
        1.0 / value
    }
}

/// Square root with protection for negative values (returns 0)
#[inline]
pub fn safe_sqrt(value: f32) -> f32 {
    if value <= 0.0 { 0.0 } else { value.sqrt() }
}

/// Check if a value is finite (not NaN or infinity)
#[inline]
pub fn is_finite(value: f32) -> bool {
    value.is_finite()
}

/// Check if a value is NaN (not a number)
#[inline]
pub fn is_nan(value: f32) -> bool {
    value.is_nan()
}

/// Saturating addition - clamps result to f32 range
/// Returns f32::MAX on positive overflow, f32::MIN on negative overflow
#[inline]
pub fn saturating_add(a: f32, b: f32) -> f32 {
    let result = a + b;

    // Only clamp if overflow actually occurred (result is infinity)
    if result.is_infinite() {
        if a > 0.0 && b > 0.0 {
            return f32::MAX;
        } else if a < 0.0 && b < 0.0 {
            return f32::MIN;
        }
    }

    result
}

/// Saturating subtraction - clamps result to f32 range
/// Returns f32::MAX on positive overflow, f32::MIN on negative overflow
#[inline]
pub fn saturating_sub(a: f32, b: f32) -> f32 {
    let result = a - b;

    // Only clamp if overflow actually occurred (result is infinity)
    if result.is_infinite() {
        if a > 0.0 && b < 0.0 {
            return f32::MAX;
        } else if a < 0.0 && b > 0.0 {
            return f32::MIN;
        }
    }

    result
}

/// Saturating multiplication - clamps result to f32 range
#[inline]
pub fn saturating_mul(a: f32, b: f32) -> f32 {
    (a * b).clamp(f32::MIN, f32::MAX)
}

/// Fast inverse square root (Quake III algorithm)
/// Returns 1/sqrt(x) - useful for normalization
#[inline]
pub fn fast_inverse_sqrt(x: f32) -> f32 {
    let xhalf = x * 0.5;
    let mut i = x.to_bits();
    i = 0x5f3759df - (i >> 1);
    let mut y = f32::from_bits(i);
    y = y * (1.5 - (xhalf * y * y));
    y
}

/// Fast square root using fast inverse sqrt
#[inline]
pub fn fast_sqrt(x: f32) -> f32 {
    if x <= 0.0 {
        0.0
    } else {
        x * fast_inverse_sqrt(x)
    }
}
