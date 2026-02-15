/// EntityId is a unique identifier for an entity in the ECS.
///
/// In this architecture, entities are just IDs. All component data is stored
/// separately in the World's component vectors.
///
/// The ID is composed of a 32-bit index and a 32-bit generation counter.
/// The generation counter allows detection of stale references when an
/// entity is destroyed and its slot is reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityId(u64);

// Layout: [32-bit generation | 32-bit index]
const INDEX_BITS: u32 = 32;
const INDEX_MASK: u64 = (1u64 << INDEX_BITS) - 1;

impl EntityId {
    /// Creates a new EntityId from an index and generation.
    ///
    /// The index identifies the slot in the entity allocator.
    /// The generation is used to detect stale references.
    pub(crate) fn new(index: u32, generation: u32) -> Self {
        Self(((generation as u64) << INDEX_BITS) | (index as u64))
    }

    /// Returns the slot index for this entity.
    pub(crate) fn index(&self) -> u32 {
        (self.0 & INDEX_MASK) as u32
    }

    /// Returns the generation counter for this entity.
    pub(crate) fn generation(&self) -> u32 {
        (self.0 >> INDEX_BITS) as u32
    }

    /// Returns the raw ID value for backward compatibility.
    pub fn id(&self) -> u64 {
        self.0
    }

    /// Creates an EntityId from a raw u64 value.
    ///
    /// This should only be used for deserialization or when you know
    /// the ID was previously created by this system.
    pub fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Creates an EntityId for testing purposes with a simple index.
    ///
    /// Uses generation 0. In production code, always use World::create_entity()
    /// to get proper generation-tracked IDs.
    #[cfg(test)]
    pub fn test_new(index: u32) -> Self {
        Self::new(index, 0)
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Entity({}:{})", self.index(), self.generation())
    }
}

// EntityId does not implement Default because entity IDs should only
// be created through the World's entity allocator.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_id_packing() {
        let id = EntityId::new(42, 7);
        assert_eq!(id.index(), 42);
        assert_eq!(id.generation(), 7);
    }

    #[test]
    fn test_entity_id_max_values() {
        let id = EntityId::new(u32::MAX, u32::MAX);
        assert_eq!(id.index(), u32::MAX);
        assert_eq!(id.generation(), u32::MAX);
    }

    #[test]
    fn test_entity_id_zero_values() {
        let id = EntityId::new(0, 0);
        assert_eq!(id.index(), 0);
        assert_eq!(id.generation(), 0);
    }

    #[test]
    fn test_entity_id_equality() {
        let id1 = EntityId::new(42, 7);
        let id2 = EntityId::new(42, 7);
        let id3 = EntityId::new(42, 8);
        let id4 = EntityId::new(43, 7);

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
        assert_ne!(id1, id4);
    }

    #[test]
    fn test_raw_roundtrip() {
        let id = EntityId::new(123, 456);
        let raw = id.id();
        let recovered = EntityId::from_raw(raw);

        assert_eq!(id, recovered);
        assert_eq!(recovered.index(), 123);
        assert_eq!(recovered.generation(), 456);
    }
}
