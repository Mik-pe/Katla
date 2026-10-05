use super::*;
use crate::scene::{EntityDescriptor, EntitySource};

pub(crate) fn template() -> Prefab {
    let mut scene = Scene::new("Reusable object");
    scene.next_entity_id = 3;
    scene
        .entities
        .push(EntityDescriptor::new(SceneEntityId(1), EntitySource::Empty));
    let mut child = EntityDescriptor::new(SceneEntityId(2), EntitySource::Cube { size: [1.0; 3] });
    child.parent = Some(SceneEntityId(1));
    scene.entities.push(child);
    Prefab {
        version: PREFAB_VERSION,
        root: SceneEntityId(1),
        scene,
    }
}

#[test]
fn test_prefab_ron_json_and_rooted_tree_contract() {
    let prefab = template();
    prefab.validate().unwrap();
    let text = ron::ser::to_string_pretty(&prefab, crate::scene::ron_pretty_config()).unwrap();
    assert_eq!(Prefab::parse(&text).unwrap(), prefab);
    assert_eq!(
        serde_json::from_value::<Prefab>(serde_json::to_value(&prefab).unwrap()).unwrap(),
        prefab
    );
    let mut invalid = prefab.clone();
    invalid.root = SceneEntityId(99);
    assert!(invalid.validate().is_err());
    let mut invalid = prefab.clone();
    invalid.scene.entities[1].parent = None;
    assert!(invalid.validate().is_err());
    let mut invalid = prefab.clone();
    invalid.scene.entities[0].transform.position[0] = 1.0;
    assert!(invalid.validate().is_err());
    let mut invalid = prefab;
    invalid.version += 1;
    assert!(invalid.validate().is_err());
}

#[test]
fn test_prefab_rejects_external_references_and_cycles_before_instantiation() {
    let mut prefab = template();
    prefab.scene.entities[1].parent = Some(SceneEntityId(99));
    assert!(prefab.validate().is_err());
    prefab.scene.entities[1].parent = Some(SceneEntityId(2));
    assert!(prefab.validate().is_err());
    prefab = template();
    prefab.scene.entities[1]
        .trigger_rules
        .push(katla_agent::events::TriggerRule {
            event: katla_agent::events::TriggerPhase::Enter,
            other_entity: Some(SceneEntityId(99)),
            once: true,
            actions: vec![katla_agent::events::EventAction::Emit {
                name: "outside".into(),
            }],
        });
    assert!(prefab.validate().is_err());
}

#[test]
fn test_describe_examples_match_actual_engine_schemas() {
    let example = control::describe().unwrap();
    let mesh: crate::mesh_asset::MeshAsset =
        serde_json::from_value(example["mesh_example"].clone()).unwrap();
    mesh.compile().unwrap();
    let prefab: Prefab = serde_json::from_value(example["prefab_example"].clone()).unwrap();
    prefab.validate().unwrap();
}
