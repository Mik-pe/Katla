//! Atomic role-specific UV/sampler commands and agent-readable sampling values.

use crate::{
    components::DrawableComponent,
    rendering::{MaterialSampling, TextureSampling},
};
use katla_agent::material_sampling::{
    Magnification, Minification, SamplingPatch, TextureRole, TextureWrap,
};
use katla_ecs::scene_tool::{SceneCommand, SceneToolError};
use katla_ecs::{EntityId, World};
use katla_gfx::{AddressMode, FilterMode, MipFilter, SamplerDescriptor};
use serde_json::{Value, json};

pub(crate) struct SamplingCommand {
    edits: Vec<(EntityId, MaterialSampling, MaterialSampling)>,
}

impl SamplingCommand {
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
            if let Some(drawable) = world.get_component_mut::<DrawableComponent>(*entity) {
                drawable.sampling = if redo { *after } else { *before };
            }
        }
        Ok(())
    }
    pub(super) fn merge(&mut self, latest: Self) {
        for (edit, latest) in self.edits.iter_mut().zip(latest.edits) {
            edit.2 = latest.2;
        }
    }
}

impl SceneCommand for SamplingCommand {
    fn execute(&mut self, world: &mut World) -> Result<(), SceneToolError> {
        self.apply(world, true)
    }
    fn undo(&mut self, world: &mut World) -> Result<(), SceneToolError> {
        self.apply(world, false)
    }
    fn description(&self) -> String {
        "Edit material texture sampling".into()
    }
    fn affected_entities(&self) -> Vec<EntityId> {
        self.edits.iter().map(|edit| edit.0).collect()
    }
}

pub(super) fn apply(
    world: &mut World,
    ids: Vec<String>,
    role: TextureRole,
    patch: SamplingPatch,
) -> Result<(Value, SamplingCommand), String> {
    if ids.is_empty() || ids.len() > 256 {
        return Err("Choose between 1 and 256 entity_ids".into());
    }
    if patch.is_empty() {
        return Err("Supply at least one UV or sampler property in patch".into());
    }
    let mut edits = Vec::new();
    for id in &ids {
        let id = super::material::entity(world, id)?;
        if edits.iter().any(|(existing, _, _)| *existing == id) {
            return Err("entity_ids must not contain duplicates".into());
        }
        let drawable = super::material::drawable(world, id)?;
        let before = drawable.sampling;
        let mut after = before;
        let value = role_mut(&mut after, role);
        if let Some(set) = patch.tex_coord {
            if !drawable.uv_sets.get(set as usize).copied().unwrap_or(false) {
                return Err(format!(
                    "Entity {id} has no TEXCOORD_{set}; inspect uv_sets before selecting coordinates"
                ));
            }
            value.uv.tex_coord = set;
        }
        if let Some(offset) = patch.offset {
            value.uv.offset = offset;
        }
        if let Some(rotation) = patch.rotation {
            value.uv.rotation = rotation;
        }
        if let Some(scale) = patch.scale {
            value.uv.scale = scale;
        }
        if let Some(minification) = patch.minification {
            (value.sampler.min_filter, value.sampler.mip_filter) =
                minification_filters(minification);
        }
        if let Some(magnification) = patch.magnification {
            value.sampler.mag_filter = match magnification {
                Magnification::Nearest => FilterMode::Nearest,
                Magnification::Linear => FilterMode::Linear,
            };
        }
        if let Some(wrap) = patch.wrap_u {
            value.sampler.address_u = address(wrap);
        }
        if let Some(wrap) = patch.wrap_v {
            value.sampler.address_v = address(wrap);
        }
        if let Some(anisotropy) = patch.anisotropy {
            value.sampler.anisotropy = anisotropy;
        }
        drawable
            .validate_sampling(after)
            .map_err(|error| format!("Entity {id} {} sampling: {error}", role.name()))?;
        edits.push((id, before, after));
    }
    let mut command = SamplingCommand { edits };
    command.execute(world).map_err(|error| error.to_string())?;
    let result = command.edits.iter().map(|(id,before,after)| json!({"entity_id":id.id().to_string(),"before":inspect_role(before.roles()[role.index()]),"sampling":inspect_role(after.roles()[role.index()])})).collect::<Vec<_>>();
    Ok((
        json!({"role":role,"materials":result,"rotation_unit":"radians","image_bindings_preserved":true,"batch_atomic":true}),
        command,
    ))
}

fn role_mut(sampling: &mut MaterialSampling, role: TextureRole) -> &mut TextureSampling {
    match role {
        TextureRole::Albedo => &mut sampling.albedo,
        TextureRole::Normal => &mut sampling.normal,
        TextureRole::MetallicRoughness => &mut sampling.metallic_roughness,
        TextureRole::Occlusion => &mut sampling.occlusion,
        TextureRole::Emission => &mut sampling.emission,
    }
}

fn minification_filters(filter: Minification) -> (FilterMode, MipFilter) {
    match filter {
        Minification::Nearest => (FilterMode::Nearest, MipFilter::None),
        Minification::Linear => (FilterMode::Linear, MipFilter::None),
        Minification::NearestMipmapNearest => (FilterMode::Nearest, MipFilter::Nearest),
        Minification::LinearMipmapNearest => (FilterMode::Linear, MipFilter::Nearest),
        Minification::NearestMipmapLinear => (FilterMode::Nearest, MipFilter::Linear),
        Minification::LinearMipmapLinear => (FilterMode::Linear, MipFilter::Linear),
    }
}
fn address(wrap: TextureWrap) -> AddressMode {
    match wrap {
        TextureWrap::Repeat => AddressMode::Repeat,
        TextureWrap::ClampToEdge => AddressMode::ClampToEdge,
        TextureWrap::MirroredRepeat => AddressMode::MirroredRepeat,
    }
}
fn wrap(address: AddressMode) -> TextureWrap {
    match address {
        AddressMode::Repeat => TextureWrap::Repeat,
        AddressMode::ClampToEdge => TextureWrap::ClampToEdge,
        AddressMode::MirroredRepeat => TextureWrap::MirroredRepeat,
    }
}
fn minification(sampler: SamplerDescriptor) -> Minification {
    match (sampler.min_filter, sampler.mip_filter) {
        (FilterMode::Nearest, MipFilter::None) => Minification::Nearest,
        (FilterMode::Linear, MipFilter::None) => Minification::Linear,
        (FilterMode::Nearest, MipFilter::Nearest) => Minification::NearestMipmapNearest,
        (FilterMode::Linear, MipFilter::Nearest) => Minification::LinearMipmapNearest,
        (FilterMode::Nearest, MipFilter::Linear) => Minification::NearestMipmapLinear,
        (FilterMode::Linear, MipFilter::Linear) => Minification::LinearMipmapLinear,
    }
}

pub(super) fn inspect_role(value: TextureSampling) -> Value {
    json!({"uv":value.uv,"sampler":{"minification":minification(value.sampler),"magnification":if value.sampler.mag_filter==FilterMode::Nearest { Magnification::Nearest } else { Magnification::Linear },"wrap_u":wrap(value.sampler.address_u),"wrap_v":wrap(value.sampler.address_v),"anisotropy":value.sampler.anisotropy}})
}

pub(super) fn inspect(drawable: &DrawableComponent) -> Value {
    let fields = TextureRole::ALL
        .into_iter()
        .zip(drawable.sampling.roles())
        .map(|(role, value)| (role.name().to_owned(), inspect_role(value)));
    Value::Object(fields.collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use katla_gfx::{MaterialHandle, MeshHandle};

    fn spawn(world: &mut World, uv_sets: [bool; 2]) -> EntityId {
        let mut drawable = DrawableComponent::with_handles(MeshHandle::NONE, MaterialHandle::NONE);
        drawable.uv_sets = uv_sets;
        world.spawn((drawable,))
    }

    #[test]
    fn test_sampling_patch_is_atomic_preserves_other_roles_and_undo_scope() {
        let mut world = World::new();
        let first = spawn(&mut world, [true; 2]);
        let second = spawn(&mut world, [true, false]);
        let patch = SamplingPatch {
            tex_coord: Some(1),
            rotation: Some(0.5),
            wrap_u: Some(TextureWrap::MirroredRepeat),
            ..Default::default()
        };
        let before = world
            .get_component::<DrawableComponent>(first)
            .unwrap()
            .sampling;
        assert!(
            apply(
                &mut world,
                vec![first.id().to_string(), second.id().to_string()],
                TextureRole::Normal,
                patch
            )
            .err()
            .unwrap()
            .contains("TEXCOORD_1")
        );
        assert_eq!(
            world
                .get_component::<DrawableComponent>(first)
                .unwrap()
                .sampling,
            before
        );
        let (receipt, mut command) = apply(
            &mut world,
            vec![first.id().to_string()],
            TextureRole::Normal,
            patch,
        )
        .unwrap();
        assert_eq!(receipt["role"], "normal");
        assert_eq!(
            receipt["materials"][0]["sampling"]["sampler"]["wrap_u"],
            "mirrored_repeat"
        );
        let after = world
            .get_component::<DrawableComponent>(first)
            .unwrap()
            .sampling;
        assert_eq!(after.albedo, before.albedo);
        assert_eq!(after.emission, before.emission);
        world
            .get_component_mut::<DrawableComponent>(first)
            .unwrap()
            .roughness = 0.9;
        command.undo(&mut world).unwrap();
        assert_eq!(
            world
                .get_component::<DrawableComponent>(first)
                .unwrap()
                .sampling,
            before
        );
        assert_eq!(
            world
                .get_component::<DrawableComponent>(first)
                .unwrap()
                .roughness,
            0.9
        );
        command.execute(&mut world).unwrap();
        assert_eq!(
            world
                .get_component::<DrawableComponent>(first)
                .unwrap()
                .sampling,
            after
        );
    }

    #[test]
    fn test_invalid_sampling_rejects_before_mutation_and_inspection_uses_authoring_units() {
        let mut world = World::new();
        let id = spawn(&mut world, [true; 2]);
        let ids = vec![id.id().to_string()];
        let before = world
            .get_component::<DrawableComponent>(id)
            .unwrap()
            .sampling;
        for patch in [
            SamplingPatch::default(),
            SamplingPatch {
                rotation: Some(f32::INFINITY),
                ..Default::default()
            },
            SamplingPatch {
                anisotropy: Some(17),
                ..Default::default()
            },
            SamplingPatch {
                anisotropy: Some(4),
                minification: Some(Minification::Nearest),
                ..Default::default()
            },
        ] {
            assert!(apply(&mut world, ids.clone(), TextureRole::Albedo, patch).is_err());
            assert_eq!(
                world
                    .get_component::<DrawableComponent>(id)
                    .unwrap()
                    .sampling,
                before
            );
        }
        let patch = SamplingPatch {
            minification: Some(Minification::NearestMipmapLinear),
            magnification: Some(Magnification::Nearest),
            scale: Some([-1., 0.]),
            ..Default::default()
        };
        apply(&mut world, ids, TextureRole::Albedo, patch).unwrap();
        let inspected = inspect(world.get_component::<DrawableComponent>(id).unwrap());
        assert_eq!(
            inspected["albedo"]["sampler"]["minification"],
            "nearest_mipmap_linear"
        );
        assert_eq!(inspected["albedo"]["uv"]["scale"], json!([-1., 0.]));
    }
}
