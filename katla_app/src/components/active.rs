use katla_ecs::Component;

#[derive(Component, Debug, Clone)]
pub struct ActiveComponent {
    pub value: bool,
}

impl ActiveComponent {
    /// Creates a new ActiveComponent with the specified name.
    pub fn new() -> Self {
        Self { value: true }
    }
}

#[cfg(test)]
mod tests {
    use super::ActiveComponent;

    #[test]
    fn test_name_component() {
        let component = ActiveComponent::new();
        assert_eq!(component.value, true);
    }
}
