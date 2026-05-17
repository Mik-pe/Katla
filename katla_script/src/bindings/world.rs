use katla_ecs::EntityId;
use katla_math::{Transform, Vec3};

#[derive(Clone)]
pub enum ScriptCommand {
    SetTransform(EntityId, Transform),
    SetPosition(EntityId, Vec3),
    SpawnEntity { return_index: usize },
    DestroyEntity(EntityId),
}
