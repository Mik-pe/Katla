//! Rapier owns native bodies and intersections; scene policy remains in Odin.
mod joints;
use rapier3d::prelude::*;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum BodyType {
    Dynamic,
    Kinematic,
    Fixed,
}
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Shape {
    None,
    Box {
        half_extents: [f32; 3],
    },
    Sphere {
        radius: f32,
    },
    Capsule {
        half_height: f32,
        radius: f32,
    },
    Trimesh {
        vertices: Vec<[f32; 3]>,
        indices: Vec<[u32; 3]>,
    },
    ConvexHull {
        vertices: Vec<[f32; 3]>,
    },
}
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Body {
    entity_id: String,
    body_type: BodyType,
    position: [f32; 3],
    rotation: [f32; 4],
    shape: Shape,
    sensor: bool,
    linear_velocity: [f32; 3],
    gravity_scale: f32,
    #[serde(default = "default_density")]
    density: f32,
    friction: f32,
    restitution: f32,
    #[serde(default)]
    ccd: bool,
    #[serde(default = "all_layers")]
    layers: u32,
    #[serde(default = "all_layers")]
    mask: u32,
}
fn default_density() -> f32 {
    1.0
}
fn all_layers() -> u32 {
    u32::MAX
}
#[derive(Deserialize)]
#[serde(tag = "method", deny_unknown_fields)]
enum Request {
    #[serde(rename = "physics_sync")]
    Sync {
        bodies: Vec<Body>,
        #[serde(default)]
        joints: Vec<joints::Joint>,
    },
    #[serde(rename = "physics_step")]
    Step { delta_seconds: f32 },
    #[serde(rename = "physics_reset")]
    Reset,
}
struct Entry {
    authored: Body,
    body: RigidBodyHandle,
    collider: Option<ColliderHandle>,
}
pub struct Physics {
    entries: BTreeMap<u64, Entry>,
    joints: BTreeMap<u64, joints::Owned>,
    overlaps: BTreeSet<(u64, u64)>,
    bodies: RigidBodySet,
    colliders: ColliderSet,
    pipeline: PhysicsPipeline,
    islands: IslandManager,
    broad: BroadPhaseBvh,
    narrow: NarrowPhase,
    impulses: ImpulseJointSet,
    multibodies: MultibodyJointSet,
    ccd: CCDSolver,
}
impl Physics {
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            joints: BTreeMap::new(),
            overlaps: BTreeSet::new(),
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            pipeline: PhysicsPipeline::new(),
            islands: IslandManager::new(),
            broad: BroadPhaseBvh::new(),
            narrow: NarrowPhase::new(),
            impulses: ImpulseJointSet::new(),
            multibodies: MultibodyJointSet::new(),
            ccd: CCDSolver::new(),
        }
    }
    pub fn call(&mut self, value: Value) -> Result<Value, String> {
        let request: Request = serde_json::from_value(value).map_err(|e| e.to_string())?;
        match request {
            Request::Sync { bodies, joints } => self.sync(bodies, joints),
            Request::Step { delta_seconds } => self.step(delta_seconds),
            Request::Reset => {
                *self = Self::new();
                Ok(json!({}))
            }
        }
    }
    fn sync(&mut self, input: Vec<Body>, joint_input: Vec<joints::Joint>) -> Result<Value, String> {
        if input.len() > 100_000 {
            return Err("Physics body budget exceeded".into());
        }
        let mut staged = BTreeMap::new();
        for authored in input {
            let id = validate(&authored)?;
            let prepared = shape(&authored.shape)?;
            if staged.insert(id, (authored, prepared)).is_some() {
                return Err("Duplicate physics entity".into());
            }
        }
        for entry in self.entries.values() {
            if !self.bodies.contains(entry.body)
                || entry
                    .collider
                    .is_some_and(|handle| !self.colliders.contains(handle))
            {
                return Err("Missing native physics owner".into());
            }
        }
        let staged_joints = joints::prepare(joint_input, &staged)?;
        joints::validate_owners(&self.joints, &self.impulses)?;
        joints::remove_missing(&mut self.joints, &mut self.impulses, &staged_joints);
        let removed: Vec<_> = self
            .entries
            .keys()
            .filter(|id| !staged.contains_key(id))
            .copied()
            .collect();
        for id in removed {
            if let Some(entry) = self.entries.remove(&id) {
                self.bodies.remove(
                    entry.body,
                    &mut self.islands,
                    &mut self.colliders,
                    &mut self.impulses,
                    &mut self.multibodies,
                    true,
                );
            }
        }
        for (id, (authored, prepared)) in staged {
            if let Some(entry) = self.entries.get_mut(&id) {
                let old = &entry.authored;
                let body = self
                    .bodies
                    .get_mut(entry.body)
                    .ok_or("Missing native body")?;
                if old.body_type != authored.body_type {
                    body.set_body_type(body_type(authored.body_type), true);
                }
                if old.position != authored.position || old.rotation != authored.rotation {
                    body.set_position(pose(&authored), true);
                    if authored.body_type == BodyType::Kinematic {
                        body.set_next_kinematic_position(pose(&authored));
                    }
                }
                if old.linear_velocity != authored.linear_velocity {
                    body.set_linvel(Vector::from_array(authored.linear_velocity), true);
                }
                if old.gravity_scale != authored.gravity_scale {
                    body.set_gravity_scale(authored.gravity_scale, true);
                }
                if old.ccd != authored.ccd {
                    body.enable_ccd(authored.ccd);
                }
                if old.shape != authored.shape {
                    if let Some(handle) = entry.collider.take() {
                        self.colliders
                            .remove(handle, &mut self.islands, &mut self.bodies, true);
                    }
                    if let Some(prepared) = prepared {
                        entry.collider = Some(self.colliders.insert_with_parent(
                            collider_builder(&authored, prepared, id).build(),
                            entry.body,
                            &mut self.bodies,
                        ));
                    }
                } else if let Some(handle) = entry.collider {
                    let collider = self
                        .colliders
                        .get_mut(handle)
                        .ok_or("Missing native collider")?;
                    if old.sensor != authored.sensor {
                        collider.set_sensor(authored.sensor);
                    }
                    if old.density != authored.density {
                        collider.set_density(authored.density);
                    }
                    if old.friction != authored.friction {
                        collider.set_friction(authored.friction);
                    }
                    if old.restitution != authored.restitution {
                        collider.set_restitution(authored.restitution);
                    }
                    if old.layers != authored.layers || old.mask != authored.mask {
                        collider.set_collision_groups(groups(&authored));
                    }
                }
                entry.authored = authored;
            } else {
                let body = self.bodies.insert(
                    RigidBodyBuilder::new(body_type(authored.body_type))
                        .pose(pose(&authored))
                        .linvel(Vector::from_array(authored.linear_velocity))
                        .gravity_scale(authored.gravity_scale)
                        .ccd_enabled(authored.ccd)
                        .user_data(id as u128)
                        .build(),
                );
                let collider = prepared.map(|prepared| {
                    self.colliders.insert_with_parent(
                        collider_builder(&authored, prepared, id).build(),
                        body,
                        &mut self.bodies,
                    )
                });
                self.entries.insert(
                    id,
                    Entry {
                        authored,
                        body,
                        collider,
                    },
                );
            }
        }
        joints::publish(
            &mut self.joints,
            &mut self.impulses,
            &self.entries,
            staged_joints,
        );
        Ok(json!({"body_count": self.entries.len(),"joint_count":self.joints.len()}))
    }
    fn step(&mut self, delta: f32) -> Result<Value, String> {
        if !delta.is_finite() || delta <= 0.0 || delta > 0.25 {
            return Err("Physics delta must be in (0, 0.25]".into());
        }
        let params = IntegrationParameters {
            dt: delta,
            ..Default::default()
        };
        self.pipeline.step(
            Vector::new(0.0, -9.81, 0.0),
            &params,
            &mut self.islands,
            &mut self.broad,
            &mut self.narrow,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulses,
            &mut self.multibodies,
            &mut self.ccd,
            &(),
            &(),
        );
        let mut overlaps = BTreeSet::new();
        for (a, b, intersecting) in self.narrow.intersection_pairs() {
            if !intersecting {
                continue;
            }
            let (Some(a), Some(b)) = (self.colliders.get(a), self.colliders.get(b)) else {
                continue;
            };
            if a.is_sensor() {
                overlaps.insert((a.user_data as u64, b.user_data as u64));
            }
            if b.is_sensor() {
                overlaps.insert((b.user_data as u64, a.user_data as u64));
            }
        }
        let mut events: Vec<Value> = overlaps.difference(&self.overlaps).map(|&(a,b)|
            json!({"phase":"enter","trigger_entity":a.to_string(),"other_entity":b.to_string()})).collect();
        events.extend(self.overlaps.difference(&overlaps).map(|&(a,b)|
            json!({"phase":"exit","trigger_entity":a.to_string(),"other_entity":b.to_string()})));
        let active: Vec<_> = overlaps
            .iter()
            .map(|&(a, b)| json!({"trigger_entity":a.to_string(),"other_entity":b.to_string()}))
            .collect();
        self.overlaps = overlaps;
        let mut poses = Vec::with_capacity(self.entries.len());
        for (&id, entry) in &self.entries {
            let body = self.bodies.get(entry.body).ok_or("Missing native body")?;
            let p = body.position();
            poses.push(json!({"entity_id":id.to_string(),"position":p.translation.to_array(),
                "rotation":[p.rotation.x,p.rotation.y,p.rotation.z,p.rotation.w],"linear_velocity":body.linvel().to_array()}));
        }
        Ok(json!({"poses":poses,"events":events,"overlaps":active}))
    }
}
fn validate(body: &Body) -> Result<u64, String> {
    if body.entity_id.is_empty()
        || (body.entity_id.len() > 1 && body.entity_id.starts_with('0'))
        || !body.entity_id.bytes().all(|b| b.is_ascii_digit())
    {
        return Err("Physics entity ID must be canonical decimal u64".into());
    }
    let id = body
        .entity_id
        .parse::<u64>()
        .map_err(|_| "Physics entity ID overflow")?;
    let finite = |v: f32| v.is_finite() && v.abs() <= 1.0e8;
    if !body
        .position
        .iter()
        .chain(&body.rotation)
        .chain(&body.linear_velocity)
        .all(|&v| finite(v))
        || !finite(body.gravity_scale)
        || !finite(body.density)
        || body.density < 0.0
        || !finite(body.friction)
        || !finite(body.restitution)
        || body.friction < 0.0
        || !(0.0..=1.0).contains(&body.restitution)
        || (body.rotation.iter().map(|v| v * v).sum::<f32>() - 1.0).abs() > 1.0e-4
    {
        return Err("Invalid physics transform, velocity or material".into());
    }
    let positive = |v: f32| finite(v) && v > 0.0;
    let valid_vertices = |vertices: &[[f32; 3]], minimum: usize| {
        vertices.len() >= minimum
            && vertices.len() <= 1_000_000
            && vertices.iter().flatten().all(|&v| finite(v))
    };
    let valid_shape = match &body.shape {
        Shape::None => true,
        Shape::Box { half_extents } => half_extents.iter().all(|&v| positive(v)),
        Shape::Sphere { radius } => positive(*radius),
        Shape::Capsule {
            half_height,
            radius,
        } => finite(*half_height) && *half_height >= 0.0 && positive(*radius),
        Shape::Trimesh { vertices, indices } => {
            valid_vertices(vertices, 3)
                && !indices.is_empty()
                && indices.len() <= 1_000_000
                && indices.iter().all(|t| {
                    t.iter().all(|&i| (i as usize) < vertices.len())
                        && t[0] != t[1]
                        && t[1] != t[2]
                        && t[0] != t[2]
                })
        }
        Shape::ConvexHull { vertices } => valid_vertices(vertices, 4),
    };
    if !valid_shape {
        return Err("Invalid collider dimensions".into());
    }
    Ok(id)
}
fn pose(body: &Body) -> Pose {
    Pose::from_parts(
        Vector::from_array(body.position),
        Rotation::from_xyzw(
            body.rotation[0],
            body.rotation[1],
            body.rotation[2],
            body.rotation[3],
        )
        .normalize(),
    )
}
fn body_type(kind: BodyType) -> RigidBodyType {
    match kind {
        BodyType::Dynamic => RigidBodyType::Dynamic,
        BodyType::Kinematic => RigidBodyType::KinematicPositionBased,
        BodyType::Fixed => RigidBodyType::Fixed,
    }
}
fn shape(shape: &Shape) -> Result<Option<SharedShape>, String> {
    if matches!(shape, Shape::None) {
        return Ok(None);
    }
    Ok(Some(match shape {
        Shape::None => return Ok(None),
        Shape::Box {
            half_extents: [x, y, z],
        } => SharedShape::cuboid(*x, *y, *z),
        Shape::Sphere { radius } => SharedShape::ball(*radius),
        Shape::Capsule {
            half_height,
            radius,
        } => SharedShape::capsule_y(*half_height, *radius),
        Shape::Trimesh { vertices, indices } => SharedShape::trimesh(
            vertices.iter().copied().map(Vector::from_array).collect(),
            indices.clone(),
        )
        .map_err(|e| format!("Invalid triangle mesh: {e}"))?,
        Shape::ConvexHull { vertices } => SharedShape::convex_hull(
            &vertices
                .iter()
                .copied()
                .map(Vector::from_array)
                .collect::<Vec<_>>(),
        )
        .ok_or("Invalid convex hull")?,
    }))
}
fn collider_builder(body: &Body, shape: SharedShape, id: u64) -> ColliderBuilder {
    ColliderBuilder::new(shape)
        .sensor(body.sensor)
        .active_collision_types(ActiveCollisionTypes::all())
        .density(body.density)
        .friction(body.friction)
        .restitution(body.restitution)
        .collision_groups(groups(body))
        .user_data(id as u128)
}
fn groups(body: &Body) -> InteractionGroups {
    InteractionGroups::new(
        Group::from_bits_retain(body.layers),
        Group::from_bits_retain(body.mask),
        InteractionTestMode::And,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn body(id: &str, kind: &str, sensor: bool, position: [f32; 3]) -> Value {
        json!({"entity_id":id,"body_type":kind,"position":position,"rotation":[0,0,0,1],
            "shape":{"kind":"sphere","radius":0.5},"sensor":sensor,"linear_velocity":[0,0,0],
            "gravity_scale":1,"friction":0.5,"restitution":0})
    }
    fn sync(p: &mut Physics, bodies: Vec<Value>) -> Result<Value, String> {
        p.call(json!({"method":"physics_sync","bodies":bodies}))
    }
    fn step(p: &mut Physics) -> Value {
        p.call(json!({"method":"physics_step","delta_seconds":1.0/60.0}))
            .expect("step")
    }
    #[test]
    fn test_native_fall_contact_and_unchanged_sync_preserves_motion() {
        let mut p = Physics::new();
        let mut ground = body("1", "fixed", false, [0.0, -0.5, 0.0]);
        ground["shape"] = json!({"kind":"box","half_extents":[10,0.5,10]});
        let ball = body("18446744073709551615", "dynamic", false, [0.0, 3.0, 0.0]);
        sync(&mut p, vec![ground.clone(), ball.clone()]).expect("sync");
        for _ in 0..30 {
            step(&mut p);
        }
        let prior = step(&mut p);
        sync(&mut p, vec![ground, ball]).expect("unchanged sync");
        let next = step(&mut p);
        assert!(
            next["poses"][1]["position"][1].as_f64() < prior["poses"][1]["position"][1].as_f64()
        );
        for _ in 0..180 {
            step(&mut p);
        }
        let settled = step(&mut p);
        let y = settled["poses"][1]["position"][1].as_f64().expect("height");
        assert!((y - 0.5).abs() < 0.03, "native contact height {y}");
    }
    #[test]
    fn test_sensor_pair_directions_repeat_and_deletion_exit() {
        let mut p = Physics::new();
        let a = body("2", "fixed", true, [0.0, 0.0, 0.0]);
        let b = body("3", "fixed", true, [0.5, 0.0, 0.0]);
        sync(&mut p, vec![a.clone(), b]).expect("sync");
        assert_eq!(step(&mut p)["events"].as_array().expect("events").len(), 2);
        assert_eq!(step(&mut p)["events"], json!([]));
        sync(&mut p, vec![a]).expect("remove");
        let result = step(&mut p);
        assert_eq!(result["events"].as_array().expect("events").len(), 2);
        assert_eq!(result["events"][0]["phase"], "exit");
        assert_eq!(result["overlaps"], json!([]));
    }
    #[test]
    fn test_atomic_rejection_filters_teleport_and_reset() {
        let mut p = Physics::new();
        let mut a = body("1", "fixed", true, [0.0, 0.0, 0.0]);
        let b = body("2", "fixed", false, [0.1, 0.0, 0.0]);
        sync(&mut p, vec![a.clone(), b.clone()]).expect("sync");
        let mut invalid = b.clone();
        invalid["rotation"] = json!([0, 0, 0, 0]);
        assert!(sync(&mut p, vec![invalid]).is_err());
        assert_eq!(p.entries.len(), 2);
        assert_eq!(
            step(&mut p)["overlaps"].as_array().expect("overlaps").len(),
            1
        );
        a["mask"] = json!(0);
        sync(&mut p, vec![a.clone(), b.clone()]).expect("filter");
        assert_eq!(step(&mut p)["events"][0]["phase"], "exit");
        a["mask"] = json!(u32::MAX);
        a["position"] = json!([5, 0, 0]);
        sync(&mut p, vec![a, b]).expect("teleport");
        assert_eq!(step(&mut p)["overlaps"], json!([]));
        p.call(json!({"method":"physics_reset"})).expect("reset");
        assert_eq!(step(&mut p)["poses"], json!([]));
        assert!(
            p.call(json!({"method":"physics_step","delta_seconds":0}))
                .is_err()
        );
    }
    #[test]
    fn test_triangle_mesh_contact_hull_and_whole_batch_preparation() {
        let mut p = Physics::new();
        let mut ground = body("1", "fixed", false, [0.0, 0.0, 0.0]);
        ground["shape"] = json!({"kind":"trimesh", "vertices":[[-10,0,-10],[-10,0,10],[10,0,10],[10,0,-10]],"indices":[[0,1,2],[0,2,3]]});
        let mut cube = body("2", "dynamic", false, [0.0, 3.0, 0.0]);
        cube["shape"] = json!({"kind":"convex_hull", "vertices":[[-0.5,-0.5,-0.5],[-0.5,-0.5,0.5],[-0.5,0.5,-0.5],[-0.5,0.5,0.5],[0.5,-0.5,-0.5],[0.5,-0.5,0.5],[0.5,0.5,-0.5],[0.5,0.5,0.5]]});
        cube["density"] = json!(2.5);
        sync(&mut p, vec![ground.clone(), cube.clone()]).expect("mesh and hull");
        for _ in 0..240 {
            step(&mut p);
        }
        let settled = step(&mut p);
        let y = settled["poses"][1]["position"][1].as_f64().expect("height");
        assert!((y - 0.5).abs() < 0.04, "actual hull on triangle mesh {y}");
        let handles: Vec<_> = p.entries.values().map(|e| (e.body, e.collider)).collect();
        let mut malformed = cube.clone();
        malformed["shape"] =
            json!({"kind":"convex_hull", "vertices":[[0,0,0],[1,0,0],[2,0,0],[3,0,0]]});
        assert!(sync(&mut p, vec![malformed]).is_err());
        assert_eq!(
            handles,
            p.entries
                .values()
                .map(|e| (e.body, e.collider))
                .collect::<Vec<_>>()
        );
        let mut invalid_mesh = ground;
        invalid_mesh["shape"]["indices"] = json!([[0, 1, 99]]);
        assert!(sync(&mut p, vec![invalid_mesh, cube]).is_err());
        assert_eq!(p.entries.len(), 2);
    }
    #[test]
    fn test_native_body_without_collider_and_shape_transitions_keep_body_owner() {
        let mut p = Physics::new();
        let mut authored = body("1", "dynamic", false, [0.0, 2.0, 0.0]);
        authored["shape"] = json!({"kind":"none"});
        authored["linear_velocity"] = json!([2, 0, 0]);
        sync(&mut p, vec![authored.clone()]).expect("body without collider");
        let handle = p.entries[&1].body;
        assert!(p.entries[&1].collider.is_none());
        assert_eq!(p.colliders.len(), 0);
        assert_eq!(step(&mut p)["poses"][0]["entity_id"], "1");
        authored["shape"] = json!({"kind":"sphere","radius":0.5});
        sync(&mut p, vec![authored.clone()]).expect("attach collider");
        assert_eq!(p.entries[&1].body, handle);
        assert!(p.entries[&1].collider.is_some());
        let first = step(&mut p);
        authored["shape"] = json!({"kind":"none"});
        sync(&mut p, vec![authored]).expect("remove collider");
        assert_eq!(p.entries[&1].body, handle);
        assert!(p.entries[&1].collider.is_none());
        assert_eq!(p.colliders.len(), 0);
        let next = step(&mut p);
        assert_eq!(
            next["poses"][0]["linear_velocity"][0],
            first["poses"][0]["linear_velocity"][0]
        );
        assert_eq!(next["overlaps"], json!([]));
    }
    #[test]
    fn test_native_fixed_joint_preserves_ownership_and_rejects_whole_batch() {
        let mut p = Physics::new();
        let mut a = body("1", "dynamic", false, [0.0, 3.0, 0.0]);
        a["gravity_scale"] = json!(0);
        let b = body("2", "dynamic", false, [2.0, 3.0, 0.0]);
        let joint = json!({"entity_id":"18446744073709551615","kind":"fixed","a":"1","b":"2","anchor_a":[1,0,0],"anchor_b":[-1,0,0],"limits":null});
        let sync_joint = |p: &mut Physics, bodies: Vec<Value>, joints: Vec<Value>| {
            p.call(json!({"method":"physics_sync","bodies":bodies,"joints":joints}))
        };
        sync_joint(&mut p, vec![a.clone(), b.clone()], vec![joint.clone()])
            .expect("native constraint");
        let handle = p.impulses.iter().next().expect("joint").0;
        for _ in 0..120 {
            step(&mut p);
            sync_joint(&mut p, vec![a.clone(), b.clone()], vec![joint.clone()])
                .expect("unchanged sync");
            assert!(p.impulses.contains(handle));
        }
        let pose_a = p.bodies[p.entries[&1].body].position();
        let pose_b = p.bodies[p.entries[&2].body].position();
        let anchor_a = pose_a.translation + pose_a.rotation * Vector::new(1.0, 0.0, 0.0);
        let anchor_b = pose_b.translation + pose_b.rotation * Vector::new(-1.0, 0.0, 0.0);
        assert!(
            (anchor_a - anchor_b).length() < 0.02,
            "actual fixed joint anchors {anchor_a}/{anchor_b}"
        );
        assert!(pose_a.rotation.dot(pose_b.rotation).abs() > 0.999);
        assert!(((pose_a.translation - pose_b.translation).length() - 2.0).abs() < 0.02);
        let mut invalid = joint.clone();
        invalid["b"] = json!("99");
        assert!(sync_joint(&mut p, vec![a.clone()], vec![invalid]).is_err());
        assert_eq!(p.entries.len(), 2);
        assert!(p.impulses.contains(handle));
        for kind in ["point_to_point", "hinge", "distance", "fixed"] {
            let mut variant = joint.clone();
            variant["kind"] = json!(kind);
            variant["limits"] = json!([0.25, 0.75]);
            sync_joint(&mut p, vec![a.clone(), b.clone()], vec![variant])
                .expect("actual native joint variant");
            assert_eq!(p.impulses.len(), 1);
            step(&mut p);
        }
        sync_joint(&mut p, vec![a, b], vec![]).expect("remove joint");
        assert_eq!(p.impulses.len(), 0);
        assert!(p.joints.is_empty());
    }
}
