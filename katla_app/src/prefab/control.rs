//! One validated authoring path for MCP, editor AI and native regression fixtures.

use super::{PREFAB_VERSION, Prefab};
use crate::scene::{
    AssetRef, EntityDescriptor, EntitySource, Scene, SceneEntityId, TransformDescriptor,
};
use crate::{
    application::Application,
    mesh_asset::{Geometry, MESH_VERSION, MeshAsset, MeshPart},
};
use katla_agent::prefab::PrefabOp;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// Execute an AI asset operation, returning structured feedback for the next edit.
pub fn execute(app: &mut Application, op: PrefabOp) -> Result<Value, String> {
    #[cfg(feature = "editor")]
    if matches!(
        &op,
        PrefabOp::Instantiate { .. } | PrefabOp::Capture { .. } | PrefabOp::Remove { .. }
    ) && app.play_mode != crate::application::game_state::PlayMode::Editing
    {
        return Err("Stop simulation before editing or capturing prefab instances".into());
    }
    match op {
        PrefabOp::Describe => describe(),
        PrefabOp::Read { path } => {
            let file = project_file(app, &path)?;
            match extension(&file)? {
                "katmesh" => {
                    serde_json::to_value(MeshAsset::load(&file)?).map_err(|error| error.to_string())
                }
                _ => serde_json::to_value(Prefab::load(&file)?).map_err(|error| error.to_string()),
            }
        }
        PrefabOp::Validate { path, document } => {
            validate_document(app, &project_file(app, &path)?, &document)
        }
        PrefabOp::Write { path, document } => {
            let file = project_file(app, &path)?;
            let stats = validate_document(app, &file, &document)?;
            match extension(&file)? {
                "katmesh" => serde_json::from_value::<MeshAsset>(document)
                    .map_err(|error| error.to_string())?
                    .save(&file)?,
                _ => serde_json::from_value::<Prefab>(document)
                    .map_err(|error| error.to_string())?
                    .save(&file)?,
            }
            Ok(json!({"path":path,"saved":true,"stats":stats}))
        }
        PrefabOp::Instantiate {
            path,
            position,
            rotation,
            scale,
        } => {
            let file = project_file(app, &path)?;
            let instance = super::instantiate_asset(
                app,
                &file,
                TransformDescriptor {
                    position,
                    rotation,
                    scale,
                },
            )?;
            let nodes: Vec<_> = instance.entities.iter().map(|id| json!({
                "entity_id":id.id().to_string(),
                "name":app.world.get_component::<crate::components::NameComponent>(*id).map(|n| &n.name),
                "parent_id":app.world.get_component::<crate::components::Parent>(*id).map(|p| p.parent.id().to_string()),
                "scene_key":app.world.get_component::<crate::scene::identity::SceneIdentity>(*id).map(|key| key.id.0)
            })).collect();
            Ok(
                json!({"root_entity":instance.root.id().to_string(),"entities":instance.entities.iter().map(|entity| entity.id().to_string()).collect::<Vec<_>>(),"nodes":nodes,"path":path}),
            )
        }
        PrefabOp::Capture { path, root_entity } => {
            let file = project_file(app, &path)?;
            if extension(&file)? != "katprefab" {
                return Err("Capture requires a .katprefab destination".into());
            }
            let prefab = Prefab::capture(app, entity_id(&root_entity)?, &file)?;
            prefab.save(&file)?;
            Ok(json!({"path":path,"entities":prefab.scene.entities.len(),"root":prefab.root.0}))
        }
        PrefabOp::Remove { root_entity } => {
            super::remove_instance(app, entity_id(&root_entity)?)?;
            Ok(json!({"removed_root":root_entity}))
        }
    }
}

fn entity_id(value: &str) -> Result<katla_ecs::EntityId, String> {
    value
        .parse::<u64>()
        .map(katla_ecs::EntityId::from_raw)
        .map_err(|_| "Use a full decimal entity ID string".into())
}

fn validate_document(app: &Application, file: &Path, document: &Value) -> Result<Value, String> {
    match extension(file)? {
        "katmesh" => {
            let asset: MeshAsset =
                serde_json::from_value(document.clone()).map_err(|error| error.to_string())?;
            let mesh = asset.compile()?;
            Ok(
                json!({"parts":asset.parts.len(),"vertices":mesh.vertices.len(),"triangles":mesh.indices.len()/3,"draws_per_entity":1,"bounds":{"min":mesh.bounds.min().to_array(),"max":mesh.bounds.max().to_array()}}),
            )
        }
        _ => {
            let prefab: Prefab =
                serde_json::from_value(document.clone()).map_err(|error| error.to_string())?;
            prefab.validate()?;
            let context = crate::scene::SceneAssetContext::new(&app.resources.root, Some(file))
                .map_err(|error| error.to_string())?;
            for entity in &prefab.scene.entities {
                for key in entity.components.keys() {
                    if !app.scene_components.contains(key) {
                        return Err(format!(
                            "Prefab component '{key}' requires a registered codec"
                        ));
                    }
                }
            }
            crate::scene::serialization::preflight_scene(app, &prefab.scene, &context)
                .map_err(|error| error.to_string())?;
            Ok(json!({"entities":prefab.scene.entities.len(),"root":prefab.root.0}))
        }
    }
}

fn extension(file: &Path) -> Result<&str, String> {
    match file.extension().and_then(|extension| extension.to_str()) {
        Some("katmesh") => Ok("katmesh"),
        Some("katprefab") => Ok("katprefab"),
        _ => Err("Expected a .katmesh or .katprefab asset".into()),
    }
}

fn project_file(app: &Application, relative: &str) -> Result<PathBuf, String> {
    let file = crate::util::asset_io::project_file(&app.resources.root, relative)?;
    extension(&file)?;
    Ok(file)
}

pub(super) fn describe() -> Result<Value, String> {
    let mesh = MeshAsset {
        version: MESH_VERSION,
        name: "Seat".into(),
        parts: vec![MeshPart {
            id: "seat".into(),
            transform: TransformDescriptor::default_transform(),
            geometry: Geometry::Cube {
                size: [0.8, 0.1, 0.8],
            },
        }],
    };
    let mut scene = Scene::new("Chair");
    scene.next_entity_id = 3;
    let mut root = EntityDescriptor::new(SceneEntityId(1), EntitySource::Empty);
    root.name = Some("Chair".into());
    scene.entities.push(root);
    let mut seat = EntityDescriptor::new(
        SceneEntityId(2),
        EntitySource::MeshAsset {
            path: AssetRef::Resource("meshes/seat.katmesh".into()),
        },
    );
    seat.parent = Some(SceneEntityId(1));
    seat.transform.position[1] = 0.45;
    scene.entities.push(seat);
    Ok(json!({
        "mesh_example":mesh,
        "prefab_example":Prefab { version: PREFAB_VERSION, root: SceneEntityId(1), scene },
        "geometry_kinds":["cube","sphere","plane","cylinder","cone","torus","triangles"],
        "triangles_fields":{"positions":"array of XYZ positions","indices":"CCW index triples","normals":"optional per-position XYZ normals","uvs":"optional per-position UV pairs"},
        "limits":{"parts":1024,"vertices":1_000_000,"indices":6_000_000,"file_bytes":super::MAX_ASSET_BYTES},
        "workflow":"Read/describe; edit named parts in JSON; validate; write .katmesh first then .katprefab; instantiate; inspect using editor_view; instantiate a revision successfully before removing the old preview. Capture exports an edited subtree. Writing assets does not mutate live instances; re-instantiation or scene reload reads the new assets.",
        "transforms":"meters, right-handed Y up, rotation quaternion XYZW, root identity; placement supplied when instantiating",
        "materials":"one PBR material per mesh entity; use separate prefab children for different materials or independent gameplay parts"
    }))
}
