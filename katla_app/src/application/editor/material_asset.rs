//! Portable surface capture and atomic full-material history.

use super::{
    Application,
    material::{self, Snapshot},
};
use crate::{
    components::DrawableComponent,
    material_asset::MaterialAsset,
    material_images::{TextureBindings, TextureSource},
    rendering::MaterialSampling,
    scene::SceneAssetContext,
};
use katla_agent::{material_asset::MaterialAssetOp, material_sampling::TextureRole};
use katla_ecs::{
    EntityId, World,
    scene_tool::{SceneCommand, SceneToolError, UndoGroup},
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[derive(Clone)]
struct State {
    factors: Snapshot,
    sampling: MaterialSampling,
    textures: TextureBindings,
}
impl State {
    fn read(d: &DrawableComponent) -> Self {
        Self {
            factors: Snapshot::read(d),
            sampling: d.sampling,
            textures: d.texture_bindings.clone(),
        }
    }
    fn apply(&self, d: &mut DrawableComponent) {
        self.factors.apply(d);
        d.sampling = self.sampling;
        d.texture_bindings = self.textures.clone();
    }
}
struct AssetCommand {
    name: String,
    edits: Vec<(EntityId, State, State)>,
}
impl AssetCommand {
    fn apply(&self, world: &mut World, redo: bool) -> Result<(), SceneToolError> {
        for (id, _, _) in &self.edits {
            if !world.entity_exists(*id) {
                return Err(SceneToolError::EntityNotFound(*id));
            }
            if world.get_component::<DrawableComponent>(*id).is_none() {
                return Err(SceneToolError::ComponentNotFound {
                    entity: *id,
                    component: "DrawableComponent".into(),
                });
            }
        }
        for (id, before, after) in &self.edits {
            if let Some(d) = world.get_component_mut::<DrawableComponent>(*id) {
                (if redo { after } else { before }).apply(d);
            }
        }
        Ok(())
    }
}
impl SceneCommand for AssetCommand {
    fn execute(&mut self, world: &mut World) -> Result<(), SceneToolError> {
        self.apply(world, true)
    }
    fn undo(&mut self, world: &mut World) -> Result<(), SceneToolError> {
        self.apply(world, false)
    }
    fn description(&self) -> String {
        format!("Apply material {}", self.name)
    }
    fn affected_entities(&self) -> Vec<EntityId> {
        self.edits.iter().map(|row| row.0).collect()
    }
}

pub(crate) fn execute(
    app: &mut Application,
    op: MaterialAssetOp,
    agent: bool,
) -> Result<Value, String> {
    if matches!(
        op,
        MaterialAssetOp::Apply { .. } | MaterialAssetOp::Capture { .. }
    ) && app.play_mode != crate::application::game_state::PlayMode::Editing
    {
        return Err("Stop play mode before applying or capturing a material".into());
    }
    material::finish_drag(app);
    let result = run(app, op, agent);
    app.drain_material_images();
    result
}
fn run(app: &mut Application, op: MaterialAssetOp, agent: bool) -> Result<Value, String> {
    match op {
        MaterialAssetOp::Describe => Ok(
            json!({"example":MaterialAsset::default(),"format":".katmat RON, version 1","paths":"Project-relative tool paths. Image Resource roots use project resources; Scene roots use the material file's directory; File is intentionally absolute.","workflow":"describe/read or capture; edit JSON; validate/write; apply to mesh entity_ids; inspect and editor_view; undo/redo and save_scene. Writes leave live copies unchanged; reapply reads revisions.","capture":"Resolves effective inherited images; neutral fallbacks become explicit neutral roles. All five image roles are explicit, so reuse preserves the captured surface.","apply":"Complete factors, sampling and images replace each target surface atomically as one undoable operation. Meshes, transforms and pipeline handles are preserved; applied objects remain independently editable.","units":{"base_color":"sRGB RGB and linear alpha","emission":"linear HDR RGB","uv_rotation":"radians"},"limits":{"targets":256,"file_bytes":crate::util::asset_io::MAX_ASSET_BYTES}}),
        ),
        MaterialAssetOp::Read { path } => {
            serde_json::to_value(MaterialAsset::load(&file(app, &path)?)?)
                .map_err(|error| error.to_string())
        }
        MaterialAssetOp::Validate { path, document } => {
            let asset = validate_document(app, &file(app, &path)?, document)?;
            Ok(json!({"path":path,"valid":true,"name":asset.name}))
        }
        MaterialAssetOp::Write { path, document } => {
            let destination = file(app, &path)?;
            let asset = validate_document(app, &destination, document)?;
            asset.save(&destination)?;
            Ok(json!({"path":path,"saved":true,"name":asset.name}))
        }
        MaterialAssetOp::Capture { path, entity_id } => {
            let destination = file(app, &path)?;
            let id = material::entity(&app.world, &entity_id)?;
            let mut asset = MaterialAsset {
                name: app
                    .world
                    .get_component::<crate::components::NameComponent>(id)
                    .map_or_else(|| "Material".into(), |name| name.name.clone()),
                values: material::values(material::drawable(&app.world, id)?),
                sampling: material::drawable(&app.world, id)?.sampling,
                ..Default::default()
            };
            let provenance = super::material_provenance::inspect(app, id)?;
            for (role, source) in TextureRole::ALL.into_iter().zip(asset.textures.roles_mut()) {
                let authored = &provenance["authored_textures"][role.name()]["source"];
                *source = if authored["kind"] == "inherit" {
                    let original = &provenance["imported_textures"][role.name()];
                    if original["using_fallback"] == false {
                        serde_json::from_value(original["source"].clone())
                            .map_err(|error| error.to_string())?
                    } else {
                        TextureSource::Neutral
                    }
                } else {
                    serde_json::from_value(authored.clone()).map_err(|error| error.to_string())?
                };
            }
            let old =
                SceneAssetContext::new(&app.resources.root, app.scene_document.path.as_deref())
                    .map_err(|error| error.to_string())?;
            let new = old
                .with_origin(&destination)
                .map_err(|error| error.to_string())?;
            for source in asset.textures.roles_mut() {
                if let Some(reference) = source.asset_mut() {
                    *reference = new.identify(&old.resolve(reference)?)?;
                }
            }
            validate_images(app, &asset, &destination)?;
            asset.save(&destination)?;
            Ok(json!({"path":path,"saved":true,"document":asset,"entity_id":entity_id}))
        }
        MaterialAssetOp::Apply { path, entity_ids } => {
            let origin = file(app, &path)?;
            let asset = MaterialAsset::load(&origin)?;
            let ids = targets(app, &entity_ids, &asset)?;
            let context = SceneAssetContext::new(&app.resources.root, Some(&origin))
                .map_err(|error| error.to_string())?;
            let bindings = app.prepare_material_assignments(
                ids[0],
                &asset.textures,
                &context,
                asset.sampling,
            )?;
            let after = State {
                factors: Snapshot::from_values(asset.values),
                sampling: asset.sampling,
                textures: bindings,
            };
            let edits = ids
                .iter()
                .map(|id| {
                    Ok((
                        *id,
                        State::read(material::drawable(&app.world, *id)?),
                        after.clone(),
                    ))
                })
                .collect::<Result<Vec<_>, String>>()?;
            let mut command = AssetCommand {
                name: asset.name.clone(),
                edits,
            };
            command
                .execute(&mut app.world)
                .map_err(|error| error.to_string())?;
            let mut undo = UndoGroup::new(command.description());
            undo.commands.push(Box::new(command));
            if agent {
                app.editor.agent_undo_stack.push(undo);
                app.editor.agent_redo_stack.clear();
            } else {
                app.editor.push_undo(undo);
            }
            Ok(
                json!({"path":path,"name":asset.name,"entity_ids":entity_ids,"batch_atomic":true,"independently_editable":true,"document":asset}),
            )
        }
    }
}
fn file(app: &Application, path: &str) -> Result<PathBuf, String> {
    let file = crate::util::asset_io::project_file(&app.resources.root, path)?;
    if file.extension().and_then(|value| value.to_str()) != Some("katmat") {
        return Err("Expected a project-relative .katmat path".into());
    }
    Ok(file)
}
fn validate_images(
    app: &mut Application,
    asset: &MaterialAsset,
    origin: &Path,
) -> Result<(), String> {
    asset.validate()?;
    let context = SceneAssetContext::new(&app.resources.root, Some(origin))
        .map_err(|error| error.to_string())?;
    for (role, source) in TextureRole::ALL.into_iter().zip(asset.textures.roles()) {
        app.validate_material_image(source, role, &context)
            .map_err(|error| format!("{} image: {error}", role.name()))?;
    }
    Ok(())
}
fn targets(
    app: &Application,
    ids: &[String],
    asset: &MaterialAsset,
) -> Result<Vec<EntityId>, String> {
    if ids.is_empty() || ids.len() > 256 {
        return Err("Choose between 1 and 256 entity_ids".into());
    }
    let mut targets = Vec::with_capacity(ids.len());
    for raw in ids {
        let id = material::entity(&app.world, raw)?;
        if targets.contains(&id) {
            return Err("entity_ids must not contain duplicates".into());
        }
        let d = material::drawable(&app.world, id)?;
        for ((role, source), sampling) in TextureRole::ALL
            .into_iter()
            .zip(asset.textures.roles())
            .zip(asset.sampling.roles())
        {
            if !matches!(source, TextureSource::Neutral)
                && !d.uv_sets[sampling.uv.tex_coord as usize]
            {
                return Err(format!(
                    "Entity {raw} {} image requires missing TEXCOORD_{}",
                    role.name(),
                    sampling.uv.tex_coord
                ));
            }
        }
        targets.push(id);
    }
    Ok(targets)
}

fn validate_document(
    app: &mut Application,
    origin: &Path,
    document: Value,
) -> Result<MaterialAsset, String> {
    let asset: MaterialAsset =
        serde_json::from_value(document).map_err(|error| error.to_string())?;
    validate_images(app, &asset, origin)?;
    Ok(asset)
}
