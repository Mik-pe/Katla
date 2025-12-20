/// EntityId is a unique identifier for an entity in the ECS.
///
/// In this architecture, entities are just IDs. All component data is stored
/// separately in the World's component vectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[derive(Default)]
pub struct EntityId(pub u64);

impl EntityId {
    /// Creates a new EntityId with the given value.
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// Returns the raw ID value.
    pub fn id(&self) -> u64 {
        self.0
    }
}


impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Entity({})", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_id_creation() {
        let id = EntityId::new(42);
        assert_eq!(id.0, 42);
        assert_eq!(id.id(), 42);
    }

    #[test]
    fn test_entity_id_equality() {
        let id1 = EntityId::new(1);
        let id2 = EntityId::new(1);
        let id3 = EntityId::new(2);

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_entity_id_ordering() {
        let id1 = EntityId::new(1);
        let id2 = EntityId::new(2);

        assert!(id1 < id2);
        assert!(id2 > id1);
    }

    #[test]
    fn test_entity_id_display() {
        let id = EntityId::new(123);
        assert_eq!(format!("{}", id), "Entity(123)");
    }
}
