use katla_ecs::EntityId;
use katla_ecs::scene_tool::SceneOp;

/// A two-phase plan for creating entity variants with different field values.
///
/// Phase 1: Execute `duplicates` to create new entities and collect their IDs.
/// Phase 2: For each resulting entity, apply the corresponding field setting from
/// `field_sets` (indexed by position).
pub struct VariantsPlan {
    /// Duplicate ops to execute first.
    pub duplicates: Vec<SceneOp>,
    /// Per-variant field settings: (component, field, value) for each duplicate.
    pub field_sets: Vec<(String, String, f32)>,
}

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

/// Generate A/B comparison variants: duplicate an entity and assign different field values.
///
/// Returns a [`VariantsPlan`] containing duplicate ops and the field settings to apply
/// after execution. The caller must execute phase 1 (duplicates), collect the resulting
/// entity IDs, then execute phase 2 (`SetField` ops using those IDs).
pub fn create_variants(
    source: EntityId,
    component: &str,
    field: &str,
    values: &[f32],
    spacing: f32,
) -> VariantsPlan {
    let duplicates: Vec<SceneOp> = values
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let offset = [spacing * (i as f32 + 1.0), 0.0, 0.0];
            SceneOp::DuplicateEntity {
                entity: source,
                position_offset: Some(offset),
            }
        })
        .collect();

    let field_sets: Vec<(String, String, f32)> = values
        .iter()
        .map(|v| (component.to_string(), field.to_string(), *v))
        .collect();

    VariantsPlan {
        duplicates,
        field_sets,
    }
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
