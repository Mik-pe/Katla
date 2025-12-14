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

#[cfg(test)]
mod tests {
    use super::NameComponent;

    #[test]
    fn test_name_component() {
        let name = NameComponent::new("Test");
        assert_eq!(name.name, "Test");
    }
}
