//! Particle inspector widget for runtime particle emitter editing.

use super::ParticleStats;
use katla_ecs::EntityId;
use katla_ui::{ScrollAreaState, widgets::DraggablePanelState};

/// State for the particle inspector floating panel.
#[derive(Debug, Clone, Default)]
pub struct ParticleInspectorState {
    pub panel: DraggablePanelState,
    pub scroll_state: ScrollAreaState,
}

/// Pre-collected data for the particle inspector, gathered from World + GlobalParticleSystem.
#[derive(Debug, Clone, Default)]
pub struct ParticleInspectorData {
    pub emitter_entities: Vec<EntityId>,
    pub selected_emitter_config: Option<EmitterConfigView>,
    pub stats: Option<ParticleStats>,
}

/// Read-only view of emitter config for display in the inspector.
#[derive(Debug, Clone)]
pub struct EmitterConfigView {
    pub active: bool,
    pub shape_name: &'static str,
    pub shape_params: [f32; 3],
    pub emit_rate: f32,
    pub base_lifetime: f32,
    pub lifetime_variation: f32,
    pub velocity_magnitude: f32,
    pub velocity_cone_angle: f32,
    pub base_scale: f32,
    pub scale_variation: f32,
    pub color: [f32; 4],
    pub color_variation: f32,
    pub gravity: f32,
    pub turbulence_strength: f32,
    pub turbulence_frequency: f32,
}

/// Actions emitted by the particle inspector.
#[derive(Debug, Clone)]
pub enum ParticleInspectorAction {
    SelectEmitter(EntityId),
    ToggleEmitter,
    ResetSystem,
    Close,
}
