/// EntityId is a unique identifier for an entity in the ECS.
///
/// In this architecture, entities are just IDs. All component data is stored
/// separately in the World's component vectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
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

// EntityId is a newtype wrapper around u64 with derived traits.
// Tests for derived traits (PartialEq, Ord, Display, etc.) are omitted
// as they test compiler-generated code rather than application logic.
