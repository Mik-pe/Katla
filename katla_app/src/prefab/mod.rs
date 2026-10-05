//! Reusable, self-contained scene subtrees. Instantiation appends transactionally.

pub mod control;
#[cfg(test)]
mod examples;
#[cfg(all(test, feature = "editor"))]
mod native_tests;
#[cfg(test)]
mod tests;

use crate::application::Application;
use crate::components::Parent;
use crate::scene::identity::SceneIdentity;
use crate::scene::{Scene, SceneAssetContext, SceneEntityId, TransformDescriptor};
use katla_ecs::EntityId;
use katla_gfx::GpuRenderer;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

/// Current `.katprefab` subtree version.
pub const PREFAB_VERSION: u32 = 1;
use crate::util::asset_io::{MAX_ASSET_BYTES, read_text};

/// A rooted scene template with private entity keys and no runtime handles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prefab {
    pub version: u32,
    pub root: SceneEntityId,
    pub scene: Scene,
}

/// Newly created IDs; all keys and references are private to this instance.
pub struct PrefabInstance {
    pub root: EntityId,
    pub entities: Vec<EntityId>,
}

impl Prefab {
    /// Validate a single tree whose root placement is supplied at instantiation.
    pub fn validate(&self) -> Result<(), String> {
        if self.version != PREFAB_VERSION {
            return Err(format!(
                "Unsupported prefab version {}; expected {PREFAB_VERSION}",
                self.version
            ));
        }
        self.scene.validate().map_err(|error| error.to_string())?;
        let root = self
            .scene
            .entities
            .iter()
            .find(|entity| entity.id == self.root)
            .ok_or("Prefab root does not exist")?;
        if root.parent.is_some() || root.transform != TransformDescriptor::default_transform() {
            return Err("Prefab root must have no parent and an identity transform; use instance placement or mesh-part transforms".into());
        }
        if self
            .scene
            .entities
            .iter()
            .any(|entity| entity.id != self.root && entity.parent.is_none())
        {
            return Err("Every prefab entity must descend from the single root".into());
        }
        Ok(())
    }

    /// Parse a bounded strict RON prefab; scene data uses the current scene schema.
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() > MAX_ASSET_BYTES {
            return Err("Prefab file exceeds the 64 MiB limit".into());
        }
        let prefab: Self = ron::from_str(text).map_err(|error| error.to_string())?;
        prefab.validate()?;
        Ok(prefab)
    }

    /// Read without changing engine state.
    pub fn load(path: &Path) -> Result<Self, String> {
        Self::parse(&read_text(path)?)
    }

    /// Atomically save a validated, canonically ordered template.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        let mut prefab = self.clone();
        prefab.scene.entities.sort_by_key(|entity| entity.id);
        let text = ron::ser::to_string_pretty(&prefab, crate::scene::ron_pretty_config())
            .map_err(|error| error.to_string())?;
        crate::util::asset_io::write_text(path, &text)
    }

    /// Append a fresh instance. Any preparation failure preserves the old world.
    /// Built-in asset paths are resolved against the prefab origin, not the active scene.
    pub fn instantiate(
        &self,
        app: &mut Application,
        origin: &Path,
        placement: TransformDescriptor,
    ) -> Result<PrefabInstance, String> {
        self.validate()?;
        let context = SceneAssetContext::new(&app.resources.root, Some(origin))
            .map_err(|error| error.to_string())?;
        let mut scene = self.scene.clone();
        for entity in &mut scene.entities {
            for key in entity.components.keys() {
                if !app.scene_components.contains(key) {
                    return Err(format!(
                        "Prefab component '{key}' requires a registered codec to remap instance references"
                    ));
                }
            }
            for (_, asset) in crate::scene::serialization::entity_assets_mut(entity) {
                *asset = crate::scene::AssetRef::File(context.resolve(asset)?);
            }
            if entity.id == self.root {
                entity.transform = placement.clone();
            }
        }
        let next = app
            .world
            .query_ref::<&SceneIdentity>()
            .fold(app.scene_document.next_entity_id, |next, (_, key)| {
                next.max(key.id.0.saturating_add(1))
            });
        let end = next
            .checked_add(scene.entities.len() as u64)
            .ok_or("Scene identity space exhausted")?;
        let prepared = crate::scene::serialization::stage_scene(app, &scene, &context)
            .map_err(|error| error.to_string())?;
        let mut root = None;
        for (index, (descriptor, entity)) in
            scene.entities.iter().zip(&prepared.entities).enumerate()
        {
            app.world.add_component(
                *entity,
                SceneIdentity {
                    id: SceneEntityId(next + index as u64),
                },
            );
            if descriptor.id == self.root {
                root = Some(*entity);
            }
            #[cfg(feature = "editor")]
            crate::application::editor::record_entity_gpu_handles(app, *entity);
        }
        app.scene_document.next_entity_id = end;
        // The root was checked before staging and the complete mapping is returned in order.
        let root = root.ok_or("Prepared prefab has no root")?;
        Ok(PrefabInstance {
            root,
            entities: prepared.entities,
        })
    }

    /// Export only the selected root and its descendants, rebasing built-in assets.
    /// External component/joint/trigger references are rejected, not silently redirected.
    pub fn capture(
        app: &mut Application,
        root: EntityId,
        destination: &Path,
    ) -> Result<Self, String> {
        let entities = subtree(app, root)?;
        let mut scene = crate::scene::capture::capture_selection(app, &entities, Some(root))
            .map_err(|error| error.to_string())?;
        let root_key = app
            .world
            .get_component::<SceneIdentity>(root)
            .ok_or("Root has no scene identity")?
            .id;
        let old = app
            .scene_document
            .assets
            .as_ref()
            .ok_or("Document has no asset context")?;
        let new = old
            .with_origin(destination)
            .map_err(|error| error.to_string())?;
        for entity in &mut scene.entities {
            for key in entity.components.keys() {
                if !app.scene_components.contains(key) {
                    return Err(format!(
                        "Prefab component '{key}' requires a registered codec"
                    ));
                }
            }
            for (_, asset) in crate::scene::serialization::entity_assets_mut(entity) {
                *asset = new.identify(&old.resolve(asset)?)?;
            }
            if entity.id == root_key {
                entity.transform = TransformDescriptor::default_transform();
            }
        }
        scene.name = app
            .world
            .get_component::<crate::components::NameComponent>(root)
            .map(|name| name.name.clone())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or("Prefab".into());
        scene.created_at = None;
        scene.modified_at = None;
        let prefab = Self {
            version: PREFAB_VERSION,
            root: root_key,
            scene,
        };
        prefab.validate()?;
        Ok(prefab)
    }
}

/// Instantiate a mesh asset as one entity or a prefab as a complete subtree.
pub fn instantiate_asset(
    app: &mut Application,
    path: &Path,
    placement: TransformDescriptor,
) -> Result<PrefabInstance, String> {
    let prefab = match path.extension().and_then(|value| value.to_str()) {
        Some("katmesh") => {
            let asset = crate::mesh_asset::MeshAsset::load(path)?;
            let mut scene = Scene::new(&asset.name);
            scene.next_entity_id = 2;
            let mut entity = crate::scene::EntityDescriptor::new(
                SceneEntityId(1),
                crate::scene::EntitySource::MeshAsset {
                    path: crate::scene::AssetRef::File(
                        path.canonicalize().map_err(|error| error.to_string())?,
                    ),
                },
            );
            entity.name = Some(asset.name);
            scene.entities.push(entity);
            Prefab {
                version: PREFAB_VERSION,
                root: SceneEntityId(1),
                scene,
            }
        }
        Some("katprefab") => Prefab::load(path)?,
        _ => return Err("Expected a .katmesh or .katprefab asset".into()),
    };
    prefab.instantiate(app, path, placement)
}

pub(crate) fn subtree(app: &Application, root: EntityId) -> Result<Vec<EntityId>, String> {
    if !app.world.entity_exists(root) {
        return Err("Prefab root is stale or missing".into());
    }
    let mut children = HashMap::<_, Vec<_>>::new();
    for (entity, parent) in app.world.query_ref::<&Parent>() {
        children.entry(parent.parent).or_default().push(entity);
    }
    let mut result = Vec::new();
    let mut pending = vec![root];
    let mut seen = HashSet::new();
    while let Some(entity) = pending.pop() {
        if !seen.insert(entity) {
            return Err("Prefab subtree contains a hierarchy cycle".into());
        }
        if app
            .world
            .get_component::<crate::components::EditorHidden>(entity)
            .is_some()
        {
            return Err("Editor-owned entities cannot be included in a prefab".into());
        }
        result.push(entity);
        if let Some(children) = children.get(&entity) {
            pending.extend(children);
        }
    }
    Ok(result)
}

/// Remove a complete instance and release only resources whose last reference dies.
pub fn remove_instance(app: &mut Application, root: EntityId) -> Result<(), String> {
    let entities = subtree(app, root)?;
    app.renderer.wait_for_device();
    for entity in entities.into_iter().rev() {
        if let Some(parent) = app.world.get_component::<Parent>(entity).map(|p| p.parent)
            && let Some(children) = app
                .world
                .get_component_mut::<crate::components::Children>(parent)
        {
            children.children.retain(|child| *child != entity);
        }
        if let Some(drawable) = app
            .world
            .get_component::<crate::components::DrawableComponent>(entity)
        {
            let resources = app.gpu_resource_tracker.release_drawable(
                drawable.mesh_handle,
                drawable.material_handle,
                drawable.skeleton_handle,
            );
            crate::scene::serialization::destroy_resources(app, resources);
        }
        let textures = app
            .world
            .get_component::<crate::application::spawning::ModelTextures>(entity)
            .map(|textures| textures.handles.clone())
            .unwrap_or_default();
        let mut resources = crate::gpu_resource_tracker::GpuResourcesToDestroy::default();
        for texture in textures {
            if app.gpu_resource_tracker.release_texture(texture) {
                resources.textures.push(texture);
            }
        }
        crate::scene::serialization::destroy_resources(app, resources);
        if let Some(emitter) = app
            .world
            .get_component_mut::<crate::components::ParticleEmitterComponent>(entity)
            && let Some(handle) = emitter.emitter_handle.take()
            && let Some(features) = &mut app.scene_features
        {
            katla_gfx::ParticleEmitterDriver::destroy_emitter(
                &mut features.particles,
                handle,
                emitter.kill_on_destroy,
            );
        }
        #[cfg(feature = "editor")]
        app.editor.entity_gpu_handles.remove(&entity);
        app.world.destroy_entity(entity);
    }
    Ok(())
}
