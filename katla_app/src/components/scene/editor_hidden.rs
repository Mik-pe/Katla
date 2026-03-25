use katla_ecs::Component;

/// Marker component to hide an entity from the editor hierarchy.
/// Add this to internal entities like the editor camera.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct EditorHidden;
