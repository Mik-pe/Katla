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
    Snapshot::read(drawable).values()
}

#[derive(Clone, Copy)]
struct Snapshot {
    color: Option<Color>,
    metallic: f32,
    roughness: f32,
    ao: f32,
    surface: crate::rendering::MaterialSurface,
}

impl Snapshot {
    fn values(self) -> MaterialValues {
        let c = self.color.unwrap_or(Color::WHITE).to_srgb();
        MaterialValues {
            base_color: [c.r, c.g, c.b, c.a],
            metallic: self.metallic,
            roughness: self.roughness,
            ao: self.ao,
            emissive_factor: self.surface.emissive_factor,
            normal_scale: self.surface.normal_scale,
            occlusion_strength: self.surface.occlusion_strength,
            alpha_mode: self.surface.alpha_mode,
            alpha_cutoff: self.surface.alpha_cutoff,
            double_sided: self.surface.double_sided,
        }
    }
    fn read(d: &DrawableComponent) -> Self {
        Self {
            color: d.color,
            metallic: d.metallic,
            roughness: d.roughness,
            ao: d.ao,
            surface: d.surface,
        }
    }
    fn apply(self, d: &mut DrawableComponent) {
        d.color = self.color;
        d.metallic = self.metallic;
        d.roughness = self.roughness;
        d.ao = self.ao;
        d.surface = self.surface;
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
            json!({"presets":MaterialPreset::ALL.map(|p| json!({"preset":p,"label":p.label(),"values":p.values()})), "base_color_space":"srgb","emissive_color_space":"linear", "scope":"Per-object multipliers; textures and mesh geometry are preserved.", "capabilities":capabilities()}),
            None,
        )),
        MaterialOp::Inspect { entity_id } => {
            let id = entity(world, &entity_id)?;
            Ok((
                json!({"entity_id":entity_id,"values":values(drawable(world,id)?),"base_color_space":"srgb","emissive_color_space":"linear","capabilities":capabilities()}),
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
            emissive_factor,
            normal_scale,
            occlusion_strength,
            alpha_mode,
            alpha_cutoff,
            double_sided,
        } => {
            if entity_ids.is_empty() || entity_ids.len() > 256 {
                return Err("Choose between 1 and 256 entity_ids".into());
            }
            if preset.is_none()
                && base_color.is_none()
                && metallic.is_none()
                && roughness.is_none()
                && ao.is_none()
                && emissive_factor.is_none()
                && normal_scale.is_none()
                && occlusion_strength.is_none()
                && alpha_mode.is_none()
                && alpha_cutoff.is_none()
                && double_sided.is_none()
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
                if let Some(emissive) = emissive_factor {
                    v.emissive_factor = emissive;
                }
                if let Some(scale) = normal_scale {
                    v.normal_scale = scale;
                }
                if let Some(strength) = occlusion_strength {
                    v.occlusion_strength = strength;
                }
                if let Some(mode) = alpha_mode {
                    v.alpha_mode = mode;
                }
                if let Some(cutoff) = alpha_cutoff {
                    v.alpha_cutoff = cutoff;
                }
                if let Some(two_sided) = double_sided {
                    v.double_sided = two_sided;
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
                        surface: crate::rendering::MaterialSurface {
                            emissive_factor: v.emissive_factor,
                            normal_scale: v.normal_scale,
                            occlusion_strength: v.occlusion_strength,
                            alpha_mode: v.alpha_mode,
                            alpha_cutoff: v.alpha_cutoff,
                            double_sided: v.double_sided,
                        },
                    },
                ));
            }
            let mut command = MaterialCommand { edits };
            command.execute(world).map_err(|e| e.to_string())?;
            let results: Vec<_> = command.edits.iter().map(|(id,before,_)| Ok(json!({"entity_id":id.id().to_string(),"before":before.values(),"values":values(drawable(world,*id)?)}))).collect::<Result<_,String>>()?;
            Ok((
                json!({"materials":results,"base_color_space":"srgb","emissive_color_space":"linear","capabilities":capabilities()}),
                Some(command),
            ))
        }
    }
}

fn capabilities() -> Value {
    json!({
        "base_color":"sRGB RGB and linear alpha texture multiplier",
        "alpha_changes_render_mode":false,
        "alpha_mode_editable":true,
        "alpha_modes":["opaque","mask","blend"],
        "alpha_cutoff_editable":true,
        "double_sided_editable":true,
        "blend_depth_policy":"sorted back-to-front, depth test enabled, scene depth writes disabled",
        "blend_shadow_policy":"blended surfaces do not cast binary shadow-map shadows",
        "blend_picking_policy":"nonzero-alpha surfaces are pickable; picking has its own depth buffer",
        "emission_editable":true,
        "emission_color_space":"linear RGB; HDR values allowed; missing emissive texture samples white",
        "normal_scale_editable":true,
        "occlusion_strength_editable":true,
        "textures_editable":false,
        "presets":"isotropic metallic/roughness factors; no texture or directional brushing",
        "maximum_batch_size":256,
        "batch_atomic":true
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use katla_gfx::{MaterialHandle, MeshHandle};
    #[test]
    fn test_coverage_patch_undo_and_invalid_threshold_are_atomic() {
        let mut world = World::new();
        let entity = world.spawn((DrawableComponent::with_handles(
            MeshHandle::NONE,
            MaterialHandle::NONE,
        ),));
        let request = |cutoff| {
            serde_json::from_value::<MaterialOp>(json!({"action":"set","entity_ids":[entity.id().to_string()],"alpha_mode":"mask","alpha_cutoff":cutoff,"double_sided":true})).unwrap()
        };
        let (_, mut command) = apply(&mut world, request(0.25)).unwrap();
        let surface = world
            .get_component::<DrawableComponent>(entity)
            .unwrap()
            .surface;
        assert_eq!(surface.alpha_mode, katla_agent::material::AlphaMode::Mask);
        assert!(surface.double_sided);
        assert!(apply(&mut world, request(-0.25)).is_err());
        assert_eq!(
            world
                .get_component::<DrawableComponent>(entity)
                .unwrap()
                .surface,
            surface
        );
        command.as_mut().unwrap().undo(&mut world).unwrap();
        assert_eq!(
            world
                .get_component::<DrawableComponent>(entity)
                .unwrap()
                .surface,
            Default::default()
        );
        command.as_mut().unwrap().execute(&mut world).unwrap();
        assert_eq!(
            world
                .get_component::<DrawableComponent>(entity)
                .unwrap()
                .surface,
            surface
        );
    }

    #[test]
    fn test_surface_patch_keeps_linear_hdr_values_and_validates_entire_batch() {
        let mut world = World::new();
        let entity = world.spawn((DrawableComponent::with_handles(
            MeshHandle::NONE,
            MaterialHandle::NONE,
        ),));
        let request = |emission| {
            serde_json::from_value::<MaterialOp>(json!({"action":"set", "entity_ids":[entity.id().to_string()], "emissive_factor":emission, "normal_scale":0.0, "occlusion_strength":0.25})).unwrap()
        };
        let (receipt, undo) = apply(&mut world, request([4.0, 0.2, 0.0])).unwrap();
        assert_eq!(receipt["emissive_color_space"], "linear");
        assert_eq!(receipt["base_color_space"], "srgb");
        let surface = drawable(&world, entity).unwrap().surface;
        assert_eq!(surface.emissive_factor, [4.0, 0.2, 0.0]);
        assert_eq!(surface.normal_scale, 0.0);
        assert_eq!(surface.occlusion_strength, 0.25);
        assert!(apply(&mut world, request([-1.0, 0.2, 0.0])).is_err());
        assert_eq!(drawable(&world, entity).unwrap().surface, surface);
        let invalid = serde_json::from_value::<MaterialOp>(json!({"action":"set", "entity_ids":[entity.id().to_string(), "0"], "emissive_factor":[2.0, 0.0, 0.0]})).unwrap();
        assert!(apply(&mut world, invalid).is_err());
        assert_eq!(drawable(&world, entity).unwrap().surface, surface);
        undo.unwrap().undo(&mut world).unwrap();
        assert_eq!(
            drawable(&world, entity).unwrap().surface,
            crate::rendering::MaterialSurface::default()
        );
        assert!(drawable(&world, entity).unwrap().color.is_none());
    }
    fn set(ids: Vec<String>, roughness: f32) -> MaterialOp {
        MaterialOp::Set {
            entity_ids: ids,
            preset: None,
            base_color: None,
            metallic: None,
            roughness: Some(roughness),
            ao: None,
            emissive_factor: None,
            normal_scale: None,
            occlusion_strength: None,
            alpha_mode: None,
            alpha_cutoff: None,
            double_sided: None,
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
        let (receipt, undo) = apply(&mut world, set(vec![a.id().to_string()], 0.2)).unwrap();
        assert_eq!(receipt["materials"][0]["before"]["roughness"], 0.5);
        assert!(
            (receipt["materials"][0]["values"]["roughness"]
                .as_f64()
                .unwrap()
                - 0.2)
                .abs()
                < 1e-6
        );
        assert_eq!(receipt["capabilities"]["alpha_changes_render_mode"], false);
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
                emissive_factor: None,
                normal_scale: None,
                occlusion_strength: None,
                alpha_mode: None,
                alpha_cutoff: None,
                double_sided: None,
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
