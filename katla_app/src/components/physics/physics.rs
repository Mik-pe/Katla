use katla_ecs::Component;
use katla_math::Vec3;

#[derive(Component, Default)]
pub struct VelocityComponent {
    pub velocity: Vec3,
    pub acceleration: Vec3,
}

impl VelocityComponent {
    pub fn new(velocity: Vec3, acceleration: Vec3) -> Self {
        VelocityComponent {
            velocity,
            acceleration,
        }
    }
}
