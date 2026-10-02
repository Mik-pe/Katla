pub mod assets;
pub(crate) mod capture;
pub mod component_registry;
pub mod default_scene;
pub mod descriptors;
pub(crate) mod document;
pub mod entity_source;
pub mod error;
#[cfg(test)]
mod format_tests;
pub mod identity;
pub mod migration;
pub mod serialization;
mod spawn;
#[cfg(test)]
mod tests;
pub mod validation;

pub use default_scene::{DEFAULT_SCENE_PATH, build_default_scene, default_scene_path};
pub use descriptors::{
    AnimationDescriptor, ColliderShapeDescriptor, CollisionFilterDescriptor,
    CustomComponentDescriptor, DrawableDescriptor, EntityDescriptor, JointDescriptor,
    ParticleEmitterDescriptor, PerspectiveDescriptor, PhysicsMaterialDescriptor,
    PointLightDescriptor, RigidBodyDescriptor, Scene, ScriptDescriptor, TransformDescriptor,
    TriggerVolumeDescriptor, VelocityDescriptor,
};
pub use entity_source::EntitySource;
pub use serialization::{SCENE_VERSION, SceneManager, ron_pretty_config};

pub use assets::{AssetRef, SceneAssetContext};
pub use component_registry::{SceneComponentRegistry, SceneReadContext, SceneWriteContext};
pub use error::{SceneError, SceneIssue};
pub use identity::SceneEntityId;
