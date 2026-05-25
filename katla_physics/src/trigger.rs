//! Trigger volume component for overlap detection without collision response.

use katla_ecs::Component;
use serde::{Deserialize, Serialize};

/// A marker component that makes a collider act as a sensor (trigger volume).
///
/// When present on an entity with a `ColliderShape`, the collider is created
/// with Rapier's sensor flag enabled. Sensor colliders do not generate collision
/// response forces but report overlap events (enter/exit) each physics step.
#[derive(Component, Debug, Clone, Default, Serialize, Deserialize)]
pub struct TriggerVolume {
    #[serde(skip)]
    pub overlapping_entities: Vec<u64>,
}

/// Collision event emitted when a trigger volume overlap state changes.
#[derive(Debug, Clone)]
pub enum TriggerEvent {
    Enter {
        trigger_entity: u64,
        other_entity: u64,
    },
    Exit {
        trigger_entity: u64,
        other_entity: u64,
    },
}

impl TriggerVolume {
    pub fn new() -> Self {
        Self::default()
    }
}
