pub mod default_scene;
pub mod descriptors;
pub mod entity_source;
pub mod migration;
pub mod serialization;
#[cfg(test)]
mod tests;

pub use default_scene::{DEFAULT_SCENE_PATH, build_default_scene};
pub use descriptors::{
    AnimationDescriptor, ColliderShapeDescriptor, CollisionFilterDescriptor, DrawableDescriptor,
    EntityDescriptor, ParticleEmitterDescriptor, PerspectiveDescriptor, PhysicsMaterialDescriptor,
    PointLightDescriptor, RigidBodyDescriptor, Scene, ScriptDescriptor, TransformDescriptor,
    TriggerVolumeDescriptor, VelocityDescriptor,
};
pub use entity_source::EntitySource;
pub use serialization::{SCENE_VERSION, SceneManager, ron_pretty_config};
