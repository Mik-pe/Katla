use katla_ecs::Component;
use katla_math::Color;

/// Icon type for billboard rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BillboardIcon {
    /// Lightbulb icon for point lights
    Lightbulb,
    /// Fire icon for particle emitters
    Fire,
}

/// Billboard component for rendering editor-only billboard icons.
///
/// Attached to entities that have no visible mesh (point lights, particle emitters)
/// so they appear as camera-facing icon quads in the editor viewport.
#[derive(Component, Debug, Clone)]
pub struct BillboardComponent {
    /// Which icon to display
    pub icon: BillboardIcon,
    /// Optional color tint (white = no tint)
    pub color: Color,
    /// Screen-space size scaling factor
    pub size: f32,
}

impl BillboardComponent {
    /// Create a new billboard with the given icon and default styling.
    pub fn new(icon: BillboardIcon) -> Self {
        Self {
            icon,
            color: Color::WHITE,
            size: 1.0,
        }
    }
}

impl Default for BillboardComponent {
    fn default() -> Self {
        Self::new(BillboardIcon::Lightbulb)
    }
}
