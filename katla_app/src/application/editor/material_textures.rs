//! Atomic image assignment and image-only history for material authoring.

use crate::{
    components::DrawableComponent,
    material_images::{TextureBinding, TextureSource},
};
use katla_agent::material_sampling::TextureRole;
use katla_ecs::{
    EntityId, World,
    scene_tool::{SceneCommand, SceneToolError},
};
use serde_json::{Value, json};

pub(crate) struct TextureCommand {
    role: TextureRole,
    edits: Vec<(EntityId, Option<TextureBinding>, Option<TextureBinding>)>,
}
impl TextureCommand {
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
            if let Some(drawable) = world.get_component_mut::<DrawableComponent>(*id) {
                drawable.texture_bindings.0[self.role.index()] =
                    (if redo { after } else { before }).clone();
            }
        }
        Ok(())
    }
}
impl SceneCommand for TextureCommand {
    fn execute(&mut self, world: &mut World) -> Result<(), SceneToolError> {
        self.apply(world, true)
    }
    fn undo(&mut self, world: &mut World) -> Result<(), SceneToolError> {
        self.apply(world, false)
    }
    fn description(&self) -> String {
        format!("Assign {} image", self.role.name())
    }
    fn affected_entities(&self) -> Vec<EntityId> {
        self.edits.iter().map(|row| row.0).collect()
    }
}

pub(super) fn apply(
    app: &mut super::Application,
    ids: Vec<String>,
    role: TextureRole,
    source: Value,
) -> Result<(Value, TextureCommand), String> {
    if ids.is_empty() || ids.len() > 256 {
        return Err("Choose between 1 and 256 entity_ids".into());
    }
    let source: TextureSource =
        serde_json::from_value(source).map_err(|error| error.to_string())?;
    let mut targets = Vec::with_capacity(ids.len());
    for id in ids {
        let id = super::material::entity(&app.world, &id)?;
        if targets.contains(&id) {
            return Err("entity_ids must not contain duplicates".into());
        }
        let drawable = super::material::drawable(&app.world, id)?;
        drawable.sampling.validate().map_err(str::to_owned)?;
        let needs_uv = match source {
            TextureSource::File { .. } | TextureSource::GltfImage { .. } => true,
            TextureSource::Inherit => drawable.texture_roles[role.index()],
            TextureSource::Neutral => false,
        };
        let set = drawable.sampling.roles()[role.index()].uv.tex_coord;
        if needs_uv && !drawable.uv_sets[set as usize] {
            return Err(format!(
                "Entity {id} {} image requires missing TEXCOORD_{set}",
                role.name()
            ));
        }
        targets.push(id);
    }
    let context = crate::scene::SceneAssetContext::new(
        &app.resources.root,
        app.scene_document.path.as_deref(),
    )
    .map_err(|error| error.to_string())?;
    let binding = app.prepare_material_image(source.clone(), role, &context)?;
    let mut edits = Vec::with_capacity(targets.len());
    let mut receipts = Vec::with_capacity(targets.len());
    for id in targets {
        let before = super::material::drawable(&app.world, id)?
            .texture_bindings
            .0[role.index()]
        .clone();
        receipts.push(json!({"entity_id":id.id().to_string(),"before":inspect_binding(before.as_ref()),"texture":inspect_binding(binding.as_ref())}));
        edits.push((id, before, binding.clone()));
    }
    let mut command = TextureCommand { role, edits };
    command
        .execute(&mut app.world)
        .map_err(|error| error.to_string())?;
    Ok((
        json!({"role":role,"requested_source":source,"materials":receipts,"batch_atomic":true,"sampling_preserved":true,"factors_preserved":true}),
        command,
    ))
}

pub(super) fn inspect_binding(binding: Option<&TextureBinding>) -> Value {
    binding.map_or_else(||json!({"source":{"kind":"inherit"}}),|binding|json!({"source":binding.source,"image":binding.metadata,"sampled_color_space":"linear"}))
}
