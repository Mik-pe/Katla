use super::capture::capture_entities;
use super::component_registry::SceneReadContext;
use super::identity::SceneIdentity;
use super::spawn::spawn_entity;
use super::{AssetRef, EntityDescriptor, EntitySource, Scene, SceneAssetContext, SceneError};
use crate::application::Application;
use crate::components::{DrawableComponent, ParticleEmitterComponent};
use katla_gfx::GpuRenderer;
use log::info;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use ron::extensions::Extensions;

/// Current scene format version.
pub const SCENE_VERSION: u32 = 3;

/// RON serialization extensions configuration.
///
/// Enables concise optional and newtype values. Comments and formatting are
/// accepted when reading but regenerated when saving.
const RON_EXTENSIONS: Extensions = Extensions::IMPLICIT_SOME
    .union(Extensions::UNWRAP_NEWTYPES)
    .union(Extensions::UNWRAP_VARIANT_NEWTYPES);

pub fn ron_pretty_config() -> ron::ser::PrettyConfig {
    ron::ser::PrettyConfig::new()
        .enumerate_arrays(true)
        .extensions(RON_EXTENSIONS)
}

/// Manages scene save/load operations.
pub struct SceneManager;

impl SceneManager {
    /// Capture serializable entities, assigning persistent keys to new entities.
    /// Registered game codecs must explicitly encode live entity references.
    pub fn save_scene(app: &mut Application) -> Result<Scene, SceneError> {
        super::capture::capture_scene(app)
    }

    /// Parse v3 or migrate a v0/v1/v2 document, then validate it without engine state.
    pub fn parse(content: &str) -> Result<Scene, SceneError> {
        let scene = super::migration::parse_scene(content)?;
        scene.validate()?;
        Ok(scene)
    }

    /// Validate and emit deterministic RON ordered by persistent key.
    pub fn to_ron(scene: &Scene) -> Result<String, SceneError> {
        scene.validate()?;
        let mut scene = scene.clone();
        scene.entities.sort_by_key(|entity| entity.id);
        ron::ser::to_string_pretty(&scene, ron_pretty_config()).map_err(SceneError::Encode)
    }

    /// Atomically save and update the document only after writing succeeds.
    /// Save As rebases scene-relative assets to the destination's directory.
    pub fn save_to_file(app: &mut Application, path: &Path) -> Result<(), SceneError> {
        let destination = absolute_path(path)?;
        let mut scene = Self::save_scene(app)?;
        let old_context = app
            .scene_document
            .assets
            .clone()
            .ok_or_else(|| SceneError::Capture("Document roots were not captured".into()))?;
        let new_context =
            old_context
                .with_origin(&destination)
                .map_err(|source| SceneError::Io {
                    path: destination.clone(),
                    source,
                })?;
        for entity in &mut scene.entities {
            let id = entity.id;
            for (_, reference) in entity_assets_mut(entity) {
                let absolute = old_context
                    .resolve(reference)
                    .map_err(|error| SceneError::entity(id, "asset", error))?;
                *reference = new_context
                    .identify(&absolute)
                    .map_err(|error| SceneError::entity(id, "asset", error))?;
            }
        }
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|value| value.as_secs().to_string());
        scene.created_at = scene.created_at.or_else(|| timestamp.clone());
        scene.modified_at = timestamp;
        scene.engine_version = Some(env!("CARGO_PKG_VERSION").into());
        let text = Self::to_ron(&scene)?;
        crate::util::config::write_atomic(path, text.as_bytes()).map_err(|source| {
            SceneError::Io {
                path: path.into(),
                source,
            }
        })?;
        let sources: HashMap<_, _> = scene
            .entities
            .iter()
            .map(|entity| (entity.id, entity.source.clone()))
            .collect();
        let updates: Vec<_> = app
            .world
            .query_ref::<&SceneIdentity>()
            .filter_map(|(entity, key)| sources.get(&key.id).map(|source| (entity, source.clone())))
            .collect();
        for (entity, source) in updates {
            app.world.add_component(entity, source);
        }
        app.scene_document.saved = scene;
        app.scene_document.assets = Some(new_context);
        app.scene_document.path = Some(destination);
        info!("Saved scene to {}", path.display());
        Ok(())
    }

    /// Preflight the complete file before staging replacement entities.
    pub fn load_from_file(app: &mut Application, path: &Path) -> Result<(), SceneError> {
        let size = std::fs::metadata(path)
            .map_err(|source| SceneError::Io {
                path: path.into(),
                source,
            })?
            .len();
        if size > 64 * 1024 * 1024 {
            return Err(SceneError::Limit {
                field: "file size in bytes",
                maximum: 64 * 1024 * 1024,
            });
        }
        let content = std::fs::read_to_string(path).map_err(|source| SceneError::Io {
            path: path.into(),
            source,
        })?;
        Self::load_with_origin(app, Self::parse(&content)?, Some(path))
    }

    /// Load an in-memory current-version scene. Scene-relative assets need an origin.
    pub fn load_scene(app: &mut Application, scene: Scene) -> Result<(), SceneError> {
        Self::load_with_origin(app, scene, None)
    }

    pub(crate) fn load_with_origin(
        app: &mut Application,
        scene: Scene,
        origin: Option<&Path>,
    ) -> Result<(), SceneError> {
        let context = asset_context(app, origin)?;
        let path = origin.map(absolute_path).transpose()?;
        Self::load_with_context(app, scene, context, path)
    }

    pub(crate) fn load_with_context(
        app: &mut Application,
        scene: Scene,
        context: SceneAssetContext,
        path: Option<std::path::PathBuf>,
    ) -> Result<(), SceneError> {
        info!(
            "Loading scene '{}' (version {}) with {} entities",
            scene.name,
            scene.version,
            scene.entities.len()
        );
        app.renderer.wait_for_device();
        let previous_entities: HashSet<_> = app.world.entity_ids().collect();
        let mut previous_tracker = app.gpu_resource_tracker.clone();
        let prepared = stage_scene(app, &scene, &context)?;
        let baseline = prepared.baseline;
        for id in previous_entities {
            if app
                .world
                .get_component::<crate::components::EditorHidden>(id)
                .is_some()
            {
                if let Some(drawable) = app.world.get_component::<DrawableComponent>(id) {
                    previous_tracker.release_drawable(
                        drawable.mesh_handle,
                        drawable.material_handle,
                        drawable.skeleton_handle,
                    );
                }
                continue;
            }
            if let Some(emitter) = app.world.get_component_mut::<ParticleEmitterComponent>(id)
                && let Some(handle) = emitter.emitter_handle.take()
                && let Some(features) = &mut app.scene_features
            {
                katla_gfx::ParticleEmitterDriver::destroy_emitter(
                    &mut features.particles,
                    handle,
                    emitter.kill_on_destroy,
                );
            }
            app.world.destroy_entity(id);
        }
        if let Some(commands) = app
            .world
            .get_resource_mut::<katla_script::PendingParticleCommands>()
        {
            commands.0.clear();
        }
        let retired = app.gpu_resource_tracker.retire_snapshot(previous_tracker);
        destroy_resources(app, retired);
        app.scene_document.assets = Some(context);
        app.scene_document.next_entity_id = baseline.next_entity_id;
        app.scene_document.saved = baseline;
        app.scene_document.path = path;
        #[cfg(feature = "editor")]
        {
            app.editor.clear_entity_references();
            for entity in prepared.entities {
                crate::application::editor::record_entity_gpu_handles(app, entity);
            }
        }
        Ok(())
    }
}

pub(crate) fn destroy_resources(
    app: &mut Application,
    resources: crate::gpu_resource_tracker::GpuResourcesToDestroy,
) {
    for handle in resources.meshes {
        app.geometry_cache.remove(handle);
        app.mesh_assets.remove(handle);
        if let Some(cache) = app
            .world
            .get_resource_mut::<crate::geometry_cache::GeometryCache>()
        {
            cache.remove(handle);
        }
        app.renderer.destroy_mesh(handle);
    }
    for handle in resources.materials {
        app.renderer.destroy_material(handle);
    }
    for handle in resources.textures {
        app.renderer.destroy_texture(handle);
    }
    for handle in resources.skeletons {
        app.renderer.destroy_skeleton(handle);
    }
}

fn absolute_path(path: &Path) -> Result<std::path::PathBuf, SceneError> {
    if path.is_absolute() {
        Ok(path.into())
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .map_err(|source| SceneError::Io {
                path: path.into(),
                source,
            })
    }
}

pub(super) fn asset_context(
    app: &Application,
    origin: Option<&Path>,
) -> Result<SceneAssetContext, SceneError> {
    SceneAssetContext::new(&app.resources.root, origin).map_err(|source| SceneError::Io {
        path: app.resources.root.clone(),
        source,
    })
}

pub(crate) fn entity_assets(entity: &EntityDescriptor) -> Vec<(&'static str, &AssetRef)> {
    let mut assets = Vec::new();
    if let EntitySource::GltfModel { path }
    | EntitySource::GltfGroup { path }
    | EntitySource::GltfPrimitive { path, .. }
    | EntitySource::StlModel { path }
    | EntitySource::MeshAsset { path } = &entity.source
    {
        assets.push(("source.path", path));
    }
    if let Some(script) = &entity.script {
        assets.push(("script.path", &script.path));
    }
    if let Some(audio) = &entity.audio_emitter {
        assets.push(("audio_emitter.path", &audio.path));
    }
    assets
}
pub(crate) fn entity_assets_mut(
    entity: &mut EntityDescriptor,
) -> Vec<(&'static str, &mut AssetRef)> {
    let mut assets = Vec::new();
    if let EntitySource::GltfModel { path }
    | EntitySource::GltfGroup { path }
    | EntitySource::GltfPrimitive { path, .. }
    | EntitySource::StlModel { path }
    | EntitySource::MeshAsset { path } = &mut entity.source
    {
        assets.push(("source.path", path));
    }
    if let Some(script) = &mut entity.script {
        assets.push(("script.path", &mut script.path));
    }
    if let Some(audio) = &mut entity.audio_emitter {
        assets.push(("audio_emitter.path", &mut audio.path));
    }
    assets
}

/// A fully prepared subtree; caller decides whether to replace or append.
pub(crate) struct PreparedScene {
    pub(crate) entities: Vec<katla_ecs::EntityId>,
    pub(crate) baseline: Scene,
}

pub(crate) fn preflight_scene(
    app: &Application,
    scene: &Scene,
    context: &SceneAssetContext,
) -> Result<(), SceneError> {
    scene.validate()?;
    let mut meshes = HashSet::new();
    for entity in &scene.entities {
        app.scene_components.validate(&entity.components)?;
        for (field, asset) in entity_assets(entity) {
            let file = context
                .resolve(asset)
                .map_err(|error| SceneError::entity(entity.id, field, error))?;
            if !file.is_file() {
                return Err(SceneError::entity(
                    entity.id,
                    field,
                    format!("Asset {} is not a file", file.display()),
                ));
            }
        }
        if let EntitySource::MeshAsset { path } = &entity.source {
            let file = context
                .resolve(path)
                .map_err(|error| SceneError::entity(entity.id, "source.path", error))?;
            if meshes.insert(file.clone()) {
                let asset = crate::mesh_asset::MeshAsset::load(&file)
                    .map_err(|error| SceneError::entity(entity.id, "source.path", error))?;
                let key = asset
                    .geometry_key()
                    .map_err(|error| SceneError::entity(entity.id, "source.path", error))?;
                if !app.mesh_assets.contains(&key) {
                    asset
                        .compile()
                        .map_err(|error| SceneError::entity(entity.id, "source.path", error))?;
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn stage_scene(
    app: &mut Application,
    scene: &Scene,
    context: &SceneAssetContext,
) -> Result<PreparedScene, SceneError> {
    preflight_scene(app, scene, context)?;
    app.renderer.wait_for_device();
    let previous_entities: HashSet<_> = app.world.entity_ids().collect();
    let previous_tracker = app.gpu_resource_tracker.clone();
    let staged = (|| {
        let mut mapping = HashMap::new();
        let mut entities = Vec::with_capacity(scene.entities.len());
        for desc in &scene.entities {
            let entity = spawn_entity(app, desc, context)
                .map_err(|error| SceneError::entity(desc.id, "source", error))?;
            mapping.insert(desc.id, entity);
            entities.push(entity);
        }
        let references = SceneReadContext { entities: mapping };
        for desc in &scene.entities {
            let entity = references
                .entity(desc.id)
                .map_err(|error| SceneError::entity(desc.id, "id", error))?;
            if !desc.trigger_rules.is_empty() {
                let rules = desc
                    .trigger_rules
                    .iter()
                    .map(|rule| {
                        rule.map_entities(|key| references.entity(*key).map(|entity| entity.id()))
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| SceneError::entity(desc.id, "trigger_rules", error))?;
                let rules = crate::events::TriggerRules::new(rules)
                    .map_err(|error| SceneError::entity(desc.id, "trigger_rules", error))?;
                app.world.add_component(entity, rules);
            }
            if let Some(parent) = desc.parent {
                let parent = references
                    .entity(parent)
                    .map_err(|error| SceneError::entity(desc.id, "parent", error))?;
                app.world
                    .add_component(entity, crate::components::Parent::new(parent));
                if let Some(children) = app
                    .world
                    .get_component_mut::<crate::components::Children>(parent)
                {
                    children.children.push(entity);
                } else {
                    app.world
                        .add_component(parent, crate::components::Children::new(vec![entity]));
                }
            }
            if let Some(joint) = &desc.joint {
                let a = references
                    .entity(joint.a)
                    .map_err(|error| SceneError::entity(desc.id, "joint.a", error))?;
                let b = references
                    .entity(joint.b)
                    .map_err(|error| SceneError::entity(desc.id, "joint.b", error))?;
                app.world.add_component(
                    entity,
                    katla_physics::Joint {
                        joint_type: joint.kind,
                        entity_a: a.id(),
                        entity_b: b.id(),
                        anchor_a: joint.anchor_a,
                        anchor_b: joint.anchor_b,
                        limits: joint
                            .limits
                            .map(|[min, max]| katla_physics::JointLimits { min, max }),
                        joint_handle: None,
                    },
                );
            }
        }
        app.scene_components
            .restore(&mut app.world, scene, &references)?;
        entities = app
            .world
            .entity_ids()
            .filter(|entity| !previous_entities.contains(entity))
            .collect();
        let mut next = scene.next_entity_id;
        for entity in &entities {
            if app.world.get_component::<SceneIdentity>(*entity).is_none() {
                let id = super::SceneEntityId(next);
                next = next
                    .checked_add(1)
                    .ok_or_else(|| SceneError::Capture("Scene identity space exhausted".into()))?;
                app.world.add_component(*entity, SceneIdentity { id });
            }
        }
        let mut expanded = scene.clone();
        expanded.next_entity_id = next;
        // Encoding can fail in game codecs. Prepare the baseline before retiring anything.
        let baseline = capture_entities(app, &entities, expanded, context)?;
        Ok(PreparedScene { entities, baseline })
    })();
    let prepared = match staged {
        Ok(prepared) => prepared,
        Err(error) => {
            let prepared: Vec<_> = app
                .world
                .entity_ids()
                .filter(|id| !previous_entities.contains(id))
                .collect();
            for id in prepared {
                app.world.destroy_entity(id);
            }
            let abandoned = app.gpu_resource_tracker.rollback_to(previous_tracker);
            destroy_resources(app, abandoned);
            return Err(error);
        }
    };
    Ok(prepared)
}
