use katla_ecs::Component;

#[derive(Component)]
pub struct InputComponent {
    
}

impl Default for InputComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl InputComponent {
    pub fn new() -> Self {
        InputComponent {}
    }
}
