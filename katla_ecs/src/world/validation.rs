//! Entity and component integrity validation.

use super::World;
use crate::EntityId;
#[cfg(debug_assertions)]
use std::collections::HashSet;

impl World {
    /// Validates the internal consistency of the world state.
    ///
    /// Checks entity allocator consistency, component storage integrity,
    /// and verifies no orphaned component data exists for deleted entities.
    ///
    /// Returns `Ok(())` if all checks pass, or `Err` with descriptions of
    /// inconsistencies found. Does not panic — callers decide how to handle errors.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        self.validate_entity_allocator(&mut errors);
        self.validate_component_integrity(&mut errors);

        #[cfg(debug_assertions)]
        self.validate_no_duplicate_entities(&mut errors);

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Verifies a list of entities still exist in the world.
    ///
    /// Returns `true` only if every entity in the slice is currently live.
    pub fn validate_entities(&self, entities: &[EntityId]) -> bool {
        entities.iter().all(|&id| self.entity_exists(id))
    }

    fn validate_entity_allocator(&self, errors: &mut Vec<String>) {
        let actual_live = self.entities.iter_live().count();
        let reported_live = self.entities.live_count();
        if actual_live != reported_live {
            errors.push(format!(
                "Entity live_count ({reported_live}) doesn't match iter_live count ({actual_live})"
            ));
        }

        for id in self.entities.iter_live() {
            if !self.entities.is_valid(id) {
                errors.push(format!("Live entity {id} failed is_valid check"));
            }
        }
    }

    fn validate_component_integrity(&self, errors: &mut Vec<String>) {
        // SAFETY: Validation holds a shared World borrow without active worker jobs.
        let entities_with_components = unsafe { (&*self.storage.get()).entities_with_components() };
        for id in &entities_with_components {
            if !self.entities.is_valid(*id) {
                errors.push(format!("Orphaned component data for deleted entity {id}"));
            }
        }
    }

    #[cfg(debug_assertions)]
    fn validate_no_duplicate_entities(&self, errors: &mut Vec<String>) {
        let mut seen = HashSet::new();
        for id in self.entities.iter_live() {
            if !seen.insert(id) {
                errors.push(format!("Duplicate entity ID {id} in live iteration"));
            }
        }
    }
}
