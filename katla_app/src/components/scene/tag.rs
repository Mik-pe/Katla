use katla_ecs::Component;

#[derive(Component, Debug, Clone)]
pub struct TagComponent {
    pub tag: String,
}

impl TagComponent {
    /// Creates a new TagComponent with the specified tag.
    pub fn new(tag: impl Into<String>) -> Self {
        Self { tag: tag.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::TagComponent;

    #[test]
    fn test_tag_component() {
        let tag = TagComponent::new("Test");
        assert_eq!(tag.tag, "Test");
    }
}
