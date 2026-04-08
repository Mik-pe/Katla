use katla_ecs::EntityId;
use katla_ecs::scene_tool::SceneOp;

/// Adjust a field value by a relative amount (e.g., +10%, -20%).
/// Returns a `SetField` op with the new absolute value.
pub fn adjust_field(
    entity: EntityId,
    component: &str,
    field: &str,
    current_value: f32,
    factor: f32,
) -> SceneOp {
    SceneOp::SetField {
        entity,
        component: component.to_string(),
        field: field.to_string(),
        value: serde_json::json!(current_value * factor),
    }
}

/// Set a field to a specific value.
pub fn set_field(entity: EntityId, component: &str, field: &str, value: f32) -> SceneOp {
    SceneOp::SetField {
        entity,
        component: component.to_string(),
        field: field.to_string(),
        value: serde_json::json!(value),
    }
}

/// Generate A/B comparison variants: create duplicates with different position offsets.
/// Returns `DuplicateEntity` ops for each variant value.
///
/// Note: Setting the field on each duplicate requires a second pass after execution,
/// since the duplicate's `EntityId` isn't known until the executor runs.
pub fn create_variants(
    source: EntityId,
    _component: &str,
    _field: &str,
    values: &[f32],
    spacing: f32,
) -> Vec<SceneOp> {
    values
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let offset = [spacing * (i as f32 + 1.0), 0.0, 0.0];
            SceneOp::DuplicateEntity {
                entity: source,
                position_offset: Some(offset),
            }
        })
        .collect()
}

/// Semantic tuning presets for common adjustments.
pub mod presets {
    /// Warm up a light: increase intensity and range.
    pub fn warm_light(intensity: f32, range: f32) -> (f32, f32) {
        (intensity * 1.2, range * 1.1)
    }

    /// Cool down a light: decrease intensity and range.
    pub fn cool_light(intensity: f32, range: f32) -> (f32, f32) {
        (intensity * 0.9, range * 0.95)
    }

    /// Suggestion for making light flickery.
    pub fn flickery() -> &'static str {
        "Consider varying intensity between 0.5x and 1.5x per frame"
    }
}
