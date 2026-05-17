use katla_ecs::EntityId;
use katla_math::{Transform, Vec3};

#[derive(Clone)]
pub enum ScriptCommand {
    SetTransform(EntityId, Transform),
    SetPosition(EntityId, Vec3),
    SpawnEntity { return_index: usize },
    DestroyEntity(EntityId),
}

pub trait ScriptWorldAccess {
    fn get_transform(&self, entity: EntityId) -> Option<Transform>;
    fn set_transform(&mut self, entity: EntityId, transform: Transform);
    fn entity_exists(&self, entity: EntityId) -> bool;
    fn spawn_entity(&mut self) -> EntityId;
    fn destroy_entity(&mut self, entity: EntityId);
}
