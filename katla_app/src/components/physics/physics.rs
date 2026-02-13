use katla_ecs::Component;
use katla_math::Vec3;

#[derive(Component, Default, Clone)]
pub struct MassComponent {
    pub mass: f32,
}

#[derive(Component, Default, Clone)]
pub struct DragComponent {
    pub coefficient: f32,
}

#[derive(Component, Default, Clone)]
pub struct ForceComponent {
    pub force: Vec3,
}

#[derive(Component, Default)]
pub struct VelocityComponent {
    pub velocity: Vec3,
    pub acceleration: Vec3,
}

impl DragComponent {
    pub fn new(coefficient: f32) -> Self {
        DragComponent { coefficient }
    }
}

impl VelocityComponent {
    pub fn new(velocity: Vec3, acceleration: Vec3) -> Self {
        VelocityComponent {
            velocity,
            acceleration,
        }
    }
}
