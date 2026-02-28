//! Global entity selection tracking.
//!
//! This module provides a resource for tracking which entities are currently
//! selected in the editor. Selection is global across all viewports - when
//! an entity is selected in one viewport, it's highlighted in all viewports.

use std::collections::HashSet;

use katla_ecs::EntityId;

/// Global selection state for the editor.
///
/// Tracks which entities are currently selected. The selection is shared
/// across all viewports - selecting an entity in one viewport highlights
/// it in all viewports.
///
/// # Example
///
/// ```ignore
/// let mut selection = Selection::new();
///
/// // Select an entity
/// selection.select(entity_id);
/// assert!(selection.is_selected(entity_id));
///
/// // Get the primary selection (last selected)
/// let primary = selection.get_primary();
///
/// // Clear all selections
/// selection.clear();
/// assert!(selection.is_empty());
/// ```
#[derive(Debug, Clone, Default)]
pub struct Selection {
    /// Set of all selected entities.
    selected_entities: HashSet<EntityId>,
    /// The most recently selected entity (for operations that need a "primary" target).
    primary_selection: Option<EntityId>,
}

impl Selection {
    /// Creates a new empty selection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Selects an entity.
    ///
    /// If the entity was already selected, this updates the primary selection.
    pub fn select(&mut self, entity: EntityId) {
        self.selected_entities.insert(entity);
        self.primary_selection = Some(entity);
    }

    /// Deselects an entity.
    ///
    /// If the deselected entity was the primary selection, the primary is cleared.
    pub fn deselect(&mut self, entity: EntityId) {
        self.selected_entities.remove(&entity);
        if self.primary_selection == Some(entity) {
            // Pick a new primary from remaining selections, or clear
            self.primary_selection = self.selected_entities.iter().next().copied();
        }
    }

    /// Toggles the selection state of an entity.
    ///
    /// Returns `true` if the entity is now selected, `false` if it's now deselected.
    pub fn toggle(&mut self, entity: EntityId) -> bool {
        if self.is_selected(entity) {
            self.deselect(entity);
            false
        } else {
            self.select(entity);
            true
        }
    }

    /// Clears all selections.
    pub fn clear(&mut self) {
        self.selected_entities.clear();
        self.primary_selection = None;
    }

    /// Checks if an entity is selected.
    pub fn is_selected(&self, entity: EntityId) -> bool {
        self.selected_entities.contains(&entity)
    }

    /// Returns the primary (most recently selected) entity.
    pub fn get_primary(&self) -> Option<EntityId> {
        self.primary_selection
    }

    /// Returns the number of selected entities.
    pub fn count(&self) -> usize {
        self.selected_entities.len()
    }

    /// Returns true if no entities are selected.
    pub fn is_empty(&self) -> bool {
        self.selected_entities.is_empty()
    }

    /// Returns an iterator over all selected entities.
    pub fn iter(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.selected_entities.iter().copied()
    }

    /// Selects multiple entities at once.
    ///
    /// The last entity in the iterator becomes the primary selection.
    pub fn select_multiple(&mut self, entities: impl IntoIterator<Item = EntityId>) {
        for entity in entities {
            self.select(entity);
        }
    }

    /// Replaces the selection with the given entities.
    ///
    /// The last entity becomes the primary selection.
    pub fn set_selection(&mut self, entities: impl IntoIterator<Item = EntityId>) {
        self.clear();
        self.select_multiple(entities);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use katla_ecs::World;

    fn make_entity(world: &mut World) -> EntityId {
        world.create_entity()
    }

    #[test]
    fn test_empty_selection() {
        let selection = Selection::new();
        assert!(selection.is_empty());
        assert_eq!(selection.count(), 0);
        assert_eq!(selection.get_primary(), None);
    }

    #[test]
    fn test_select_entity() {
        let mut world = World::new();
        let mut selection = Selection::new();
        let entity = make_entity(&mut world);

        selection.select(entity);

        assert!(!selection.is_empty());
        assert_eq!(selection.count(), 1);
        assert!(selection.is_selected(entity));
        assert_eq!(selection.get_primary(), Some(entity));
    }

    #[test]
    fn test_deselect_entity() {
        let mut world = World::new();
        let mut selection = Selection::new();
        let entity = make_entity(&mut world);

        selection.select(entity);
        assert!(selection.is_selected(entity));

        selection.deselect(entity);
        assert!(!selection.is_selected(entity));
        assert!(selection.is_empty());
        assert_eq!(selection.get_primary(), None);
    }

    #[test]
    fn test_toggle_selection() {
        let mut world = World::new();
        let mut selection = Selection::new();
        let entity = make_entity(&mut world);

        // Toggle on
        let result = selection.toggle(entity);
        assert!(result);
        assert!(selection.is_selected(entity));

        // Toggle off
        let result = selection.toggle(entity);
        assert!(!result);
        assert!(!selection.is_selected(entity));
    }

    #[test]
    fn test_primary_selection_updates() {
        let mut world = World::new();
        let mut selection = Selection::new();
        let e1 = make_entity(&mut world);
        let e2 = make_entity(&mut world);
        let e3 = make_entity(&mut world);

        selection.select(e1);
        assert_eq!(selection.get_primary(), Some(e1));

        selection.select(e2);
        assert_eq!(selection.get_primary(), Some(e2));

        selection.select(e3);
        assert_eq!(selection.get_primary(), Some(e3));
    }

    #[test]
    fn test_primary_selection_on_deselect() {
        let mut world = World::new();
        let mut selection = Selection::new();
        let e1 = make_entity(&mut world);
        let e2 = make_entity(&mut world);

        selection.select(e1);
        selection.select(e2);
        assert_eq!(selection.get_primary(), Some(e2));

        // Deselect primary - should pick another
        selection.deselect(e2);
        assert_eq!(selection.get_primary(), Some(e1));

        // Deselect last - should be None
        selection.deselect(e1);
        assert_eq!(selection.get_primary(), None);
    }

    #[test]
    fn test_clear_selection() {
        let mut world = World::new();
        let mut selection = Selection::new();
        let e1 = make_entity(&mut world);
        let e2 = make_entity(&mut world);

        selection.select(e1);
        selection.select(e2);
        assert_eq!(selection.count(), 2);

        selection.clear();
        assert!(selection.is_empty());
        assert_eq!(selection.count(), 0);
        assert_eq!(selection.get_primary(), None);
    }

    #[test]
    fn test_iter_selected() {
        let mut world = World::new();
        let mut selection = Selection::new();
        let e1 = make_entity(&mut world);
        let e2 = make_entity(&mut world);
        let e3 = make_entity(&mut world);

        selection.select(e1);
        selection.select(e2);
        selection.select(e3);

        let selected: HashSet<_> = selection.iter().collect();
        assert_eq!(selected.len(), 3);
        assert!(selected.contains(&e1));
        assert!(selected.contains(&e2));
        assert!(selected.contains(&e3));
    }

    #[test]
    fn test_select_multiple() {
        let mut world = World::new();
        let mut selection = Selection::new();
        let entities = vec![
            make_entity(&mut world),
            make_entity(&mut world),
            make_entity(&mut world),
        ];

        selection.select_multiple(entities.clone());

        assert_eq!(selection.count(), 3);
        for e in &entities {
            assert!(selection.is_selected(*e));
        }
        // Last entity should be primary
        assert_eq!(selection.get_primary(), Some(entities[2]));
    }

    #[test]
    fn test_set_selection() {
        let mut world = World::new();
        let mut selection = Selection::new();

        // Initial selection
        let e1 = make_entity(&mut world);
        let e2 = make_entity(&mut world);
        selection.select(e1);
        selection.select(e2);
        assert_eq!(selection.count(), 2);

        // Replace with new selection
        let new_entities = vec![make_entity(&mut world), make_entity(&mut world)];
        selection.set_selection(new_entities.clone());

        assert_eq!(selection.count(), 2);
        assert!(!selection.is_selected(e1));
        assert!(!selection.is_selected(e2));
        for e in &new_entities {
            assert!(selection.is_selected(*e));
        }
    }
}
