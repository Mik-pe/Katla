use katla_ecs::Component;

#[derive(Component, Debug, Clone)]
pub struct TagComponent {
    pub tag: String,
}

/// Marker component to hide an entity from the editor hierarchy.
/// Add this to internal entities like the editor camera.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct EditorHidden;

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
