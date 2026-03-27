//! UI integration module.
//!
//! This module provides the bridge between katla_ui and the application layer.

mod editor_ui;
mod particle_inspector;
mod particle_stats;
pub mod renderer;
pub mod theme;

pub use editor_ui::inspector::InspectorEditState;
pub use editor_ui::{
    EditorAction, EditorUI, EntityInfo, FocusedPanel, ParticleEmitterInfo, PointLightInfo,
    SpawnableModel, ThumbnailState,
};
pub use particle_inspector::{
    EmitterConfigView, ParticleInspector, ParticleInspectorAction, ParticleInspectorData,
    ParticleInspectorState,
};
pub use particle_stats::ParticleStats;
pub use renderer::UIRenderer;
pub use theme::Theme;
