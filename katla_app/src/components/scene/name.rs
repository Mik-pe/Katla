use katla_ecs::Component;

#[derive(Component, Debug, Clone)]
pub struct NameComponent {
    pub name: String,
}

impl NameComponent {
    /// Creates a new NameComponent with the specified name.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}
