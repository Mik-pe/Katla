//! Validated, undoable script and particle attachments without native handles.
use crate::application::Application;
use crate::components::{EditorHidden, ParticleEmitterComponent, TransformComponent};
use crate::scene::{
    AssetRef, EntityDescriptor, EntitySource, ParticleEmitterDescriptor, Scene, SceneEntityId,
};
use katla_agent::behavior::BehaviorOp;
use katla_ecs::scene_tool::{SceneCommand, SceneToolError, UndoGroup};
use katla_ecs::{EntityId, World};
use serde_json::{Value, json};

#[derive(Clone)]
enum Attachment {
    Script(Option<String>),
    Particles(Option<ParticleEmitterDescriptor>),
}
struct AttachmentCommand {
    entity: EntityId,
    before: Attachment,
    after: Attachment,
}
impl AttachmentCommand {
    fn apply(&self, world: &mut World, attachment: &Attachment) -> Result<(), SceneToolError> {
        if !world.entity_exists(self.entity) {
            return Err(SceneToolError::EntityNotFound(self.entity));
        }
        match attachment {
            Attachment::Script(Some(path)) => {
                world.add_component(self.entity, katla_script::ScriptComponent::new(path));
            }
            Attachment::Script(None) => {
                world.remove_component::<katla_script::ScriptComponent>(self.entity);
            }
            Attachment::Particles(Some(descriptor)) => {
                let handle = world
                    .get_component::<ParticleEmitterComponent>(self.entity)
                    .and_then(|e| e.emitter_handle);
                let position = crate::systems::resolve_world_transforms(world)
                    .get(&self.entity)
                    .map(|p| p.transform.position.to_array())
                    .unwrap_or([0.0; 3]);
                let mut component = descriptor.to_component(position);
                component.emitter_handle = handle;
                world.add_component(self.entity, component);
            }
            Attachment::Particles(None) => {
                world.remove_component::<ParticleEmitterComponent>(self.entity);
            }
        }
        Ok(())
    }
}
impl SceneCommand for AttachmentCommand {
    fn execute(&mut self, world: &mut World) -> Result<(), SceneToolError> {
        self.apply(world, &self.after)
    }
    fn undo(&mut self, world: &mut World) -> Result<(), SceneToolError> {
        self.apply(world, &self.before)
    }
    fn description(&self) -> String {
        "Edit entity behavior".into()
    }
    fn affected_entities(&self) -> Vec<EntityId> {
        vec![self.entity]
    }
}
fn entity(app: &Application, id: &str) -> Result<EntityId, String> {
    let entity = EntityId::from_raw(
        id.parse()
            .map_err(|_| "Expected a full decimal entity_id string")?,
    );
    if !app.world.entity_exists(entity) || app.world.get_component::<EditorHidden>(entity).is_some()
    {
        return Err("Entity is stale, missing or editor-owned".into());
    }
    if app
        .world
        .get_component::<TransformComponent>(entity)
        .is_none()
    {
        return Err("Behavior requires an authored transform".into());
    }
    Ok(entity)
}
fn edit(app: &mut Application, entity: EntityId, after: Attachment) -> Result<(), String> {
    if app.play_mode != super::super::game_state::PlayMode::Editing {
        return Err("Stop simulation before authoring attachments".into());
    }
    let before = match &after {
        Attachment::Script(_) => Attachment::Script(
            app.world
                .get_component::<katla_script::ScriptComponent>(entity)
                .map(|c| c.script_path.clone()),
        ),
        Attachment::Particles(_) => Attachment::Particles(
            app.world
                .get_component::<ParticleEmitterComponent>(entity)
                .map(ParticleEmitterDescriptor::from_component),
        ),
    };
    let mut command = AttachmentCommand {
        entity,
        before,
        after,
    };
    command.execute(&mut app.world).map_err(|e| e.to_string())?;
    let mut group = UndoGroup::new("Edit entity behavior");
    group.commands.push(Box::new(command));
    app.editor.agent_undo_stack.push(group);
    app.editor.agent_redo_stack.clear();
    Ok(())
}

pub(super) fn execute(app: &mut Application, op: BehaviorOp) -> Result<Value, String> {
    if matches!(op, BehaviorOp::Describe) {
        return Ok(json!({
            "particle_example":ParticleEmitterDescriptor { emit_rate:0.0, base_lifetime:0.8, base_scale:0.08, kill_on_destroy:true, ..Default::default() },
            "script_path":"scripts/prefab-effect.luau",
            "script_example":include_str!("../../../../resources/scripts/prefab-effect.luau"),
            "workflow":"Use prefab instantiate nodes to choose a child. set_script validates a resource-relative Luau file before attaching; set_particles accepts the complete descriptor shown here. Use trigger burst_particles/set_particles_active or script world:burst_particles/world:set_particles_active. simulation play verifies gameplay; stop restores authored state, so query fresh IDs before capture/save.",
            "particle_color_space":"linear RGBA, as in scene particle descriptors",
            "detach":"path/document null removes an attachment; editor_view undo restores it",
            "burst_limits":{"count":100000,"queued_bursts":1024}
        }));
    }
    let id = match &op {
        BehaviorOp::Inspect { entity_id }
        | BehaviorOp::SetScript { entity_id, .. }
        | BehaviorOp::SetParticles { entity_id, .. }
        | BehaviorOp::Burst { entity_id, .. }
        | BehaviorOp::SetActive { entity_id, .. } => entity(app, entity_id)?,
        BehaviorOp::Describe => return Err("Missing behavior operation".into()),
    };
    match op {
        BehaviorOp::SetScript { path, .. } => {
            let path = path
                .map(|path| -> Result<String, String> {
                    AssetRef::Resource(path.clone()).validate()?;
                    let directory = app
                        .resources
                        .root
                        .join("scripts")
                        .canonicalize()
                        .map_err(|e| e.to_string())?;
                    let file = app
                        .resources
                        .root
                        .join(&path)
                        .canonicalize()
                        .map_err(|e| format!("Script {path}: {e}"))?;
                    if !file.starts_with(&directory)
                        || file.extension().and_then(|e| e.to_str()) != Some("luau")
                    {
                        return Err("Choose a .luau file below resources/scripts".into());
                    }
                    let mut engine =
                        katla_script::ScriptEngine::new().map_err(|e| e.to_string())?;
                    engine.set_scripts_dir(directory.to_string_lossy());
                    let name = file.to_str().ok_or("Script path must be UTF-8")?;
                    engine.load_script(name).map_err(|e| e.to_string())?;
                    Ok(name.into())
                })
                .transpose()?;
            edit(app, id, Attachment::Script(path))?;
        }
        BehaviorOp::SetParticles { document, .. } => {
            let descriptor = document
                .map(|document| -> Result<ParticleEmitterDescriptor, String> {
                    let descriptor: ParticleEmitterDescriptor =
                        serde_json::from_value(document).map_err(|e| e.to_string())?;
                    let mut scene = Scene::new("Validate particle attachment");
                    scene.next_entity_id = 2;
                    let mut entity = EntityDescriptor::new(SceneEntityId(1), EntitySource::Empty);
                    entity.particle_emitter = Some(descriptor.clone());
                    scene.entities.push(entity);
                    scene.validate().map_err(|e| e.to_string())?;
                    Ok(descriptor)
                })
                .transpose()?;
            edit(app, id, Attachment::Particles(descriptor))?;
        }
        BehaviorOp::Burst { count, .. } => {
            crate::particle_control::burst(&mut app.world, id, count)?
        }
        BehaviorOp::SetActive { active, .. } => {
            if app.play_mode == super::super::game_state::PlayMode::Editing {
                let mut descriptor = app
                    .world
                    .get_component::<ParticleEmitterComponent>(id)
                    .map(ParticleEmitterDescriptor::from_component)
                    .ok_or("Target has no particle emitter")?;
                descriptor.active = active;
                edit(app, id, Attachment::Particles(Some(descriptor)))?;
            } else {
                crate::particle_control::set_active(&mut app.world, id, active)?;
            }
        }
        BehaviorOp::Inspect { .. } | BehaviorOp::Describe => {}
    }
    let script = app
        .world
        .get_component::<katla_script::ScriptComponent>(id)
        .map(|s| {
            std::path::Path::new(&s.script_path)
                .strip_prefix(
                    app.resources
                        .root
                        .canonicalize()
                        .unwrap_or_else(|_| app.resources.root.clone()),
                )
                .ok()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|| s.script_path.clone())
        });
    Ok(json!({"entity_id":id.id().to_string(),"script":script,
        "particles":app.world.get_component::<ParticleEmitterComponent>(id).map(ParticleEmitterDescriptor::from_component),
        "world_position":crate::systems::resolve_world_transforms(&app.world).get(&id).map(|p| p.transform.position.to_array()),
        "editing":app.play_mode == super::super::game_state::PlayMode::Editing}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_particle_attachment_command_undo_and_redo_preserve_authored_values() {
        let mut world = World::new();
        let entity = world.spawn((TransformComponent::default(),));
        let mut command = AttachmentCommand {
            entity,
            before: Attachment::Particles(None),
            after: Attachment::Particles(Some(ParticleEmitterDescriptor {
                emit_rate: 17.0,
                color: [0.2, 0.3, 0.4, 1.0],
                ..Default::default()
            })),
        };
        command.execute(&mut world).unwrap();
        assert_eq!(
            world
                .get_component::<ParticleEmitterComponent>(entity)
                .unwrap()
                .config
                .emit_rate,
            17.0
        );
        command.undo(&mut world).unwrap();
        assert!(
            world
                .get_component::<ParticleEmitterComponent>(entity)
                .is_none()
        );
        command.execute(&mut world).unwrap();
        assert_eq!(
            world
                .get_component::<ParticleEmitterComponent>(entity)
                .unwrap()
                .config
                .color,
            [0.2, 0.3, 0.4, 1.0]
        );
        world.destroy_entity(entity);
        assert!(command.undo(&mut world).is_err());
    }
}
