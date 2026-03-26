//! Entity allocator with generation-based ID recycling.
//!
//! This module provides an allocator that manages entity IDs with generation
//! counters. When an entity is destroyed, its slot can be reused, but the
//! generation counter ensures stale references are detected.

use crate::entity::EntityId;
use crate::entity_slot::EntitySlot;

/// Manages entity ID allocation with generation-based validation.
#[derive(Debug, Clone)]
pub(crate) struct EntityAllocator {
    /// Slots for each possible entity index.
    slots: Vec<EntitySlot>,
    /// Free indices available for reuse.
    free_indices: Vec<u32>,
    /// Number of currently live entities.
    live_count: usize,
}

impl EntityAllocator {
    /// Creates a new empty allocator.
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            free_indices: Vec::new(),
            live_count: 0,
        }
    }

    /// Allocates a new entity ID.
    ///
    /// Reuses freed slots when available, otherwise allocates a new slot.
    pub fn allocate(&mut self) -> EntityId {
        self.live_count += 1;

        if let Some(index) = self.free_indices.pop() {
            // Reuse a freed slot
            let slot = &mut self.slots[index as usize];
            slot.occupied = true;
            EntityId::new(index, slot.generation)
        } else {
            // Allocate a new slot
            let index = self.slots.len() as u32;
            self.slots.push(EntitySlot::occupied(0));
            EntityId::new(index, 0)
        }
    }

    /// Deallocates an entity ID.
    ///
    /// Returns `true` if the entity was valid and deallocated, `false` otherwise.
    /// Increments the generation counter to invalidate stale references.
    pub fn deallocate(&mut self, id: EntityId) -> bool {
        let index = id.index() as usize;

        if index >= self.slots.len() {
            return false;
        }

        let slot = &mut self.slots[index];

        // Validate generation and occupancy
        if !slot.occupied || slot.generation != id.generation() {
            return false;
        }

        slot.occupied = false;
        // Increment generation to invalidate stale references
        // Wrapping add handles generation overflow gracefully
        slot.generation = slot.generation.wrapping_add(1);
        self.free_indices.push(id.index());
        self.live_count -= 1;

        true
    }

    /// Checks if an entity ID is valid (correct generation and currently live).
    pub fn is_valid(&self, id: EntityId) -> bool {
        let index = id.index() as usize;

        if index >= self.slots.len() {
            return false;
        }

        let slot = &self.slots[index];
        slot.occupied && slot.generation == id.generation()
    }

    /// Returns an iterator over all live entity IDs.
    pub fn iter_live(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.slots.iter().enumerate().filter_map(|(index, slot)| {
            if slot.occupied {
                Some(EntityId::new(index as u32, slot.generation))
            } else {
                None
            }
        })
    }

    /// Returns the number of live entities.
    pub fn live_count(&self) -> usize {
        self.live_count
    }

    /// Clears all entities and resets the allocator.
    pub fn clear(&mut self) {
        self.slots.clear();
        self.free_indices.clear();
        self.live_count = 0;
    }
}

impl Default for EntityAllocator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocate_unique_ids() {
        let mut allocator = EntityAllocator::new();
        let id1 = allocator.allocate();
        let id2 = allocator.allocate();

        assert_ne!(id1, id2);
        assert_eq!(allocator.live_count(), 2);
    }

    #[test]
    fn test_deallocate_and_reuse() {
        let mut allocator = EntityAllocator::new();
        let id1 = allocator.allocate();
        let _id2 = allocator.allocate();

        // Deallocate first entity
        assert!(allocator.deallocate(id1));
        assert_eq!(allocator.live_count(), 1);

        // Allocate new entity - should reuse slot
        let id3 = allocator.allocate();
        assert_eq!(id3.index(), id1.index()); // Same index
        assert_ne!(id3.generation(), id1.generation()); // Different generation
        assert_eq!(allocator.live_count(), 2);
    }

    #[test]
    fn test_stale_reference_detection() {
        let mut allocator = EntityAllocator::new();
        let id1 = allocator.allocate();
        allocator.deallocate(id1);

        // Old ID should no longer be valid
        assert!(!allocator.is_valid(id1));

        // Allocate new entity reusing the slot
        let id2 = allocator.allocate();
        assert!(allocator.is_valid(id2));
        assert!(!allocator.is_valid(id1)); // Old ID still invalid
    }

    #[test]
    fn test_generation_overflow() {
        let mut allocator = EntityAllocator::new();
        let id1 = allocator.allocate();
        let index = id1.index();

        // Manually set generation to max
        allocator.slots[index as usize].generation = u32::MAX;
        allocator.deallocate(id1);

        // Generation should wrap to 0
        let id2 = allocator.allocate();
        assert_eq!(id2.generation(), 0);
    }

    #[test]
    fn test_iter_live() {
        let mut allocator = EntityAllocator::new();
        let id1 = allocator.allocate();
        let id2 = allocator.allocate();
        let id3 = allocator.allocate();

        allocator.deallocate(id2);

        let live: Vec<EntityId> = allocator.iter_live().collect();
        assert_eq!(live.len(), 2);
        assert!(live.contains(&id1));
        assert!(live.contains(&id3));
        assert!(!live.contains(&id2));
    }

    #[test]
    fn test_deallocate_invalid_id() {
        let mut allocator = EntityAllocator::new();

        // Try to deallocate non-existent entity
        let fake_id = EntityId::new(999, 0);
        assert!(!allocator.deallocate(fake_id));

        // Try to deallocate with wrong generation
        let id = allocator.allocate();
        let wrong_gen = EntityId::new(id.index(), id.generation() + 1);
        assert!(!allocator.deallocate(wrong_gen));
    }

    #[test]
    fn test_entity_allocator_stress_create_destroy() {
        let mut allocator = EntityAllocator::new();
        let mut ids = Vec::new();

        // Create 1000 entities
        for _ in 0..1000 {
            ids.push(allocator.allocate());
        }
        assert_eq!(allocator.live_count(), 1000);

        // Destroy all
        for id in &ids {
            assert!(allocator.deallocate(*id));
        }
        assert_eq!(allocator.live_count(), 0);

        // Create 1000 more - should reuse slots
        let mut new_ids = Vec::new();
        for _ in 0..1000 {
            new_ids.push(allocator.allocate());
        }
        assert_eq!(allocator.live_count(), 1000);

        // Verify no corruption: all new IDs should be valid
        for id in &new_ids {
            assert!(allocator.is_valid(*id));
        }

        // Verify live iteration matches count
        assert_eq!(allocator.iter_live().count(), 1000);
    }

    #[test]
    fn test_allocator_clear_resets_state() {
        let mut allocator = EntityAllocator::new();
        allocator.allocate();
        allocator.allocate();
        allocator.allocate();

        allocator.clear();

        assert_eq!(allocator.live_count(), 0);
        assert_eq!(allocator.iter_live().count(), 0);

        // Should be able to allocate fresh IDs after clear
        let id = allocator.allocate();
        assert_eq!(id.index(), 0);
        assert_eq!(id.generation(), 0);
    }
}
