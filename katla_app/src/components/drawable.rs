use katla_ecs::Component;

use crate::rendering;

#[derive(Component)]
pub struct Drawable(pub Box<dyn rendering::Drawable>);

impl Drawable {
    pub fn new(drawable: Box<dyn rendering::Drawable>) -> Self {
        Drawable(drawable)
    }
}
