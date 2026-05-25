use katla_ecs::EntityId;
use katla_math::{Transform, Vec3};

#[derive(Clone)]
pub enum ScriptCommand {
    SetTransform(EntityId, Transform),
    SetPosition(EntityId, Vec3),
    SpawnEntity {
        return_index: usize,
    },
    DestroyEntity(EntityId),
    PlaySound {
        path: String,
        volume: f32,
        looping: bool,
    },
    PlaySoundAt {
        path: String,
        position: Vec3,
        volume: f32,
        looping: bool,
    },
    PlaySoundCue {
        cue_name: String,
    },
    Raycast {
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
        return_index: usize,
    },
}
