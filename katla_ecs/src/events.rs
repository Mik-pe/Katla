use crate::entity::EntityId;
use std::any::TypeId;

/// Events emitted by the World for entity lifecycle changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntityEvent {
    /// An entity was created.
    Spawned(EntityId),
    /// An entity was destroyed.
    Destroyed(EntityId),
}

/// Events emitted by the World for component changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentEvent {
    /// A component was added to an entity.
    Added(EntityId, TypeId),
    /// A component was removed from an entity.
    Removed(EntityId, TypeId),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_event_equality() {
        let id = EntityId::new(1, 0);
        assert_eq!(EntityEvent::Spawned(id), EntityEvent::Spawned(id));
        assert_ne!(EntityEvent::Spawned(id), EntityEvent::Destroyed(id));
    }

    #[test]
    fn test_component_event_equality() {
        let id = EntityId::new(1, 0);
        let type_id = TypeId::of::<i32>();
        assert_eq!(
            ComponentEvent::Added(id, type_id),
            ComponentEvent::Added(id, type_id)
        );
        assert_ne!(
            ComponentEvent::Added(id, type_id),
            ComponentEvent::Removed(id, type_id)
        );
    }
}
