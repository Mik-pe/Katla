use katla_ecs::EntityId;
use katla_math::{Transform, Vec3};

/// Commands that scripts can emit to interact with the engine.
///
/// These commands are queued during script execution and processed by the
/// `ScriptSystem` after all scripts have run for the frame.
#[derive(Clone)]
pub enum ScriptCommand {
    /// Set the full transform (position, rotation, scale) of an entity.
    SetTransform(EntityId, Transform),
    /// Set only the position of an entity.
    SetPosition(EntityId, Vec3),
    /// Spawn a new entity.
    /// The `return_index` is used to match the spawned entity with the result
    /// when querying via `get_all_with` or similar.
    SpawnEntity { return_index: usize },
    /// Destroy an entity immediately.
    DestroyEntity(EntityId),
    /// Play a sound effect.
    PlaySound {
        /// Path to the sound file.
        path: String,
        /// Volume multiplier (0.0 to 1.0).
        volume: f32,
        /// Whether the sound should loop.
        looping: bool,
    },
    /// Play a sound effect at a specific position in 3D space.
    PlaySoundAt {
        /// Path to the sound file.
        path: String,
        /// Position in world space.
        position: Vec3,
        /// Volume multiplier (0.0 to 1.0).
        volume: f32,
        /// Whether the sound should loop.
        looping: bool,
    },
    /// Play a sound cue (pre-defined sound configuration).
    PlaySoundCue {
        /// Name of the sound cue as defined in the audio system.
        cue_name: String,
    },
    /// Perform a raycast in the physics world.
    /// Results can be retrieved on the next frame using `get_raycast_result`.
    Raycast {
        /// Origin point of the ray.
        origin: Vec3,
        /// Direction vector of the ray (should be normalized).
        direction: Vec3,
        /// Maximum distance to cast.
        max_distance: f32,
        /// Index used to retrieve the result from `PendingRaycastResults`.
        return_index: usize,
    },
    /// Apply a continuous force to a physics body.
    ApplyForce { entity_id: u64, force: [f32; 3] },
    /// Apply an instantaneous impulse to a physics body.
    ApplyImpulse { entity_id: u64, impulse: [f32; 3] },
    /// Set the linear velocity of a physics body.
    SetVelocity { entity_id: u64, velocity: [f32; 3] },
}
