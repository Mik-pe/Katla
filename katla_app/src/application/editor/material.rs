//! Validated PBR edits and reversible material commands for every authoring surface.

use katla_agent::material::{MaterialOp, MaterialPreset, MaterialValues};
use katla_ecs::scene_tool::{SceneCommand, SceneToolError, UndoGroup};
use katla_ecs::{EntityId, World};
use katla_math::Color;
use serde_json::{Value, json};

use super::Application;
use crate::components::{DrawableComponent, EditorHidden};

/// Read authoring values in sRGB space, leaving runtime GPU handles opaque.
pub(crate) fn values(drawable: &DrawableComponent) -> MaterialValues {
    let c = drawable.color.unwrap_or(Color::WHITE).to_srgb();
    MaterialValues {
        base_color: [c.r, c.g, c.b, c.a],
        metallic: drawable.metallic,
        roughness: drawable.roughness,
        ao: drawable.ao,
    }
}

#[derive(Clone, Copy)]
struct Snapshot {
    color: Option<Color>,
    metallic: f32,
    roughness: f32,
    ao: f32,
}

impl Snapshot {
    fn read(d: &DrawableComponent) -> Self {
        Self {
            color: d.color,
            metallic: d.metallic,
            roughness: d.roughness,
            ao: d.ao,
        }
    }
    fn apply(self, d: &mut DrawableComponent) {
        d.color = self.color;
        d.metallic = self.metallic;
        d.roughness = self.roughness;
        d.ao = self.ao;
    }
}

pub(crate) struct MaterialCommand {
    edits: Vec<(EntityId, Snapshot, Snapshot)>,
}

impl MaterialCommand {
    fn apply(&self, world: &mut World, redo: bool) -> Result<(), SceneToolError> {
        for (entity, _, _) in &self.edits {
            if !world.entity_exists(*entity) {
                return Err(SceneToolError::EntityNotFound(*entity));
            }
            if world.get_component::<DrawableComponent>(*entity).is_none() {
                return Err(SceneToolError::ComponentNotFound {
                    entity: *entity,
                    component: "DrawableComponent".into(),
                });
            }
        }
        for (entity, before, after) in &self.edits {
            if let Some(d) = world.get_component_mut::<DrawableComponent>(*entity) {
                (if redo { *after } else { *before }).apply(d);
            }
        }
        Ok(())
    }
}

impl SceneCommand for MaterialCommand {
    fn execute(&mut self, world: &mut World) -> Result<(), SceneToolError> {
        self.apply(world, true)
    }
    fn undo(&mut self, world: &mut World) -> Result<(), SceneToolError> {
        self.apply(world, false)
    }
    fn description(&self) -> String {
        "Edit surface material".into()
    }
    fn affected_entities(&self) -> Vec<EntityId> {
        self.edits.iter().map(|e| e.0).collect()
    }
}

fn entity(world: &World, id: &str) -> Result<EntityId, String> {
    let raw = id
        .parse::<u64>()
        .map_err(|_| "Expected a decimal generational entity_id string")?;
    let entity = EntityId::from_raw(raw);
    if !world.entity_exists(entity) {
        return Err(format!("Entity {id} does not exist"));
    }
    if world.get_component::<EditorHidden>(entity).is_some() {
        return Err("Editor-owned entities cannot be edited".into());
    }
    Ok(entity)
}

fn drawable(world: &World, id: EntityId) -> Result<&DrawableComponent, String> {
    world
        .get_component::<DrawableComponent>(id)
        .ok_or_else(|| format!("Entity {id} has no rendered material; choose a mesh object"))
}

/// Apply a material operation with complete preflight and the existing editor history.
pub(super) fn execute(app: &mut Application, op: MaterialOp, agent: bool) -> Result<Value, String> {
    if matches!(op, MaterialOp::Set { .. })
        && app.play_mode != crate::application::game_state::PlayMode::Editing
    {
        return Err("Stop play mode before editing a material".into());
    }
    finish_drag(app);
    let (result, command) = apply(&mut app.world, op)?;
    if let Some(command) = command {
        let mut undo = UndoGroup::new("Edit surface material");
        undo.commands.push(Box::new(command));
        if agent {
            app.editor.agent_undo_stack.push(undo);
            app.editor.agent_redo_stack.clear();
        } else {
            app.editor.push_undo(undo);
        }
    }
    Ok(result)
}

pub(in crate::application) fn edit_live(
    app: &mut Application,
    op: MaterialOp,
) -> Result<(), String> {
    if app.play_mode != crate::application::game_state::PlayMode::Editing {
        return Err("Stop play mode before editing a material".into());
    }
    let (_, command) = apply(&mut app.world, op)?;
    if let Some(command) = command {
        let same_target = app
            .editor
            .material_drag
            .as_ref()
            .is_some_and(|old| old.affected_entities() == command.affected_entities());
        if !same_target {
            finish_drag(app);
        }
        if let Some(old) = &mut app.editor.material_drag {
            for (edit, latest) in old.edits.iter_mut().zip(command.edits) {
                edit.2 = latest.2;
            }
        } else {
            app.editor.material_drag = Some(command);
        }
    }
    Ok(())
}

pub(in crate::application) fn finish_drag(app: &mut Application) {
    if let Some(command) = app.editor.material_drag.take() {
        let mut undo = UndoGroup::new("Edit surface material");
        undo.commands.push(Box::new(command));
        app.editor.push_undo(undo);
    }
}

fn apply(world: &mut World, op: MaterialOp) -> Result<(Value, Option<MaterialCommand>), String> {
    match op {
        MaterialOp::Presets => Ok((
            json!({"presets":MaterialPreset::ALL.map(|p| json!({"preset":p,"label":p.label(),"values":p.values()})), "color_space":"srgb", "scope":"Per-object multipliers; textures and mesh geometry are preserved."}),
            None,
        )),
        MaterialOp::Inspect { entity_id } => {
            let id = entity(world, &entity_id)?;
            Ok((
                json!({"entity_id":entity_id,"values":values(drawable(world,id)?),"color_space":"srgb"}),
                None,
            ))
        }
        MaterialOp::Set {
            entity_ids,
            preset,
            base_color,
            metallic,
            roughness,
            ao,
        } => {
            if entity_ids.is_empty() || entity_ids.len() > 256 {
                return Err("Choose between 1 and 256 entity_ids".into());
            }
            if preset.is_none()
                && base_color.is_none()
                && metallic.is_none()
                && roughness.is_none()
                && ao.is_none()
            {
                return Err("Supply a preset or at least one material factor".into());
            }
            let mut edits = Vec::new();
            for id in &entity_ids {
                let id = entity(world, id)?;
                if edits.iter().any(|(existing, _, _)| *existing == id) {
                    return Err("entity_ids must not contain duplicates".into());
                }
                let d = drawable(world, id)?;
                let mut v = preset
                    .map(MaterialPreset::values)
                    .unwrap_or_else(|| values(d));
                if let Some(c) = base_color {
                    v.base_color = c;
                }
                if let Some(m) = metallic {
                    v.metallic = m;
                }
                if let Some(r) = roughness {
                    v.roughness = r;
                }
                if let Some(a) = ao {
                    v.ao = a;
                }
                v.validate()?;
                let c = v.base_color;
                let before = Snapshot::read(d);
                // Preserve an absent tint and exact linear channels when the patch only edits PBR factors.
                let color = if preset.is_some() || base_color.is_some() {
                    Some(Color::new(c[0], c[1], c[2], c[3]).to_linear())
                } else {
                    before.color
                };
                edits.push((
                    id,
                    before,
                    Snapshot {
                        color,
                        metallic: v.metallic,
                        roughness: v.roughness,
                        ao: v.ao,
                    },
                ));
            }
            let mut command = MaterialCommand { edits };
            command.execute(world).map_err(|e| e.to_string())?;
            let results: Vec<_> = command.edits.iter().map(|(id,_,_)| Ok(json!({"entity_id":id.id().to_string(),"values":values(drawable(world,*id)?)}))).collect::<Result<_,String>>()?;
            Ok((
                json!({"materials":results,"color_space":"srgb"}),
                Some(command),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use katla_gfx::{MaterialHandle, MeshHandle};
    fn set(ids: Vec<String>, roughness: f32) -> MaterialOp {
        MaterialOp::Set {
            entity_ids: ids,
            preset: None,
            base_color: None,
            metallic: None,
            roughness: Some(roughness),
            ao: None,
        }
    }
    #[test]
    fn test_batch_preflight_and_material_undo_redo() {
        let mut world = World::new();
        let a = world.spawn((DrawableComponent::with_handles(
            MeshHandle::NONE,
            MaterialHandle::NONE,
        ),));
        let b = world.create_entity();
        assert!(
            apply(
                &mut world,
                set(vec![a.id().to_string(), b.id().to_string()], 0.2)
            )
            .is_err()
        );
        assert_eq!(drawable(&world, a).unwrap().roughness, 0.5);
        assert!(apply(&mut world, set(vec![a.id().to_string()], -0.1)).is_err());
        let (_, undo) = apply(&mut world, set(vec![a.id().to_string()], 0.2)).unwrap();
        assert_eq!(drawable(&world, a).unwrap().roughness, 0.2);
        let mut undo = undo.unwrap();
        undo.undo(&mut world).unwrap();
        assert_eq!(drawable(&world, a).unwrap().roughness, 0.5);
        assert!(drawable(&world, a).unwrap().color.is_none());
        undo.execute(&mut world).unwrap();
        assert_eq!(drawable(&world, a).unwrap().roughness, 0.2);
    }
    #[test]
    fn test_srgb_preset_and_protected_entities() {
        let mut world = World::new();
        let a = world.spawn((DrawableComponent::with_handles(
            MeshHandle::NONE,
            MaterialHandle::NONE,
        ),));
        apply(
            &mut world,
            MaterialOp::Set {
                entity_ids: vec![a.id().to_string()],
                preset: Some(MaterialPreset::Oak),
                base_color: None,
                metallic: None,
                roughness: None,
                ao: None,
            },
        )
        .unwrap();
        let actual = values(drawable(&world, a).unwrap());
        assert!(
            (actual.base_color[0] - MaterialPreset::Oak.values().base_color[0]).abs() < 0.00001
        );
        world.add_component(a, EditorHidden);
        assert!(apply(&mut world, set(vec![a.id().to_string()], 0.3)).is_err());
    }
}
