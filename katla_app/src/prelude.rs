//! Prelude module re-exporting common types for game makers.
//!
//! Import with `use katla_app::prelude::*;` to get all the types
//! typically needed for building a game.

pub use crate::application::ApplicationBuilder;
pub use crate::error::{AppError, AppResult};
pub use crate::rendering::FrameContext;
pub use crate::spawner::Spawner;

// Components
pub use crate::components::camera::{
    FlyCameraControllerComponent, FlyCameraLookComponent, FocusTarget,
    OrbitCameraControllerComponent, PerspectiveComponent,
};
pub use crate::components::lighting::{DirectionalLight, PointLight};
pub use crate::components::particle::ParticleEmitterComponent;
pub use crate::components::physics::{
    DragComponent, ForceComponent, MassComponent, VelocityComponent,
};
pub use crate::components::rendering::DrawableComponent;
pub use crate::components::scene::{Children, EditorHidden, NameComponent, Parent};
pub use crate::components::transform::{TransformComponent, TransformDirty, WorldTransform};

// Systems
pub use crate::systems::camera::{FlyCameraLookSystem, OrbitCameraSystem};
pub use crate::systems::particle_system::ParticleSystem;
pub use crate::systems::physics::{PhysicsSystem, VelocitySystem};
pub use crate::systems::transform::{TransformHierarchySystem, TransformOptimization};

// Animation
pub use crate::animation::{
    AnimatedModel, AnimationChannel, AnimationClip, AnimationEvent, AnimationManager,
    AnimationPlayer, AnimationUpdateSystem, CachedSampler, ChannelPath, Interpolation,
    JointTransform, JointWeights, MorphTargetSystem, MorphTargetWeights, SampleBuffer,
    SampledValue, Skeleton, Skin,
};
