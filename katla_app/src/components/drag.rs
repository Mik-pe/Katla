use katla_ecs::Component;

#[derive(Component, Default, Clone)]
pub struct DragComponent {
    pub drag: f32,
}

impl DragComponent {
    pub fn new(drag: f32) -> Self {
        DragComponent { drag: drag }
    }
}
