use katla_ecs::EntityId;
use katla_ecs::scene_tool::{ComponentRegistry, FieldValue};
use serde::{Deserialize, Serialize};

/// Serialized scene context sent to the LLM as part of the conversation.
#[derive(Debug, Serialize, Deserialize)]
pub struct SceneContext {
    pub entity_count: usize,
    pub selected_entity: Option<EntityContext>,
    pub component_counts: Vec<ComponentCount>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EntityContext {
    pub id: u64,
    pub components: Vec<ComponentContext>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ComponentContext {
    pub type_name: String,
    pub fields: Vec<FieldContext>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FieldContext {
    pub name: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ComponentCount {
    pub type_name: String,
    pub count: usize,
}

/// Build a SceneContext from the current world state.
pub fn serialize_scene_context(
    world: &mut katla_ecs::World,
    registry: &ComponentRegistry,
    selected_entity: Option<EntityId>,
) -> SceneContext {
    let entity_count = world.entity_count();

    let mut component_counts = Vec::new();
    for entry in registry.entries() {
        let count = world
            .entity_ids()
            .filter(|&id| (entry.has_component)(world, id))
            .count();
        if count > 0 {
            component_counts.push(ComponentCount {
                type_name: entry.type_name.to_string(),
                count,
            });
        }
    }

    let selected_entity = selected_entity.and_then(|id| {
        if !world.entity_exists(id) {
            return None;
        }
        let mut components = Vec::new();
        for entry in registry.entries() {
            if !(entry.has_component)(world, id) {
                continue;
            }
            let fields = (entry.get_fields)(world, id);
            let mut field_contexts = Vec::new();
            for field_info in &fields {
                if let Some(value) = (entry.get_field_value)(world, id, field_info.name) {
                    field_contexts.push(FieldContext {
                        name: field_info.name.to_string(),
                        value: field_value_to_json(&value),
                    });
                }
            }
            components.push(ComponentContext {
                type_name: entry.type_name.to_string(),
                fields: field_contexts,
            });
        }
        Some(EntityContext {
            id: id.id(),
            components,
        })
    });

    SceneContext {
        entity_count,
        selected_entity,
        component_counts,
    }
}

fn field_value_to_json(value: &FieldValue) -> serde_json::Value {
    match value {
        FieldValue::F32(v) => serde_json::json!(*v),
        FieldValue::F64(v) => serde_json::json!(*v),
        FieldValue::I32(v) => serde_json::json!(*v),
        FieldValue::U32(v) => serde_json::json!(*v),
        FieldValue::Bool(v) => serde_json::json!(*v),
        FieldValue::String(v) => serde_json::json!(v),
        FieldValue::Unknown => serde_json::json!(null),
    }
}
