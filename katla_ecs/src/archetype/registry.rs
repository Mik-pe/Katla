use std::collections::HashMap;

use crate::entity::EntityId;

use super::archetype::Archetype;
use super::signature::{ArchetypeId, ComponentSignature};

pub struct ArchetypeRegistry {
    archetypes: Vec<Archetype>,
    signature_to_id: HashMap<ComponentSignature, ArchetypeId>,
    entity_locations: HashMap<EntityId, (ArchetypeId, usize)>,
}

impl ArchetypeRegistry {
    pub fn new() -> Self {
        Self {
            archetypes: Vec::new(),
            signature_to_id: HashMap::new(),
            entity_locations: HashMap::new(),
        }
    }

    pub fn get_or_create(&mut self, signature: &ComponentSignature) -> ArchetypeId {
        if let Some(&id) = self.signature_to_id.get(signature) {
            return id;
        }

        let id = ArchetypeId(self.archetypes.len() as u32);
        self.archetypes.push(Archetype::new(id));
        self.signature_to_id.insert(signature.clone(), id);
        id
    }

    pub fn get(&self, id: ArchetypeId) -> &Archetype {
        &self.archetypes[id.0 as usize]
    }

    pub fn get_mut(&mut self, id: ArchetypeId) -> &mut Archetype {
        &mut self.archetypes[id.0 as usize]
    }

    pub fn entity_location(&self, entity: EntityId) -> Option<(ArchetypeId, usize)> {
        self.entity_locations.get(&entity).copied()
    }

    pub fn len(&self) -> usize {
        self.archetypes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.archetypes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::TypeId;

    fn sig<T: 'static>() -> ComponentSignature {
        vec![TypeId::of::<T>()]
    }

    #[test]
    fn test_registry_new_is_empty() {
        let registry = ArchetypeRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_get_or_create_same_signature_returns_same_id() {
        let mut registry = ArchetypeRegistry::new();
        let sig = sig::<i32>();
        let id1 = registry.get_or_create(&sig);
        let id2 = registry.get_or_create(&sig);
        assert_eq!(id1, id2);
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_get_or_create_different_signatures_return_different_ids() {
        let mut registry = ArchetypeRegistry::new();
        let sig_a = sig::<i32>();
        let sig_b = sig::<f32>();
        let id_a = registry.get_or_create(&sig_a);
        let id_b = registry.get_or_create(&sig_b);
        assert_ne!(id_a, id_b);
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn test_entity_location_not_found() {
        let registry = ArchetypeRegistry::new();
        let entity = EntityId::test_new(0);
        assert!(registry.entity_location(entity).is_none());
    }
}
