use katla_math::Vec2;

use super::descriptor::Callback;

/// Easing functions that map a normalized time value t ∈ [0, 1] → [0, 1].
#[derive(Clone, Debug)]
pub enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    CubicBezier(f32, f32, f32, f32),
    Spring { stiffness: f32, damping: f32 },
}

impl Easing {
    /// Apply this easing function to a normalized time value.
    /// For spring easing, `t` is treated as elapsed time in seconds (not 0..1).
    pub fn apply(&self, t: f32) -> f32 {
        match self {
            Easing::Linear => t,
            Easing::EaseIn => t * t * t,
            Easing::EaseOut => {
                let t1 = t - 1.0;
                t1 * t1 * t1 + 1.0
            }
            Easing::EaseInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    let t1 = 2.0 * t - 2.0;
                    0.5 * t1 * t1 * t1 + 1.0
                }
            }
            Easing::CubicBezier(x1, y1, x2, y2) => cubic_bezier_ease(*x1, *y1, *x2, *y2, t),
            Easing::Spring { stiffness, damping } => {
                // For spring, t is elapsed time in seconds.
                // Simple damped spring: critically damped approximation.
                let omega = (*stiffness).sqrt();
                let zeta = *damping / (2.0 * omega);
                if zeta < 1.0 {
                    // Underdamped
                    let omega_d = omega * (1.0 - zeta * zeta).sqrt();
                    let exp_term = (-zeta * omega * t).exp();
                    let result = 1.0
                        - exp_term
                            * ((omega_d * t).cos() + zeta * omega / omega_d * (omega_d * t).sin());
                    result.clamp(0.0, 1.2) // Allow slight overshoot
                } else {
                    // Critically/overdamped
                    let exp_term = (-omega * t).exp();
                    let result = 1.0 - exp_term * (1.0 + omega * t);
                    result.clamp(0.0, 1.0)
                }
            }
        }
    }
}

/// Approximate cubic bezier easing using Newton's method on the x-curve.
fn cubic_bezier_ease(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    let x = t.clamp(0.0, 1.0);

    // Newton-Raphson to solve bezier_x(s) = x
    let mut s = x;
    for _ in 0..8 {
        let bs = bezier_coord(s, x1, x2);
        let bs_prime = bezier_coord_derivative(s, x1, x2);
        if bs_prime.abs() < 1e-6 {
            break;
        }
        s -= (bs - x) / bs_prime;
        s = s.clamp(0.0, 1.0);
    }

    bezier_coord(s, y1, y2)
}

/// Evaluate one coordinate of a cubic bezier curve with control points (p1, p2).
/// Curve goes from (0,0) to (1,1) with control points at (p1, p1) and (p2, p2)
/// for each axis independently.
fn bezier_coord(t: f32, p1: f32, p2: f32) -> f32 {
    let t2 = 1.0 - t;
    3.0 * t2 * t2 * t * p1 + 3.0 * t2 * t * t * p2 + t * t * t
}

/// Derivative of bezier_coord with respect to t.
fn bezier_coord_derivative(t: f32, p1: f32, p2: f32) -> f32 {
    let t2 = 1.0 - t;
    3.0 * t2 * t2 * p1 + 6.0 * t2 * t * (p2 - 2.0 * p1) + 3.0 * t * t * (1.0 - 2.0 * p2 + p1)
}

/// A tween from one value to another over time.
#[derive(Clone, Debug)]
pub struct Tween {
    pub from: f32,
    pub to: f32,
    pub duration: f64,
    pub easing: Easing,
}

impl Tween {
    /// Compute the interpolated value at the given elapsed time.
    pub fn value_at(&self, elapsed: f64) -> f32 {
        let t = if self.duration > 0.0 {
            (elapsed / self.duration) as f32
        } else {
            1.0
        };

        let t = t.clamp(0.0, 1.0);
        let eased_t = self.easing.apply(t);

        self.from + (self.to - self.from) * eased_t
    }
}

/// Properties that can be animated on a view node.
#[derive(Clone, Copy, Debug)]
pub enum AnimatedProperty {
    Opacity,
    OffsetX,
    OffsetY,
    Scale,
    CornerRadius,
}

/// An active animation instance stored on a ViewNode.
#[derive(Clone, Debug)]
pub struct Animation {
    pub property: AnimatedProperty,
    pub tween: Tween,
    pub start_time: f64,
    pub on_complete: Option<Callback>,
}

impl Animation {
    /// Returns true if this animation has completed.
    pub fn is_complete(&self, current_time: f64) -> bool {
        let elapsed = current_time - self.start_time;
        elapsed >= self.tween.duration
    }

    /// Compute the interpolated value at the given time.
    pub fn value_at(&self, current_time: f64) -> f32 {
        let elapsed = current_time - self.start_time;
        self.tween.value_at(elapsed)
    }

    pub fn property(&self) -> AnimatedProperty {
        self.property
    }

    pub fn on_complete_id(&self) -> Option<u32> {
        self.on_complete.as_ref().map(|cb| cb.0)
    }
}

/// A single keyframe in a keyframe animation.
#[derive(Clone, Debug)]
pub struct Keyframe {
    /// Normalized time position, 0.0..1.0.
    pub time: f32,
    /// Value at this keyframe.
    pub value: f32,
    /// Easing into this keyframe from the previous one.
    pub easing: Easing,
}

/// A multi-stop keyframe animation.
#[derive(Clone, Debug)]
pub struct KeyframeAnimation {
    pub property: AnimatedProperty,
    /// Keyframes sorted by time (ascending).
    pub keyframes: Vec<Keyframe>,
    pub duration: f64,
    pub start_time: f64,
    pub on_complete: Option<Callback>,
}

impl KeyframeAnimation {
    /// Returns true if this animation has completed.
    pub fn is_complete(&self, current_time: f64) -> bool {
        let elapsed = current_time - self.start_time;
        elapsed >= self.duration
    }

    /// Compute the interpolated value at the given time.
    pub fn value_at(&self, current_time: f64) -> f32 {
        let elapsed = current_time - self.start_time;
        if self.keyframes.is_empty() {
            return 0.0;
        }
        if self.keyframes.len() == 1 {
            return self.keyframes[0].value;
        }

        let t = if self.duration > 0.0 {
            (elapsed / self.duration) as f32
        } else {
            1.0
        };
        let t = t.clamp(0.0, 1.0);

        // Find surrounding keyframes
        let mut prev_idx = 0;
        let mut next_idx = self.keyframes.len() - 1;

        for (i, kf) in self.keyframes.iter().enumerate() {
            if kf.time <= t {
                prev_idx = i;
            }
            if kf.time >= t && i < next_idx {
                next_idx = i;
                break;
            }
        }

        let prev = &self.keyframes[prev_idx];
        let next = &self.keyframes[next_idx];

        if prev_idx == next_idx {
            return prev.value;
        }

        let segment_duration = next.time - prev.time;
        if segment_duration.abs() < 1e-6 {
            return next.value;
        }

        let local_t = (t - prev.time) / segment_duration;
        let local_t = local_t.clamp(0.0, 1.0);
        let eased_t = next.easing.apply(local_t);

        prev.value + (next.value - prev.value) * eased_t
    }

    pub fn property(&self) -> AnimatedProperty {
        self.property
    }

    pub fn on_complete_id(&self) -> Option<u32> {
        self.on_complete.as_ref().map(|cb| cb.0)
    }
}

/// Trait for types that support linear interpolation.
pub trait Interpolate: Copy {
    fn lerp(self, other: Self, t: f32) -> Self;
}

impl Interpolate for f32 {
    fn lerp(self, other: Self, t: f32) -> Self {
        self + (other - self) * t
    }
}

/// Resolved animation values for a single view node, computed each frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct AnimationState {
    pub opacity: Option<f32>,
    pub offset: Option<Vec2>,
    pub scale: Option<f32>,
    pub corner_radius: Option<f32>,
}

impl AnimationState {
    /// Create an empty animation state (no overrides).
    pub fn empty() -> Self {
        Self::default()
    }

    /// Returns true if no animation overrides are active.
    pub fn is_empty(&self) -> bool {
        self.opacity.is_none()
            && self.offset.is_none()
            && self.scale.is_none()
            && self.corner_radius.is_none()
    }

    /// Apply animation state as overrides to a bounds rect.
    /// Returns the modified bounds.
    pub fn apply_to_bounds(&self, bounds: katla_math::Rect2D) -> katla_math::Rect2D {
        let mut result = bounds;

        if let Some(offset) = self.offset {
            result = katla_math::Rect2D::new(result.min + offset, result.max + offset);
        }

        if let Some(scale) = self.scale {
            let center = result.center();
            let half_w = result.width() * 0.5 * scale;
            let half_h = result.height() * 0.5 * scale;
            result = katla_math::Rect2D::new(
                Vec2::new(center.x() - half_w, center.y() - half_h),
                Vec2::new(center.x() + half_w, center.y() + half_h),
            );
        }

        result
    }

    /// Apply opacity override to a color. Returns the modified color.
    pub fn apply_to_color(&self, color: katla_math::Color) -> katla_math::Color {
        if let Some(opacity) = self.opacity {
            katla_math::Color::new(color.r, color.g, color.b, color.a * opacity)
        } else {
            color
        }
    }

    /// Apply corner radius override. Returns the override value or the original.
    pub fn apply_to_corner_radius(&self, original: f32) -> f32 {
        self.corner_radius.unwrap_or(original)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_easing_linear() {
        assert!((Easing::Linear.apply(0.0) - 0.0).abs() < 1e-4);
        assert!((Easing::Linear.apply(0.5) - 0.5).abs() < 1e-4);
        assert!((Easing::Linear.apply(1.0) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_easing_ease_in() {
        assert!((Easing::EaseIn.apply(0.0) - 0.0).abs() < 1e-4);
        assert!((Easing::EaseIn.apply(1.0) - 1.0).abs() < 1e-4);
        // Ease-in at 0.5 should be < 0.5
        assert!(Easing::EaseIn.apply(0.5) < 0.5);
    }

    #[test]
    fn test_easing_ease_out() {
        assert!((Easing::EaseOut.apply(0.0) - 0.0).abs() < 1e-4);
        assert!((Easing::EaseOut.apply(1.0) - 1.0).abs() < 1e-4);
        // Ease-out at 0.5 should be > 0.5
        assert!(Easing::EaseOut.apply(0.5) > 0.5);
    }

    #[test]
    fn test_tween_value_at() {
        let tween = Tween {
            from: 0.0,
            to: 1.0,
            duration: 1.0,
            easing: Easing::Linear,
        };
        assert!((tween.value_at(0.0) - 0.0).abs() < 1e-4);
        assert!((tween.value_at(0.5) - 0.5).abs() < 1e-4);
        assert!((tween.value_at(1.0) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_tween_clamps_beyond_duration() {
        let tween = Tween {
            from: 0.0,
            to: 100.0,
            duration: 1.0,
            easing: Easing::Linear,
        };
        assert!((tween.value_at(2.0) - 100.0).abs() < 1e-4);
    }

    #[test]
    fn test_animation_is_complete() {
        let anim = Animation {
            property: AnimatedProperty::Opacity,
            tween: Tween {
                from: 0.0,
                to: 1.0,
                duration: 1.0,
                easing: Easing::Linear,
            },
            start_time: 0.0,
            on_complete: None,
        };
        assert!(!anim.is_complete(0.5));
        assert!(anim.is_complete(1.0));
        assert!(anim.is_complete(2.0));
    }

    #[test]
    fn test_animation_value_at() {
        let anim = Animation {
            property: AnimatedProperty::Opacity,
            tween: Tween {
                from: 0.0,
                to: 1.0,
                duration: 2.0,
                easing: Easing::Linear,
            },
            start_time: 1.0,
            on_complete: None,
        };
        assert!((anim.value_at(1.0) - 0.0).abs() < 1e-4);
        assert!((anim.value_at(2.0) - 0.5).abs() < 1e-4);
        assert!((anim.value_at(3.0) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_interpolate_f32() {
        assert!((0.0_f32.lerp(10.0, 0.5) - 5.0).abs() < 1e-4);
        assert!((0.0_f32.lerp(10.0, 0.0)).abs() < 1e-4);
        assert!((0.0_f32.lerp(10.0, 1.0) - 10.0).abs() < 1e-4);
    }

    #[test]
    fn test_animation_state_apply_to_bounds_offset() {
        let state = AnimationState {
            offset: Some(Vec2::new(10.0, 20.0)),
            ..Default::default()
        };
        let bounds = katla_math::Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(100.0, 100.0));
        let result = state.apply_to_bounds(bounds);
        assert!((result.min.x() - 10.0).abs() < 1e-4);
        assert!((result.min.y() - 20.0).abs() < 1e-4);
        assert!((result.max.x() - 110.0).abs() < 1e-4);
        assert!((result.max.y() - 120.0).abs() < 1e-4);
    }

    #[test]
    fn test_animation_state_apply_to_bounds_scale() {
        let state = AnimationState {
            scale: Some(0.5),
            ..Default::default()
        };
        let bounds = katla_math::Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(100.0, 100.0));
        let result = state.apply_to_bounds(bounds);
        // Center is (50, 50), half-size becomes 25
        assert!((result.min.x() - 25.0).abs() < 1e-4);
        assert!((result.min.y() - 25.0).abs() < 1e-4);
        assert!((result.max.x() - 75.0).abs() < 1e-4);
        assert!((result.max.y() - 75.0).abs() < 1e-4);
    }

    #[test]
    fn test_animation_state_apply_to_color() {
        let state = AnimationState {
            opacity: Some(0.5),
            ..Default::default()
        };
        let color = katla_math::Color::new(1.0, 0.5, 0.0, 1.0);
        let result = state.apply_to_color(color);
        assert!((result.a - 0.5).abs() < 1e-4);
        assert!((result.r - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_keyframe_animation_single_keyframe() {
        let anim = KeyframeAnimation {
            property: AnimatedProperty::Opacity,
            keyframes: vec![Keyframe {
                time: 0.0,
                value: 0.5,
                easing: Easing::Linear,
            }],
            duration: 1.0,
            start_time: 0.0,
            on_complete: None,
        };
        assert!((anim.value_at(0.5) - 0.5).abs() < 1e-4);
    }

    #[test]
    fn test_keyframe_animation_two_keyframes() {
        let anim = KeyframeAnimation {
            property: AnimatedProperty::Opacity,
            keyframes: vec![
                Keyframe {
                    time: 0.0,
                    value: 0.0,
                    easing: Easing::Linear,
                },
                Keyframe {
                    time: 1.0,
                    value: 1.0,
                    easing: Easing::Linear,
                },
            ],
            duration: 2.0,
            start_time: 0.0,
            on_complete: None,
        };
        assert!((anim.value_at(0.0) - 0.0).abs() < 1e-4);
        assert!((anim.value_at(1.0) - 0.5).abs() < 1e-4);
        assert!((anim.value_at(2.0) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_keyframe_animation_is_complete() {
        let anim = KeyframeAnimation {
            property: AnimatedProperty::Opacity,
            keyframes: vec![],
            duration: 1.0,
            start_time: 0.0,
            on_complete: None,
        };
        assert!(!anim.is_complete(0.5));
        assert!(anim.is_complete(1.0));
    }

    #[test]
    fn test_animation_state_is_empty() {
        assert!(AnimationState::empty().is_empty());
        assert!(
            !AnimationState {
                opacity: Some(1.0),
                ..Default::default()
            }
            .is_empty()
        );
    }
}
