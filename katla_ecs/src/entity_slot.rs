//! Entity slot for tracking entity lifecycle and generation.
//!
//! Each slot in the entity allocator contains a generation counter and
//! occupancy flag. The generation increments each time a slot is reused,
//! allowing detection of stale entity references.

/// A slot in the entity allocator.
#[derive(Debug, Clone)]
pub(crate) struct EntitySlot {
    /// Generation counter incremented on each reuse.
    pub generation: u32,
    /// Whether the slot currently contains a live entity.
    pub occupied: bool,
}

impl EntitySlot {
    /// Creates a new empty slot with generation 0.
    pub fn new() -> Self {
        Self {
            generation: 0,
            occupied: false,
        }
    }

    /// Creates a new occupied slot with the given generation.
    pub fn occupied(generation: u32) -> Self {
        Self {
            generation,
            occupied: true,
        }
    }
}

impl Default for EntitySlot {
    fn default() -> Self {
        Self::new()
    }
}
