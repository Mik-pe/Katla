//! UI integration module.
//!
//! This module provides the bridge between katla_ui and the application layer.

pub mod console;
#[cfg(feature = "editor")]
mod editor_ui;
#[cfg(feature = "editor")]
mod particle_inspector;
#[cfg(feature = "editor")]
mod particle_stats;
pub mod renderer;

#[cfg(feature = "editor")]
pub use editor_ui::{
    AudioEmitterInfo, AudioSourceInfo, ColliderShapeInfo, ColliderShapeType,
    DirectionalLightInfo, EditorAction, EditorRenderParams,
    EditorUI, EntityInfo, FocusedPanel, InspectorEditState, Panel, ParticleEmitterInfo,
    PerspectiveInfo, PhysicsMaterialInfo, PointLightInfo,
    RigidBodyInfo, RigidBodyType, SpawnableModel, ThumbnailState,
};
pub use katla_ui::ColorScheme;
#[cfg(feature = "editor")]
pub use particle_inspector::{
    EmitterConfigView, EmitterField, ParticleInspectorAction, ParticleInspectorData,
    ParticleInspectorState,
};
#[cfg(feature = "editor")]
pub use particle_stats::ParticleStats;
pub use renderer::UIRenderer;
