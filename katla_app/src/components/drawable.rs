use katla_ecs::Component;

use crate::rendering;

#[derive(Component)]
pub struct DrawableComponent(pub Box<dyn rendering::Drawable>);

impl DrawableComponent {
    pub fn new(drawable: Box<dyn rendering::Drawable>) -> Self {
        DrawableComponent(drawable)
    }
}
