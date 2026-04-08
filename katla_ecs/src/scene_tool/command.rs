use crate::{EntityId, World};

use super::SceneToolError;
use super::registry::{ComponentRegistryEntry, FieldValue};

/// A reversible scene mutation.
pub trait SceneCommand {
    fn execute(&mut self, world: &mut World) -> Result<(), SceneToolError>;
    fn undo(&mut self, world: &mut World) -> Result<(), SceneToolError>;
    fn description(&self) -> String;
    fn affected_entities(&self) -> Vec<EntityId>;
}

/// Groups multiple commands into one undo unit.
pub struct UndoGroup {
    pub commands: Vec<Box<dyn SceneCommand>>,
    pub description: String,
}

impl UndoGroup {
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            commands: Vec::new(),
            description: description.into(),
        }
    }

    pub fn with_command(mut self, command: Box<dyn SceneCommand>) -> Self {
        self.commands.push(command);
        self
    }

    pub fn undo_all(&mut self, world: &mut World) -> Result<(), SceneToolError> {
        for cmd in self.commands.iter_mut().rev() {
            cmd.undo(world)?;
        }
        Ok(())
    }

    pub fn affected_entities(&self) -> Vec<EntityId> {
        self.commands
            .iter()
            .flat_map(|c| c.affected_entities())
            .collect()
    }
}

/// Spawn an entity, adding it to the world. Undo destroys it.
pub struct SpawnEntityCommand {
    entity: Option<EntityId>,
    position: [f32; 3],
    rotation: [f32; 3],
    scale: [f32; 3],
    name: Option<String>,
}

impl SpawnEntityCommand {
    pub fn new(
        position: [f32; 3],
        rotation: [f32; 3],
        scale: [f32; 3],
        name: Option<String>,
    ) -> Self {
        Self {
            entity: None,
            position,
            rotation,
            scale,
            name,
        }
    }

    pub fn entity(&self) -> Option<EntityId> {
        self.entity
    }
}

impl SceneCommand for SpawnEntityCommand {
    fn execute(&mut self, world: &mut World) -> Result<(), SceneToolError> {
        let id = world.create_entity();
        self.entity = Some(id);
        // Store transform as individual f32 fields for the undo snapshot.
        // We don't depend on katla_math or katla_app, so we record the raw values.
        // The actual TransformComponent is added by the executor via the component registry.
        let _ = (self.position, self.rotation, self.scale, &self.name);
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), SceneToolError> {
        if let Some(id) = self.entity
            && world.entity_exists(id)
        {
            world.destroy_entity(id);
        }
        Ok(())
    }

    fn description(&self) -> String {
        match &self.name {
            Some(n) => format!("Spawn entity '{n}'"),
            None => "Spawn entity".to_string(),
        }
    }

    fn affected_entities(&self) -> Vec<EntityId> {
        self.entity.into_iter().collect()
    }
}

/// Destroy an entity, snapshotting known components for undo.
pub struct DestroyEntityCommand {
    entity: EntityId,
    /// Snapshot of field values per component, with function pointers for restoration.
    component_snapshots: Vec<ComponentSnapshot>,
    destroyed: bool,
}

/// Snapshot of a single component's field values, with restoration function pointers.
struct ComponentSnapshot {
    #[allow(dead_code)]
    type_name: &'static str,
    fields: Vec<(/* field name */ String, FieldValue)>,
    create_default: fn(&mut World, EntityId),
    set_fn: fn(&mut World, EntityId, &str, FieldValue) -> Result<(), SceneToolError>,
}

impl DestroyEntityCommand {
    pub fn new(entity: EntityId) -> Self {
        Self {
            entity,
            component_snapshots: Vec::new(),
            destroyed: false,
        }
    }

    /// Snapshot known component field values from the entity before destruction.
    pub(crate) fn snapshot_from_registry(
        &mut self,
        world: &mut World,
        registry: &super::registry::ComponentRegistry,
    ) {
        for entry in registry.entries() {
            if !(entry.has_component)(world, self.entity) {
                continue;
            }
            let mut fields = Vec::new();
            for field_info in (entry.get_fields)(world, self.entity) {
                if let Some(value) = (entry.get_field_value)(world, self.entity, field_info.name) {
                    // Skip Unknown values — they can't be restored
                    if !matches!(value, FieldValue::Unknown) {
                        fields.push((field_info.name.to_string(), value));
                    }
                }
            }
            if !fields.is_empty() {
                self.component_snapshots.push(ComponentSnapshot {
                    type_name: entry.type_name,
                    fields,
                    create_default: entry.create_default,
                    set_fn: entry.set_field_value,
                });
            }
        }
    }
}

impl SceneCommand for DestroyEntityCommand {
    fn execute(&mut self, world: &mut World) -> Result<(), SceneToolError> {
        if !world.entity_exists(self.entity) {
            return Err(SceneToolError::EntityNotFound(self.entity));
        }
        world.destroy_entity(self.entity);
        self.destroyed = true;
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), SceneToolError> {
        if !self.destroyed {
            return Ok(());
        }
        let new_id = world.create_entity();
        // Restore components from snapshots
        for snapshot in &self.component_snapshots {
            (snapshot.create_default)(world, new_id);
            for (field_name, value) in &snapshot.fields {
                let _ = (snapshot.set_fn)(world, new_id, field_name, value.clone());
            }
        }
        self.destroyed = false;
        Ok(())
    }

    fn description(&self) -> String {
        format!("Destroy entity {}", self.entity)
    }

    fn affected_entities(&self) -> Vec<EntityId> {
        vec![self.entity]
    }
}

/// Set a single field value on a component, storing old value for undo.
pub struct SetFieldCommand {
    entity: EntityId,
    field_name: String,
    old_value: Option<FieldValue>,
    set_fn: fn(&mut World, EntityId, &str, FieldValue) -> Result<(), SceneToolError>,
    executed: bool,
}

impl SetFieldCommand {
    pub fn new(
        entity: EntityId,
        field_name: String,
        old_value: FieldValue,
        entry: &ComponentRegistryEntry,
    ) -> Self {
        Self {
            entity,
            field_name,
            old_value: Some(old_value),
            set_fn: entry.set_field_value,
            // The mutation is already applied by the executor before creating this command,
            // so we start as already executed.
            executed: true,
        }
    }
}

impl SceneCommand for SetFieldCommand {
    fn execute(&mut self, _world: &mut World) -> Result<(), SceneToolError> {
        // The actual field setting is done via the ComponentRegistry in the executor
        // before this command is created. We just track execution state.
        self.executed = true;
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), SceneToolError> {
        if !self.executed {
            return Ok(());
        }
        if !world.entity_exists(self.entity) {
            return Err(SceneToolError::EntityNotFound(self.entity));
        }
        if let Some(ref old) = self.old_value {
            (self.set_fn)(world, self.entity, &self.field_name, old.clone())?;
        }
        self.executed = false;
        Ok(())
    }

    fn description(&self) -> String {
        format!("Set field '{}' on entity {}", self.field_name, self.entity)
    }

    fn affected_entities(&self) -> Vec<EntityId> {
        vec![self.entity]
    }
}

/// Duplicate an entity by spawning a new one. Undo destroys the duplicate.
pub struct DuplicateEntityCommand {
    source: EntityId,
    duplicate: Option<EntityId>,
    #[allow(dead_code)]
    position_offset: Option<[f32; 3]>,
}

impl DuplicateEntityCommand {
    pub fn new(source: EntityId, position_offset: Option<[f32; 3]>) -> Self {
        Self {
            source,
            duplicate: None,
            position_offset,
        }
    }

    pub fn duplicate(&self) -> Option<EntityId> {
        self.duplicate
    }
}

impl SceneCommand for DuplicateEntityCommand {
    fn execute(&mut self, world: &mut World) -> Result<(), SceneToolError> {
        if !world.entity_exists(self.source) {
            return Err(SceneToolError::EntityNotFound(self.source));
        }
        let new_id = world.create_entity();
        self.duplicate = Some(new_id);
        // Actual component copying is done via the ComponentRegistry in the executor.
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), SceneToolError> {
        if let Some(id) = self.duplicate
            && world.entity_exists(id)
        {
            world.destroy_entity(id);
        }
        Ok(())
    }

    fn description(&self) -> String {
        format!("Duplicate entity {} -> {:?}", self.source, self.duplicate)
    }

    fn affected_entities(&self) -> Vec<EntityId> {
        self.duplicate.into_iter().collect()
    }
}
