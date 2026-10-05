use super::animation::{AnimatedProperty, Easing};

/// Configuration for a single tween phase (insert or remove).
#[derive(Clone, Debug)]
pub struct TweenConfig {
    pub duration: f64,
    pub easing: Easing,
}

/// Transition configuration for insert/remove animations on a view node.
#[derive(Clone, Debug)]
pub struct Transition {
    pub insert: Option<TweenConfig>,
    pub remove: Option<TweenConfig>,
    pub property: AnimatedProperty,
}

impl Transition {
    /// Fade in/out transition using opacity.
    pub fn fade(duration: f64) -> Self {
        Transition {
            insert: Some(TweenConfig {
                duration,
                easing: Easing::EaseOut,
            }),
            remove: Some(TweenConfig {
                duration,
                easing: Easing::EaseIn,
            }),
            property: AnimatedProperty::Opacity,
        }
    }

    /// Slide up from below transition using vertical offset.
    pub fn slide_up(duration: f64) -> Self {
        Transition {
            insert: Some(TweenConfig {
                duration,
                easing: Easing::EaseOut,
            }),
            remove: Some(TweenConfig {
                duration,
                easing: Easing::EaseIn,
            }),
            property: AnimatedProperty::OffsetY,
        }
    }

    /// Slide down from above transition using vertical offset.
    pub fn slide_down(duration: f64) -> Self {
        Transition {
            insert: Some(TweenConfig {
                duration,
                easing: Easing::EaseOut,
            }),
            remove: Some(TweenConfig {
                duration,
                easing: Easing::EaseIn,
            }),
            property: AnimatedProperty::OffsetY,
        }
    }

    /// Scale transition from one factor to another.
    pub fn scale(_from: f32, _to: f32, duration: f64) -> Self {
        Transition {
            insert: Some(TweenConfig {
                duration,
                easing: Easing::Spring {
                    stiffness: 300.0,
                    damping: 20.0,
                },
            }),
            remove: Some(TweenConfig {
                duration,
                easing: Easing::EaseIn,
            }),
            property: AnimatedProperty::Scale,
        }
    }
}
