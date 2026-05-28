//! Particle inspector widget for runtime particle emitter editing.

use super::ParticleStats;
use katla_ecs::EntityId;
use katla_ui::widgets::DraggablePanelState;

/// State for the particle inspector floating panel.
#[derive(Debug, Clone, Default)]
pub struct ParticleInspectorState {
    pub panel: DraggablePanelState,
}

/// Pre-collected data for the particle inspector, gathered from World + GlobalParticleSystem.
#[derive(Debug, Clone, Default)]
pub struct ParticleInspectorData {
    pub emitter_entities: Vec<EntityId>,
    pub selected_emitter_entity: Option<EntityId>,
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
    pub color_end: [f32; 4],
    pub scale_end: f32,
    pub gravity: f32,
    pub turbulence_strength: f32,
    pub turbulence_frequency: f32,
}

/// Identifies which emitter config field changed and its new value.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum EmitterField {
    EmitRate(f32),
    BaseLifetime(f32),
    LifetimeVariation(f32),
    VelocityMagnitude(f32),
    VelocityConeAngle(f32),
    BaseScale(f32),
    ScaleVariation(f32),
    Gravity(f32),
    TurbulenceStrength(f32),
    TurbulenceFrequency(f32),
    Color([f32; 4]),
    ColorVariation(f32),
    ColorEnd([f32; 4]),
    ScaleEnd(f32),
    ShapePoint,
    ShapeLine,
    ShapeCircle,
    ShapeSphere,
    ShapeBox,
    ShapeParam0(f32),
    ShapeParam1(f32),
    ShapeParam2(f32),
}

/// Actions emitted by the particle inspector.
#[derive(Debug, Clone)]
pub enum ParticleInspectorAction {
    SelectEmitter(EntityId),
    ToggleEmitter,
    ResetSystem,
    Close,
    SetEmitterField(EntityId, EmitterField),
}
