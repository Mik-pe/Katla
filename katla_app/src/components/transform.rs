use katla_ecs::Component;
use katla_math::Transform;

#[derive(Component, Default)]
pub struct TransformComponent {
    pub transform: Transform,
}

impl TransformComponent {
    pub fn new(transform: Transform) -> Self {
        TransformComponent {
            transform: transform,
        }
    }
}
