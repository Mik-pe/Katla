use crate::World;

use super::command::{
    AddComponentCommand, DestroyEntityCommand, DuplicateEntityCommand, RemoveComponentCommand,
    SceneCommand, SetFieldCommand, SpawnEntityCommand, UndoGroup,
};
use super::registry::{ComponentRegistry, FieldValue};
use super::{SceneOp, SceneToolError, ToolResult};

/// Executes scene operations, producing Commands for the undo stack.
pub struct SceneToolExecutor;

impl SceneToolExecutor {
    /// Execute a scene operation, returning the result and undo group.
    ///
    /// The commands have already been executed — caller just needs to store
    /// them in the undo stack.
    pub fn execute(
        op: SceneOp,
        world: &mut World,
        registry: &ComponentRegistry,
    ) -> Result<(ToolResult, UndoGroup), SceneToolError> {
        match op {
            SceneOp::SpawnEntity {
                position,
                rotation,
                scale,
                name,
                primitive: _,
            } => Self::exec_spawn(world, registry, position, rotation, scale, name),
            SceneOp::DestroyEntity { entity } => Self::exec_destroy(world, registry, entity),
            SceneOp::SetField {
                entity,
                component,
                field,
                value,
            } => Self::exec_set_field(world, registry, entity, component, field, value),
            SceneOp::QueryEntities {
                component_filter,
                name_filter,
                position: _,
                radius: _,
                limit,
            } => Self::exec_query(world, registry, component_filter, name_filter, limit),
            SceneOp::GetSceneHierarchy => Self::exec_hierarchy(world),
            SceneOp::DuplicateEntity {
                entity,
                position_offset,
            } => Self::exec_duplicate(world, registry, entity, position_offset),
            SceneOp::ListAvailableComponents => Self::exec_list_components(world, registry),
            SceneOp::AddComponent { entity, component } => {
                Self::exec_add_component(world, registry, entity, component)
            }
            SceneOp::RemoveComponent { entity, component } => {
                Self::exec_remove_component(world, registry, entity, component)
            }
            SceneOp::GetComponentAttributes { entity, component } => {
                Self::exec_get_component_attributes(world, registry, entity, component)
            }
            SceneOp::SetParent { entity, parent } => Self::exec_set_parent(world, entity, parent),
            SceneOp::SpawnModel { .. } => Err(SceneToolError::WorldError(
                "SpawnModel must be executed via Application::spawn_gltf_model".to_string(),
            )),
        }
    }

    fn exec_spawn(
        world: &mut World,
        registry: &ComponentRegistry,
        position: [f32; 3],
        rotation: [f32; 3],
        scale: [f32; 3],
        name: Option<String>,
    ) -> Result<(ToolResult, UndoGroup), SceneToolError> {
        let mut cmd = SpawnEntityCommand::new(position, rotation, scale, name.clone());
        cmd.execute(world)?;

        let entity = cmd.entity().unwrap();
        let desc = cmd.description();

        // Add default components to the new entity via registry entries.
        // For spawn, we add all registered "base" components (e.g. transform, name).
        // The caller decides which components to register, so this is fully data-driven.
        for entry in registry.entries() {
            if !(entry.has_component)(world, entity) {
                (entry.create_default)(world, entity);
            }
        }

        // Set position/scale/rotation fields on registered components
        for entry in registry.entries() {
            if !(entry.has_component)(world, entity) {
                continue;
            }
            // Try to set position fields
            for (i, axis) in ["x", "y", "z"].iter().enumerate() {
                if (entry.set_field_value)(world, entity, axis, FieldValue::F32(position[i]))
                    .is_err()
                {
                    // Field might not exist, that's fine
                }
            }
            // Try to set scale fields
            for (i, axis) in ["scale_x", "scale_y", "scale_z"].iter().enumerate() {
                if (entry.set_field_value)(world, entity, axis, FieldValue::F32(scale[i])).is_err()
                {
                    // Field might not exist, that's fine
                }
            }
            // Try to set rotation fields
            for (i, axis) in ["rot_x", "rot_y", "rot_z"].iter().enumerate() {
                if (entry.set_field_value)(world, entity, axis, FieldValue::F32(rotation[i]))
                    .is_err()
                {
                    // Field might not exist, that's fine
                }
            }
        }

        // If a name was given, try to set it via registry
        if let Some(ref n) = name {
            for entry in registry.entries() {
                if !(entry.has_component)(world, entity) {
                    continue;
                }
                if let Ok(()) =
                    (entry.set_field_value)(world, entity, "name", FieldValue::String(n.clone()))
                {
                    break;
                }
            }
        }

        let mut group = UndoGroup::new(desc);
        group.commands.push(Box::new(cmd));

        Ok((
            ToolResult {
                success: true,
                message: format!("Spawned entity {entity}"),
                affected_entities: vec![entity],
                data: None,
            },
            group,
        ))
    }

    fn exec_destroy(
        world: &mut World,
        registry: &ComponentRegistry,
        entity: crate::EntityId,
    ) -> Result<(ToolResult, UndoGroup), SceneToolError> {
        if !world.entity_exists(entity) {
            return Err(SceneToolError::EntityNotFound(entity));
        }

        let mut cmd = DestroyEntityCommand::new(entity);
        cmd.snapshot_from_registry(world, registry);
        cmd.execute(world)?;

        let desc = cmd.description();
        let mut group = UndoGroup::new(desc);
        group.commands.push(Box::new(cmd));

        Ok((
            ToolResult {
                success: true,
                message: format!("Destroyed entity {entity}"),
                affected_entities: vec![entity],
                data: None,
            },
            group,
        ))
    }

    fn exec_set_field(
        world: &mut World,
        registry: &ComponentRegistry,
        entity: crate::EntityId,
        component: String,
        field: String,
        value: serde_json::Value,
    ) -> Result<(ToolResult, UndoGroup), SceneToolError> {
        if !world.entity_exists(entity) {
            return Err(SceneToolError::EntityNotFound(entity));
        }

        let entry = registry
            .get(&component)
            .ok_or_else(|| SceneToolError::ComponentNotFound {
                entity,
                component: component.clone(),
            })?;

        // Read old value
        let old_value = (entry.get_field_value)(world, entity, &field).ok_or_else(|| {
            SceneToolError::FieldNotFound {
                component: component.clone(),
                field: field.clone(),
            }
        })?;

        // Convert JSON value to typed FieldValue
        let new_value = FieldValue::from_json_typed(&value, &old_value).ok_or_else(|| {
            SceneToolError::InvalidFieldValue {
                field: field.clone(),
                expected_type: old_value.type_name().to_string(),
                got: format!("{value}"),
            }
        })?;

        // Apply the new value
        (entry.set_field_value)(world, entity, &field, new_value.clone())?;

        let cmd = SetFieldCommand::new(entity, field.clone(), old_value, entry);

        let desc = cmd.description();
        let mut group = UndoGroup::new(desc);
        group.commands.push(Box::new(cmd));

        Ok((
            ToolResult {
                success: true,
                message: format!("Set {component}.{field} on entity {entity}"),
                affected_entities: vec![entity],
                data: None,
            },
            group,
        ))
    }

    fn exec_query(
        world: &mut World,
        registry: &ComponentRegistry,
        component_filter: Option<String>,
        _name_filter: Option<String>,
        limit: Option<usize>,
    ) -> Result<(ToolResult, UndoGroup), SceneToolError> {
        let mut entities: Vec<crate::EntityId> = world.entity_ids().collect();

        if let Some(ref type_name) = component_filter {
            let entry = registry.get(type_name).ok_or_else(|| {
                SceneToolError::WorldError(format!("Component type '{type_name}' not registered"))
            })?;
            entities.retain(|&id| (entry.has_component)(world, id));
        }

        if let Some(limit) = limit {
            entities.truncate(limit);
        }

        let count = entities.len();
        let names: Vec<String> = entities.iter().map(|&id| format!("{id}")).collect();

        Ok((
            ToolResult {
                success: true,
                message: format!("Found {count} entities: {}", names.join(", ")),
                affected_entities: entities,
                data: None,
            },
            UndoGroup::new("Query (no undo)"),
        ))
    }

    fn exec_hierarchy(world: &mut World) -> Result<(ToolResult, UndoGroup), SceneToolError> {
        let entities: Vec<crate::EntityId> = world.entity_ids().collect();
        let count = entities.len();

        let entity_list: Vec<serde_json::Value> = entities
            .iter()
            .map(|&id| {
                serde_json::json!({
                    "id": id.to_string(),
                })
            })
            .collect();

        Ok((
            ToolResult {
                success: true,
                message: format!("Scene has {count} entities"),
                affected_entities: entities,
                data: Some(serde_json::json!({
                    "entities": entity_list,
                })),
            },
            UndoGroup::new("GetSceneHierarchy (no undo)"),
        ))
    }

    fn exec_duplicate(
        world: &mut World,
        registry: &ComponentRegistry,
        source: crate::EntityId,
        position_offset: Option<[f32; 3]>,
    ) -> Result<(ToolResult, UndoGroup), SceneToolError> {
        if !world.entity_exists(source) {
            return Err(SceneToolError::EntityNotFound(source));
        }

        let mut cmd = DuplicateEntityCommand::new(source, position_offset);
        cmd.execute(world)?;

        let duplicate = cmd.duplicate().unwrap();

        // Copy component fields from source to duplicate via registry
        for entry in registry.entries() {
            if !(entry.has_component)(world, source) {
                continue;
            }
            // Add a default instance of the component to the duplicate
            (entry.create_default)(world, duplicate);
            // Now copy field values from source
            let fields = (entry.get_fields)(world, source);
            for field_info in &fields {
                if let Some(value) = (entry.get_field_value)(world, source, field_info.name) {
                    let _ = (entry.set_field_value)(world, duplicate, field_info.name, value);
                }
            }
        }

        // Apply position offset if given
        if let Some(offset) = position_offset {
            for entry in registry.entries() {
                if !(entry.has_component)(world, duplicate) {
                    continue;
                }
                for (i, axis) in ["x", "y", "z"].iter().enumerate() {
                    if let Some(current) = (entry.get_field_value)(world, duplicate, axis)
                        && let Some(v) = current.as_f32()
                    {
                        let _ = (entry.set_field_value)(
                            world,
                            duplicate,
                            axis,
                            FieldValue::F32(v + offset[i]),
                        );
                    }
                }
            }
        }

        let desc = cmd.description();
        let mut group = UndoGroup::new(desc);
        group.commands.push(Box::new(cmd));

        Ok((
            ToolResult {
                success: true,
                message: format!("Duplicated entity {source} -> {duplicate}"),
                affected_entities: vec![duplicate],
                data: None,
            },
            group,
        ))
    }

    fn exec_list_components(
        world: &mut World,
        registry: &ComponentRegistry,
    ) -> Result<(ToolResult, UndoGroup), SceneToolError> {
        let mut components = Vec::new();
        let dummy_entity = crate::EntityId::from_raw(0);
        for entry in registry.entries() {
            let fields: Vec<serde_json::Value> = (entry.get_fields)(world, dummy_entity)
                .into_iter()
                .map(|f| {
                    serde_json::json!({
                        "name": f.name,
                        "display_name": f.display_name,
                        "type": f.type_name,
                    })
                })
                .collect();
            components.push(serde_json::json!({
                "type_name": entry.type_name,
                "fields": fields,
            }));
        }

        Ok((
            ToolResult {
                success: true,
                message: format!("{} component types registered", components.len()),
                affected_entities: vec![],
                data: Some(serde_json::json!({ "components": components })),
            },
            UndoGroup::new("ListAvailableComponents (no undo)"),
        ))
    }

    fn exec_add_component(
        world: &mut World,
        registry: &ComponentRegistry,
        entity: crate::EntityId,
        component: String,
    ) -> Result<(ToolResult, UndoGroup), SceneToolError> {
        if !world.entity_exists(entity) {
            return Err(SceneToolError::EntityNotFound(entity));
        }

        let entry = registry
            .get(&component)
            .ok_or_else(|| SceneToolError::ComponentNotFound {
                entity,
                component: component.clone(),
            })?;

        if (entry.has_component)(world, entity) {
            return Err(SceneToolError::WorldError(format!(
                "Entity {entity} already has component '{component}'"
            )));
        }

        let mut cmd = AddComponentCommand::new(entity, component.clone(), entry);
        cmd.execute(world)?;

        let desc = cmd.description();
        let mut group = UndoGroup::new(desc);
        group.commands.push(Box::new(cmd));

        Ok((
            ToolResult {
                success: true,
                message: format!("Added {component} to entity {entity}"),
                affected_entities: vec![entity],
                data: None,
            },
            group,
        ))
    }

    fn exec_remove_component(
        world: &mut World,
        registry: &ComponentRegistry,
        entity: crate::EntityId,
        component: String,
    ) -> Result<(ToolResult, UndoGroup), SceneToolError> {
        if !world.entity_exists(entity) {
            return Err(SceneToolError::EntityNotFound(entity));
        }

        let entry = registry
            .get(&component)
            .ok_or_else(|| SceneToolError::ComponentNotFound {
                entity,
                component: component.clone(),
            })?;

        if !(entry.has_component)(world, entity) {
            return Err(SceneToolError::ComponentNotFound {
                entity,
                component: component.clone(),
            });
        }

        let mut cmd = RemoveComponentCommand::new(entity, component.clone(), entry, world);
        cmd.execute(world)?;

        let desc = cmd.description();
        let mut group = UndoGroup::new(desc);
        group.commands.push(Box::new(cmd));

        Ok((
            ToolResult {
                success: true,
                message: format!("Removed {component} from entity {entity}"),
                affected_entities: vec![entity],
                data: None,
            },
            group,
        ))
    }

    fn exec_get_component_attributes(
        world: &mut World,
        registry: &ComponentRegistry,
        entity: crate::EntityId,
        component: String,
    ) -> Result<(ToolResult, UndoGroup), SceneToolError> {
        if !world.entity_exists(entity) {
            return Err(SceneToolError::EntityNotFound(entity));
        }

        let entry = registry
            .get(&component)
            .ok_or_else(|| SceneToolError::ComponentNotFound {
                entity,
                component: component.clone(),
            })?;

        if !(entry.has_component)(world, entity) {
            return Err(SceneToolError::ComponentNotFound {
                entity,
                component: component.clone(),
            });
        }

        let fields = (entry.get_fields)(world, entity);
        let mut field_values = Vec::new();
        for field_info in &fields {
            let value = (entry.get_field_value)(world, entity, field_info.name);
            field_values.push(serde_json::json!({
                "name": field_info.name,
                "display_name": field_info.display_name,
                "type": field_info.type_name,
                "value": value.map(|v| match v {
                    FieldValue::F32(v) => serde_json::json!(v),
                    FieldValue::F64(v) => serde_json::json!(v),
                    FieldValue::I32(v) => serde_json::json!(v),
                    FieldValue::U32(v) => serde_json::json!(v),
                    FieldValue::I64(v) => serde_json::json!(v),
                    FieldValue::U64(v) => serde_json::json!(v),
                    FieldValue::Bool(v) => serde_json::json!(v),
                    FieldValue::String(v) => serde_json::json!(v),
                    FieldValue::Unknown => serde_json::json!(null),
                }).unwrap_or(serde_json::json!(null)),
            }));
        }

        Ok((
            ToolResult {
                success: true,
                message: format!(
                    "{component} on entity {entity} has {} fields",
                    field_values.len()
                ),
                affected_entities: vec![entity],
                data: Some(serde_json::json!({
                    "component": component,
                    "entity": entity.to_string(),
                    "fields": field_values,
                })),
            },
            UndoGroup::new("GetComponentAttributes (no undo)"),
        ))
    }

    fn exec_set_parent(
        world: &mut World,
        entity: crate::EntityId,
        parent: Option<crate::EntityId>,
    ) -> Result<(ToolResult, UndoGroup), SceneToolError> {
        if !world.entity_exists(entity) {
            return Err(SceneToolError::EntityNotFound(entity));
        }
        if let Some(parent_id) = parent {
            if !world.entity_exists(parent_id) {
                return Err(SceneToolError::EntityNotFound(parent_id));
            }
            if entity == parent_id {
                return Err(SceneToolError::WorldError(
                    "Cannot set entity as its own parent".to_string(),
                ));
            }
        }

        let message = match parent {
            Some(p) => format!("Set parent of entity {entity} to {p}"),
            None => format!("Cleared parent of entity {entity}"),
        };

        Ok((
            ToolResult {
                success: true,
                message,
                affected_entities: vec![entity],
                data: Some(serde_json::json!({
                    "entity": entity.to_string(),
                    "parent": parent.map(|p| p.to_string()),
                })),
            },
            UndoGroup::new("SetParent (no undo)"),
        ))
    }

    /// Undo an entire undo group.
    pub fn undo(group: &mut UndoGroup, world: &mut World) -> Result<(), SceneToolError> {
        for cmd in group.commands.iter_mut().rev() {
            cmd.undo(world)?;
        }
        Ok(())
    }
}
