use super::tuning;
use katla_ecs::EntityId;
use katla_ecs::scene_tool::SceneOp;

#[test]
fn test_adjust_field_increase() {
    let entity = EntityId::from_raw(1);
    let op = tuning::adjust_field(entity, "PointLight", "intensity", 10.0, 1.5);

    if let SceneOp::SetField { value, .. } = op {
        let new_val = value.as_f64().unwrap() as f32;
        assert!(
            (new_val - 15.0).abs() < 0.01,
            "Expected 15.0, got {new_val}"
        );
    } else {
        panic!("Expected SetField");
    }
}

#[test]
fn test_adjust_field_decrease() {
    let entity = EntityId::from_raw(1);
    let op = tuning::adjust_field(entity, "PointLight", "intensity", 10.0, 0.5);

    if let SceneOp::SetField { value, .. } = op {
        let new_val = value.as_f64().unwrap() as f32;
        assert!((new_val - 5.0).abs() < 0.01, "Expected 5.0, got {new_val}");
    } else {
        panic!("Expected SetField");
    }
}

#[test]
fn test_set_field() {
    let entity = EntityId::from_raw(42);
    let op = tuning::set_field(entity, "PointLight", "range", 25.0);

    if let SceneOp::SetField {
        entity: e,
        component,
        field,
        value,
    } = op
    {
        assert_eq!(e, entity);
        assert_eq!(component, "PointLight");
        assert_eq!(field, "range");
        assert_eq!(value.as_f64().unwrap() as f32, 25.0);
    } else {
        panic!("Expected SetField");
    }
}

#[test]
fn test_create_variants_count() {
    let source = EntityId::from_raw(1);
    let ops = tuning::create_variants(source, "PointLight", "intensity", &[0.5, 1.0, 2.0], 3.0);
    assert_eq!(ops.len(), 3);

    for op in &ops {
        assert!(matches!(op, SceneOp::DuplicateEntity { .. }));
    }
}

#[test]
fn test_create_variants_spacing() {
    let source = EntityId::from_raw(1);
    let ops = tuning::create_variants(source, "PointLight", "intensity", &[1.0, 2.0], 5.0);

    if let SceneOp::DuplicateEntity {
        position_offset: Some(offset),
        ..
    } = &ops[0]
    {
        assert!((offset[0] - 5.0).abs() < 0.01);
    }

    if let SceneOp::DuplicateEntity {
        position_offset: Some(offset),
        ..
    } = &ops[1]
    {
        assert!((offset[0] - 10.0).abs() < 0.01);
    }
}

#[test]
fn test_presets_warm_light() {
    let (intensity, range) = tuning::presets::warm_light(10.0, 5.0);
    assert!((intensity - 12.0).abs() < 0.01);
    assert!((range - 5.5).abs() < 0.01);
}

#[test]
fn test_presets_cool_light() {
    let (intensity, range) = tuning::presets::cool_light(10.0, 5.0);
    assert!((intensity - 9.0).abs() < 0.01);
    assert!((range - 4.75).abs() < 0.01);
}

#[test]
fn test_presets_flickery() {
    let msg = tuning::presets::flickery();
    assert!(!msg.is_empty());
}
