use super::templates;
use katla_ecs::scene_tool::SceneOp;

#[test]
fn test_campfire_template() {
    let tmpl = templates::campfire([1.0, 2.0, 3.0]);
    assert_eq!(tmpl.name, "campfire");
    assert_eq!(tmpl.ops.len(), 1);

    if let SceneOp::SpawnEntity {
        position,
        name: Some(name),
        ..
    } = &tmpl.ops[0]
    {
        assert_eq!(*position, [1.0, 2.0, 3.0]);
        assert_eq!(name, "Campfire");
    } else {
        panic!("Expected SpawnEntity with name");
    }
}

#[test]
fn test_street_lamp_template() {
    let tmpl = templates::street_lamp([5.0, 3.0, 1.0]);
    assert_eq!(tmpl.name, "street_lamp");
    assert_eq!(tmpl.ops.len(), 1);

    if let SceneOp::SpawnEntity {
        name: Some(name), ..
    } = &tmpl.ops[0]
    {
        assert_eq!(name, "StreetLamp");
    } else {
        panic!("Expected SpawnEntity with name");
    }
}

#[test]
fn test_forest_clearing() {
    let tmpl = templates::forest_clearing([0.0, 0.0, 0.0], 6, 10.0);
    assert_eq!(tmpl.name, "forest_clearing");
    assert_eq!(tmpl.ops.len(), 6);
}

#[test]
fn test_village_square() {
    let tmpl = templates::village_square([0.0, 0.0, 0.0], 3, 2.0);
    assert_eq!(tmpl.name, "village_square");
    assert_eq!(tmpl.ops.len(), 9); // 3x3 grid
}

#[test]
fn test_available_templates() {
    let list = templates::available_templates();
    assert!(!list.is_empty());
    assert!(list.iter().any(|(name, _)| *name == "campfire"));
    assert!(list.iter().any(|(name, _)| *name == "street_lamp"));
    assert!(list.iter().any(|(name, _)| *name == "village_square"));
}
