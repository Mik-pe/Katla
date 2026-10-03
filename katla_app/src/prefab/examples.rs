//! Reproducible shipped examples for the authoring and rendering contract.

use super::*;
use crate::mesh_asset::{Geometry, MESH_VERSION, MeshAsset, MeshPart};
use crate::scene::{
    AssetRef, ColliderShapeDescriptor, DrawableDescriptor, EntityDescriptor, EntitySource,
    RigidBodyDescriptor,
};

fn part(id: &str, position: [f32; 3], size: [f32; 3]) -> MeshPart {
    let mut transform = TransformDescriptor::default_transform();
    transform.position = position;
    MeshPart {
        id: id.into(),
        transform,
        geometry: Geometry::Cube { size },
    }
}

fn frame() -> MeshAsset {
    let mut parts = vec![
        part("seat", [0.0, 0.45, 0.0], [0.8, 0.1, 0.8]),
        part("back", [0.0, 0.9, -0.35], [0.8, 0.9, 0.1]),
    ];
    for (name, x, z) in [
        ("front_left", -0.32, 0.32),
        ("front_right", 0.32, 0.32),
        ("back_left", -0.32, -0.32),
        ("back_right", 0.32, -0.32),
    ] {
        parts.push(part(name, [x, 0.225, z], [0.08, 0.45, 0.08]));
    }
    MeshAsset {
        version: MESH_VERSION,
        name: "Chair frame".into(),
        parts,
    }
}

fn cushion() -> MeshAsset {
    MeshAsset {
        version: MESH_VERSION,
        name: "Chair cushion".into(),
        parts: vec![part("cushion", [0.0; 3], [0.68, 0.08, 0.65])],
    }
}

fn chair() -> Prefab {
    let mut scene = Scene::new("Chair");
    scene.next_entity_id = 4;
    let mut root = EntityDescriptor::new(SceneEntityId(1), EntitySource::Empty);
    root.name = Some("Chair".into());
    scene.entities.push(root);
    let mut frame = EntityDescriptor::new(
        SceneEntityId(2),
        EntitySource::MeshAsset {
            path: AssetRef::Resource("meshes/chair-frame.katmesh".into()),
        },
    );
    frame.name = Some("Frame".into());
    frame.parent = Some(SceneEntityId(1));
    frame.drawable = Some(DrawableDescriptor {
        textures: None,
        surface: None,
        sampling: None,
        color: Some([0.55, 0.29, 0.12, 1.0]),
        metallic: 0.0,
        roughness: 0.7,
        ao: 1.0,
    });
    frame.rigid_body = Some(RigidBodyDescriptor::new(katla_physics::BodyType::Static));
    frame.collider_shape = Some(ColliderShapeDescriptor::Trimesh);
    scene.entities.push(frame);
    let mut cushion = EntityDescriptor::new(
        SceneEntityId(3),
        EntitySource::MeshAsset {
            path: AssetRef::Resource("meshes/chair-cushion.katmesh".into()),
        },
    );
    cushion.name = Some("Cushion".into());
    cushion.parent = Some(SceneEntityId(1));
    cushion.transform.position = [0.0, 0.54, 0.02];
    cushion.drawable = Some(DrawableDescriptor {
        textures: None,
        surface: None,
        sampling: None,
        color: Some([0.85, 0.18, 0.09, 1.0]),
        metallic: 0.0,
        roughness: 0.9,
        ao: 1.0,
    });
    scene.entities.push(cushion);
    Prefab {
        version: PREFAB_VERSION,
        root: SceneEntityId(1),
        scene,
    }
}

fn workshop() -> Scene {
    let mut scene = Scene::new("Prefab workshop");
    let mut floor = EntityDescriptor::new(
        SceneEntityId(1),
        EntitySource::Plane {
            width: 12.0,
            height: 12.0,
        },
    );
    floor.name = Some("Floor".into());
    floor.transform.position[1] = -0.01;
    floor.drawable = Some(DrawableDescriptor {
        textures: None,
        surface: None,
        sampling: None,
        color: Some([0.48, 0.52, 0.56, 1.0]),
        metallic: 0.0,
        roughness: 0.85,
        ao: 1.0,
    });
    scene.entities.push(floor);
    let mut next = 2;
    for (index, position) in [
        [-1.2, 0.0, 0.0],
        [1.2, 0.0, 0.0],
        [-1.2, 0.0, -2.0],
        [1.2, 0.0, -2.0],
    ]
    .into_iter()
    .enumerate()
    {
        for mut entity in chair().scene.entities {
            let offset = next - 1;
            entity.id.0 += offset;
            entity.parent = entity.parent.map(|parent| SceneEntityId(parent.0 + offset));
            if entity.parent.is_none() {
                entity.name = Some(format!("Chair {}", index + 1));
                entity.transform.position = position;
            }
            if let Some(drawable) = &mut entity.drawable
                && entity.name.as_deref() == Some("Cushion")
            {
                drawable.color = Some(
                    [
                        [0.85, 0.18, 0.09, 1.0],
                        [0.08, 0.35, 0.65, 1.0],
                        [0.18, 0.55, 0.22, 1.0],
                        [0.75, 0.55, 0.08, 1.0],
                    ][index],
                );
            }
            scene.entities.push(entity);
        }
        next += 3;
    }
    let mut sun = EntityDescriptor::new(SceneEntityId(next), EntitySource::Empty);
    sun.name = Some("Sun".into());
    sun.directional_light = Some(crate::scene::descriptors::DirectionalLightDescriptor {
        direction: [-0.35, -1.0, -0.2],
        color: [1.0, 0.98, 0.95],
        intensity: 3.0,
    });
    scene.entities.push(sun);
    scene.next_entity_id = next + 1;
    scene
}

#[test]
#[ignore = "rewrites shipped prefab and mesh examples"]
fn test_regenerate_prefab_examples() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    frame()
        .save(&root.join("resources/meshes/chair-frame.katmesh"))
        .unwrap();
    cushion()
        .save(&root.join("resources/meshes/chair-cushion.katmesh"))
        .unwrap();
    chair()
        .save(&root.join("resources/prefabs/chair.katprefab"))
        .unwrap();
    let scene = crate::scene::SceneManager::to_ron(&workshop()).unwrap();
    crate::util::config::write_atomic(
        &root.join("assets/scenes/prefab-workshop.katla"),
        scene.as_bytes(),
    )
    .unwrap();
}

#[test]
fn test_shipped_prefab_mesh_and_scene_examples_remain_reproducible() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    assert_eq!(
        MeshAsset::load(&root.join("resources/meshes/chair-frame.katmesh")).unwrap(),
        frame()
    );
    assert_eq!(
        MeshAsset::load(&root.join("resources/meshes/chair-cushion.katmesh")).unwrap(),
        cushion()
    );
    assert_eq!(
        Prefab::load(&root.join("resources/prefabs/chair.katprefab")).unwrap(),
        chair()
    );
    let compiled = frame().compile().unwrap();
    assert_eq!(compiled.vertices.len(), 144);
    assert_eq!(compiled.indices.len(), 216);
    let scene = crate::scene::SceneManager::parse(
        &std::fs::read_to_string(root.join("assets/scenes/prefab-workshop.katla")).unwrap(),
    )
    .unwrap();
    assert_eq!(scene, workshop());
}
