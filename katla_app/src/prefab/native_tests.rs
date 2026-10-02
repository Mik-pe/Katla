//! Native upload, sharing, rollback and AI authoring acceptance.

use super::*;
use crate::application::ApplicationBuilder;
use crate::components::{DrawableComponent, NameComponent, TransformComponent};
use crate::mesh_asset::{Geometry, MeshAsset};
use crate::scene::{AssetRef, EntityDescriptor, EntitySource, SceneManager};
use crate::{ApplicationFrameGraph, empty_frame_graph};
use katla_agent::prefab::PrefabOp;
use katla_gfx::GpuRenderer;
use serde_json::json;

fn setup() -> (Application, std::path::PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let mut app = ApplicationBuilder::new()
        .validation_layer(true)
        .with_frame_graph(|renderer, _| Ok(ApplicationFrameGraph::new(empty_frame_graph(renderer))))
        .build_headless(1, String::new())
        .unwrap();
    let root = std::env::temp_dir().join(format!(
        "katla-prefab-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(root.join("resources")).unwrap();
    app.resources.root = root.join("resources");
    (app, root)
}

fn mesh(app: &mut Application) -> serde_json::Value {
    control::execute(app, PrefabOp::Describe).unwrap()["mesh_example"].clone()
}

fn write(app: &mut Application, path: &str, document: serde_json::Value) {
    let result = control::execute(
        app,
        PrefabOp::Write {
            path: path.into(),
            document,
        },
    )
    .unwrap();
    assert_eq!(result["saved"], true);
}

fn template() -> Prefab {
    let mut prefab = super::tests::template();
    prefab.scene.entities[0].name = Some("Object".into());
    prefab.scene.entities[1].source = EntitySource::MeshAsset {
        path: AssetRef::Scene("body.katmesh".into()),
    };
    prefab.scene.entities[1].name = Some("Body".into());
    prefab
}

fn instantiate(app: &mut Application, path: &str, x: f32) -> EntityId {
    let result = control::execute(
        app,
        PrefabOp::Instantiate {
            path: path.into(),
            position: [x, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
        },
    )
    .unwrap();
    EntityId::from_raw(result["root_entity"].as_str().unwrap().parse().unwrap())
}

fn body(app: &Application, root: EntityId) -> (EntityId, katla_gfx::MeshHandle) {
    let entity = app
        .world
        .query_ref::<&Parent>()
        .find(|(_, parent)| parent.parent == root)
        .unwrap()
        .0;
    (
        entity,
        app.world
            .get_component::<DrawableComponent>(entity)
            .unwrap()
            .mesh_handle,
    )
}

#[test]
#[ignore = "requires native Vulkan or Metal with API validation"]
fn test_native_ai_mesh_prefab_write_preview_save_reload_and_shared_cleanup() {
    let (mut app, dir) = setup();
    let mut document = mesh(&mut app);
    let mut second = document["parts"][0].clone();
    second["id"] = json!("leg");
    second["transform"]["position"] = json!([1.0, 0.0, 0.0]);
    document["parts"].as_array_mut().unwrap().push(second);
    write(&mut app, "assets/body.katmesh", document.clone());
    assert_eq!(
        control::execute(
            &mut app,
            PrefabOp::Read {
                path: "assets/body.katmesh".into()
            }
        )
        .unwrap(),
        document
    );
    write(
        &mut app,
        "assets/object.katprefab",
        serde_json::to_value(template()).unwrap(),
    );
    let first = instantiate(&mut app, "assets/object.katprefab", 0.0);
    let second = instantiate(&mut app, "assets/object.katprefab", 4.0);
    let first_body = body(&app, first);
    let second_body = body(&app, second);
    assert_eq!(first_body.1, second_body.1);
    let first_bounds = crate::systems::subtree_render_bounds(&app.world, first).unwrap();
    let second_bounds = crate::systems::subtree_render_bounds(&app.world, second).unwrap();
    assert!((second_bounds.center.x() - first_bounds.center.x() - 4.0).abs() < 1e-5);
    assert_eq!(app.renderer.mesh_index_count(first_body.1), Some(72));
    assert_eq!(app.gpu_resource_tracker.mesh_count(), 1);
    assert!(std::sync::Arc::ptr_eq(
        app.geometry_cache.get(first_body.1).unwrap(),
        app.world
            .get_resource::<crate::geometry_cache::GeometryCache>()
            .unwrap()
            .get(first_body.1)
            .unwrap()
    ));
    let path = dir.join("scenes/level.katla");
    SceneManager::save_to_file(&mut app, &path).unwrap();
    let saved = app.scene_document.saved.clone();
    assert_eq!(saved.entities.len(), 4);
    assert_eq!(
        saved
            .entities
            .iter()
            .map(|entity| entity.id)
            .collect::<HashSet<_>>()
            .len(),
        4
    );
    let first_key = app.world.get_component::<SceneIdentity>(first).unwrap().id;
    SceneManager::load_from_file(&mut app, &path).unwrap();
    assert!(!app.world.entity_exists(first));
    let root = app
        .world
        .query_ref::<&SceneIdentity>()
        .find(|(_, key)| key.id == first_key)
        .unwrap()
        .0;
    assert_eq!(body(&app, root).1, first_body.1);
    assert_eq!(
        SceneManager::save_scene(&mut app).unwrap().entities,
        saved.entities
    );
    let roots: Vec<_> = app
        .world
        .query_ref::<&NameComponent>()
        .filter(|(_, name)| name.name == "Object")
        .map(|(id, _)| id)
        .collect();
    remove_instance(&mut app, roots[0]).unwrap();
    assert!(app.geometry_cache.get(first_body.1).is_some());
    remove_instance(&mut app, roots[1]).unwrap();
    assert!(app.geometry_cache.get(first_body.1).is_none());
    assert_eq!(app.gpu_resource_tracker.mesh_count(), 0);
    let third = instantiate(&mut app, "assets/object.katprefab", 8.0);
    assert_ne!(body(&app, third).1, first_body.1);
    assert_eq!(app.renderer.mesh_index_count(body(&app, third).1), Some(72));
}

#[test]
#[ignore = "requires native Vulkan or Metal with API validation"]
fn test_native_mesh_revision_keeps_old_instances_and_failed_write_keeps_file() {
    let (mut app, dir) = setup();
    let document = mesh(&mut app);
    write(&mut app, "assets/body.katmesh", document.clone());
    let first = instantiate(&mut app, "assets/body.katmesh", 0.0);
    let old = app
        .world
        .get_component::<DrawableComponent>(first)
        .unwrap()
        .mesh_handle;
    let before = std::fs::read_to_string(dir.join("assets/body.katmesh")).unwrap();
    let mut invalid = document.clone();
    invalid["parts"][0]["geometry"]["size"][0] = json!(-1.0);
    assert!(
        control::execute(
            &mut app,
            PrefabOp::Write {
                path: "assets/body.katmesh".into(),
                document: invalid
            }
        )
        .is_err()
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("assets/body.katmesh")).unwrap(),
        before
    );
    let mut revised = document;
    revised["parts"][0]["geometry"]["size"] = json!([2.0, 2.0, 2.0]);
    write(&mut app, "assets/body.katmesh", revised);
    let second = instantiate(&mut app, "assets/body.katmesh", 4.0);
    let new = app
        .world
        .get_component::<DrawableComponent>(second)
        .unwrap()
        .mesh_handle;
    assert_ne!(old, new);
    assert_eq!(app.renderer.mesh_index_count(old), Some(36));
    assert_eq!(
        app.geometry_cache.get(new).unwrap().positions[0][0].abs(),
        1.0
    );
    assert!(
        control::execute(
            &mut app,
            PrefabOp::Remove {
                root_entity: u64::MAX.to_string()
            }
        )
        .is_err()
    );
    assert!(
        control::execute(
            &mut app,
            PrefabOp::Read {
                path: "../body.katmesh".into()
            }
        )
        .is_err()
    );
}

#[test]
#[ignore = "requires native Vulkan or Metal with API validation"]
fn test_native_prefab_partial_stage_failure_rolls_back_cache_and_world() {
    let (mut app, dir) = setup();
    let document = mesh(&mut app);
    write(&mut app, "assets/body.katmesh", document);
    let keep = app
        .world
        .spawn((TransformComponent::default(), NameComponent::new("Keep")));
    let original = SceneManager::save_scene(&mut app).unwrap();
    let counter = app.scene_document.next_entity_id;
    std::fs::write(dir.join("assets/broken.glb"), b"not a GLTF file").unwrap();
    let mut prefab = template();
    let mut broken = EntityDescriptor::new(
        SceneEntityId(3),
        EntitySource::GltfModel {
            path: AssetRef::Scene("broken.glb".into()),
        },
    );
    broken.parent = Some(SceneEntityId(1));
    prefab.scene.entities.push(broken);
    prefab.scene.next_entity_id = 4;
    assert!(
        prefab
            .instantiate(
                &mut app,
                &dir.join("assets/object.katprefab"),
                TransformDescriptor::default_transform()
            )
            .is_err()
    );
    assert!(app.world.entity_exists(keep));
    assert_eq!(app.scene_document.next_entity_id, counter);
    assert_eq!(app.gpu_resource_tracker.mesh_count(), 0);
    assert_eq!(SceneManager::save_scene(&mut app).unwrap(), original);
    let new = instantiate(&mut app, "assets/body.katmesh", 0.0);
    assert_eq!(
        app.renderer.mesh_index_count(
            app.world
                .get_component::<DrawableComponent>(new)
                .unwrap()
                .mesh_handle
        ),
        Some(36)
    );
}

#[derive(katla_ecs::Component)]
struct Link {
    #[inspect(skip)]
    target: EntityId,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct LinkData {
    target: SceneEntityId,
}

#[test]
#[ignore = "requires native Vulkan or Metal with API validation"]
fn test_native_prefab_export_remaps_custom_joint_trigger_and_parent_references() {
    let (mut app, dir) = setup();
    app.scene_components
        .register_codec::<Link, LinkData>(
            "game.link",
            1,
            |link, context| {
                Ok(LinkData {
                    target: context.id(link.target)?,
                })
            },
            |data, context| {
                Ok(Link {
                    target: context.entity(data.target)?,
                })
            },
        )
        .unwrap();
    let outside = app.world.spawn((TransformComponent::default(),));
    let root = app.world.spawn((
        TransformComponent::from_position(katla_math::Vec3::new(8.0, 0.0, 0.0)),
        NameComponent::new("Rig"),
        Parent::new(outside),
    ));
    let a = app.world.spawn((
        TransformComponent::default(),
        Parent::new(root),
        katla_physics::RigidBody::kinematic(),
        katla_physics::ColliderShape::Sphere(katla_physics::SphereShape::new(0.5)),
    ));
    let b = app.world.spawn((
        TransformComponent::default(),
        Parent::new(root),
        katla_physics::RigidBody::kinematic(),
        katla_physics::ColliderShape::Sphere(katla_physics::SphereShape::new(0.5)),
    ));
    app.world.add_component(root, Link { target: a });
    app.world.spawn((
        TransformComponent::default(),
        Parent::new(root),
        katla_physics::Joint::fixed(a.id(), b.id(), [0.0; 3], [0.0; 3]),
    ));
    app.world
        .add_component(b, katla_physics::TriggerVolume::new());
    app.world.add_component(
        b,
        crate::events::TriggerRules::new(vec![katla_agent::events::TriggerRule {
            event: katla_agent::events::TriggerPhase::Enter,
            other_entity: Some(a.id()),
            once: true,
            actions: vec![katla_agent::events::EventAction::Emit {
                name: "entered".into(),
            }],
        }])
        .unwrap(),
    );
    let destination = dir.join("assets/rig.katprefab");
    let prefab = Prefab::capture(&mut app, root, &destination).unwrap();
    prefab.save(&destination).unwrap();
    let instance = Prefab::load(&destination)
        .unwrap()
        .instantiate(
            &mut app,
            &destination,
            TransformDescriptor::default_transform(),
        )
        .unwrap();
    assert_eq!(instance.entities.len(), 4);
    let linked = app
        .world
        .get_component::<Link>(instance.root)
        .unwrap()
        .target;
    assert_ne!(linked, a);
    assert!(instance.entities.contains(&linked));
    assert_eq!(
        app.world.get_component::<Parent>(linked).unwrap().parent,
        instance.root
    );
    let joint = instance
        .entities
        .iter()
        .find_map(|id| app.world.get_component::<katla_physics::Joint>(*id))
        .unwrap();
    assert_eq!(joint.entity_a, linked.id());
    let trigger = instance
        .entities
        .iter()
        .find_map(|id| app.world.get_component::<crate::events::TriggerRules>(*id))
        .unwrap();
    assert_eq!(trigger.rules()[0].other_entity, Some(linked.id()));
    app.world.get_component_mut::<Link>(root).unwrap().target = outside;
    assert!(Prefab::capture(&mut app, root, &destination).is_err());
    assert!(app.world.get_component::<Parent>(root).is_some());
}

#[test]
#[ignore = "requires native Vulkan or Metal with API validation"]
fn test_native_mesh_asset_rebuilds_cpu_geometry_for_mesh_colliders() {
    let (mut app, dir) = setup();
    let mut document: MeshAsset = serde_json::from_value(mesh(&mut app)).unwrap();
    document.parts[0].geometry = Geometry::Cube { size: [1.0; 3] };
    write(
        &mut app,
        "assets/body.katmesh",
        serde_json::to_value(document).unwrap(),
    );
    let mut prefab = template();
    prefab.scene.entities[1].rigid_body = Some(crate::scene::RigidBodyDescriptor::new(
        katla_physics::BodyType::Kinematic,
    ));
    prefab.scene.entities[1].collider_shape =
        Some(crate::scene::ColliderShapeDescriptor::ConvexHull);
    let (x, y, z, w) = katla_math::Quat::from_axis_angle(katla_math::Vec3::Z_AXIS, 0.4).xyzw();
    prefab.scene.entities[1].transform.rotation = [x, y, z, w];
    let placement = TransformDescriptor {
        position: [4.0, 1.0, -2.0],
        scale: [2.0, 3.0, 4.0],
        ..TransformDescriptor::default_transform()
    };
    let instance = prefab
        .instantiate(&mut app, &dir.join("assets/object.katprefab"), placement)
        .unwrap();
    let (body, handle) = body(&app, instance.root);
    assert_eq!(app.geometry_cache.get(handle).unwrap().triangles.len(), 12);
    katla_ecs::System::update(
        &mut crate::systems::RapierPhysicsSystem,
        &mut app.world,
        1.0 / 60.0,
    );
    assert!(
        app.world
            .get_component::<katla_physics::RigidBody>(body)
            .unwrap()
            .body_handle
            .is_some()
    );
    let poses = crate::systems::resolve_world_transforms(&app.world);
    let rigid = app
        .world
        .get_component::<katla_physics::RigidBody>(body)
        .unwrap()
        .body_handle
        .unwrap();
    let physics = app
        .world
        .get_resource_mut::<katla_physics::PhysicsWorld>()
        .unwrap();
    assert!(
        (physics.body_transform(rigid).unwrap().position - poses[&body].transform.position)
            .length()
            < 1e-5
    );
    physics.step(1.0 / 60.0);
    let hit = physics
        .raycast(
            katla_math::Vec3::new(10.0, 1.0, -2.0),
            -katla_math::Vec3::X_AXIS,
            10.0,
        )
        .unwrap();
    assert_eq!(hit.entity, Some(body.id()));
    assert!(
        hit.point.x() > 4.8,
        "scaled collider must reach the rendered surface: {:?}",
        hit.point
    );
    app.world
        .get_component_mut::<TransformComponent>(instance.root)
        .unwrap()
        .transform
        .position = katla_math::Vec3::new(8.0, 1.0, -2.0);
    katla_ecs::System::update(
        &mut crate::systems::RapierPhysicsSystem,
        &mut app.world,
        1.0 / 60.0,
    );
    assert!(
        (app.world
            .get_resource::<katla_physics::PhysicsWorld>()
            .unwrap()
            .body_transform(rigid)
            .unwrap()
            .position
            .x()
            - 8.0)
            .abs()
            < 1e-5
    );
}

#[test]
#[ignore = "requires native Vulkan or Metal with API validation"]
fn test_native_removing_model_prefab_releases_owned_textures() {
    let (mut app, dir) = setup();
    let initial_textures = app.gpu_resource_tracker.texture_count();
    let mut prefab = template();
    prefab.scene.entities[1].source = EntitySource::GltfModel {
        path: AssetRef::File(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../resources/models/Avocado.glb")
                .canonicalize()
                .unwrap(),
        ),
    };
    let instance = prefab
        .instantiate(
            &mut app,
            &dir.join("assets/model.katprefab"),
            TransformDescriptor::default_transform(),
        )
        .unwrap();
    assert!(app.gpu_resource_tracker.texture_count() > initial_textures);
    remove_instance(&mut app, instance.root).unwrap();
    assert_eq!(app.gpu_resource_tracker.texture_count(), initial_textures);
    assert_eq!(app.gpu_resource_tracker.mesh_count(), 0);
}
