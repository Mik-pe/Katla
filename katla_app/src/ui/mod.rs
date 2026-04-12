//! UI integration module.
//!
//! This module provides the bridge between katla_ui and the application layer.

#[cfg(feature = "editor")]
mod editor_ui;
#[cfg(feature = "editor")]
mod particle_inspector;
#[cfg(feature = "editor")]
mod particle_stats;
pub mod renderer;

#[cfg(feature = "editor")]
pub use editor_ui::{
    EditorAction, EditorRenderParams, EditorUI, EntityInfo, FocusedPanel, ParticleEmitterInfo,
    PointLightInfo, SpawnableModel, ThumbnailState, inspector::InspectorEditState,
};
pub use katla_ui::ColorScheme;
#[cfg(feature = "editor")]
pub use particle_inspector::{
    EmitterConfigView, ParticleInspector, ParticleInspectorAction, ParticleInspectorData,
    ParticleInspectorState,
};
#[cfg(feature = "editor")]
pub use particle_stats::ParticleStats;
pub use renderer::UIRenderer;
