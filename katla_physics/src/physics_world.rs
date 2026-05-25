//! Physics world wrapper around Rapier3D.
//!
//! `PhysicsWorld` owns the Rapier physics pipeline and provides the primary
//! API for stepping simulation, creating/removing bodies, and performing
//! scene queries (raycasts, shape casts).

use katla_math::{Transform, Vec3};
use rapier3d::dynamics::{
    CCDSolver, ImpulseJointSet, IntegrationParameters, IslandManager, MultibodyJointSet,
    RigidBodyBuilder, RigidBodyHandle, RigidBodySet, RigidBodyType,
};
use rapier3d::geometry::{ColliderBuilder, ColliderHandle, ColliderSet, NarrowPhase};
use rapier3d::math::Vector;
use rapier3d::na;
use rapier3d::pipeline::PhysicsPipeline;
use rapier3d::prelude::*;

use crate::collider::ColliderShape;
use crate::material::PhysicsMaterial;
use crate::rigid_body::BodyType;
use crate::trigger::TriggerEvent;

/// Result of a raycast query.
#[derive(Debug, Clone)]
pub struct RayHit {
    /// Entity that was hit (stored as user data on the Rapier collider).
    pub entity: Option<u64>,
    /// World-space hit point.
    pub point: Vec3,
    /// World-space hit normal.
    pub normal: Vec3,
    /// Distance from ray origin to hit point.
    pub distance: f32,
}

/// Wrapper around Rapier3D physics state.
///
/// Owns the rigid body set, collider set, joints, and broad phase.
/// Game code interacts with physics through this type, never directly
/// with Rapier handles.
pub struct PhysicsWorld {
    gravity: Vector,
    integration_parameters: IntegrationParameters,
    pipeline: PhysicsPipeline,
    islands: IslandManager,
    broad_phase: DefaultBroadPhase,
    narrow_phase: NarrowPhase,
    bodies: RigidBodySet,
    colliders: ColliderSet,
    impulse_joints: ImpulseJointSet,
    multibody_joints: MultibodyJointSet,
    ccd_solver: CCDSolver,
    collision_events: Vec<TriggerEvent>,
}

impl PhysicsWorld {
    pub fn new() -> Self {
        let gravity = Vector::new(0.0, -9.81, 0.0);
        let integration_parameters = IntegrationParameters::default();
        let pipeline = PhysicsPipeline::new();
        let islands = IslandManager::new();
        let broad_phase = DefaultBroadPhase::new();
        let narrow_phase = NarrowPhase::new();
        let bodies = RigidBodySet::new();
        let colliders = ColliderSet::new();
        let impulse_joints = ImpulseJointSet::new();
        let multibody_joints = MultibodyJointSet::new();
        let ccd_solver = CCDSolver::new();

        Self {
            gravity,
            integration_parameters,
            pipeline,
            islands,
            broad_phase,
            narrow_phase,
            bodies,
            colliders,
            impulse_joints,
            multibody_joints,
            ccd_solver,
            collision_events: Vec::new(),
        }
    }

    /// Step the physics simulation forward by the given delta time.
    pub fn step(&mut self, dt: f32) {
        self.integration_parameters.dt = dt;
        self.collision_events.clear();

        let (collision_send, collision_recv) = std::sync::mpsc::channel();
        let (force_send, _force_recv) = std::sync::mpsc::channel();
        let event_handler =
            rapier3d::pipeline::ChannelEventCollector::new(collision_send, force_send);

        self.pipeline.step(
            self.gravity,
            &self.integration_parameters,
            &mut self.islands,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.ccd_solver,
            &(),
            &event_handler,
        );

        while let Ok(event) = collision_recv.try_recv() {
            match event {
                rapier3d::geometry::CollisionEvent::Started(h1, h2, _flags) => {
                    let entity1 = self.collider_entity(h1);
                    let entity2 = self.collider_entity(h2);
                    if let (Some(e1), Some(e2)) = (entity1, entity2) {
                        self.collision_events.push(TriggerEvent::Enter {
                            trigger_entity: e1,
                            other_entity: e2,
                        });
                    }
                }
                rapier3d::geometry::CollisionEvent::Stopped(h1, h2, _flags) => {
                    let entity1 = self.collider_entity(h1);
                    let entity2 = self.collider_entity(h2);
                    if let (Some(e1), Some(e2)) = (entity1, entity2) {
                        self.collision_events.push(TriggerEvent::Exit {
                            trigger_entity: e1,
                            other_entity: e2,
                        });
                    }
                }
            }
        }
    }

    /// Read the entity ID stored as user data on a collider.
    pub fn collider_entity(&self, handle: ColliderHandle) -> Option<u64> {
        let collider = self.colliders.get(handle)?;
        let id = collider.user_data as u64;
        if id != 0 { Some(id) } else { None }
    }

    /// Drain collision events from the last step.
    pub fn drain_collision_events(&mut self) -> Vec<TriggerEvent> {
        std::mem::take(&mut self.collision_events)
    }

    /// Create a dynamic rigid body with a collider at the given transform.
    ///
    /// Returns the (body_handle, collider_handle) pair. The `entity_id` is stored
    /// as user data on the collider for raycast entity lookup.
    pub fn create_dynamic_body(
        &mut self,
        shape: &ColliderShape,
        transform: &Transform,
        entity_id: u64,
    ) -> (RigidBodyHandle, ColliderHandle) {
        let pose = katla_to_rapier_pose(transform);
        let rapier_shape = collider_shape_to_rapier(shape);

        let body = RigidBodyBuilder::dynamic().pose(pose.into()).build();
        let body_handle = self.bodies.insert(body);

        let collider = ColliderBuilder::new(rapier_shape)
            .position(pose.into())
            .user_data(entity_id as u128)
            .build();
        let collider_handle =
            self.colliders
                .insert_with_parent(collider, body_handle, &mut self.bodies);

        (body_handle, collider_handle)
    }

    /// Create a static (immovable) collider at the given transform.
    pub fn create_static_collider(
        &mut self,
        shape: &ColliderShape,
        transform: &Transform,
        entity_id: u64,
    ) -> ColliderHandle {
        self.create_body(shape, transform, BodyType::Static, None, entity_id)
            .1
    }

    /// Create a rigid body with a collider.
    ///
    /// Returns (body_handle, collider_handle). For static bodies, the body handle
    /// is `RigidBodyHandle::invalid()` and the collider is standalone.
    ///
    /// If `is_sensor` is true, the collider is created as a sensor (no collision response,
    /// reports overlap events via `drain_collision_events`).
    pub fn create_body(
        &mut self,
        shape: &ColliderShape,
        transform: &Transform,
        body_type: BodyType,
        material: Option<&PhysicsMaterial>,
        entity_id: u64,
    ) -> (RigidBodyHandle, ColliderHandle) {
        self.create_body_ex(shape, transform, body_type, material, entity_id, false)
    }

    /// Extended version of `create_body` with sensor support.
    pub fn create_body_ex(
        &mut self,
        shape: &ColliderShape,
        transform: &Transform,
        body_type: BodyType,
        material: Option<&PhysicsMaterial>,
        entity_id: u64,
        is_sensor: bool,
    ) -> (RigidBodyHandle, ColliderHandle) {
        let pose = katla_to_rapier_pose(transform);
        let rapier_shape = collider_shape_to_rapier(shape);

        let mut collider_builder = ColliderBuilder::new(rapier_shape)
            .position(pose.into())
            .user_data(entity_id as u128);

        if let Some(mat) = material {
            collider_builder = collider_builder
                .friction(mat.friction)
                .restitution(mat.restitution)
                .density(mat.density);
        }

        if is_sensor {
            collider_builder = collider_builder
                .sensor(true)
                .active_events(rapier3d::pipeline::ActiveEvents::COLLISION_EVENTS);
        }

        let rapier_body_type = match body_type {
            BodyType::Static => RigidBodyType::Fixed,
            BodyType::Dynamic => RigidBodyType::Dynamic,
            BodyType::Kinematic => RigidBodyType::KinematicPositionBased,
        };

        if rapier_body_type == RigidBodyType::Fixed {
            let collider = collider_builder.build();
            let collider_handle = self.colliders.insert(collider);
            return (RigidBodyHandle::invalid(), collider_handle);
        }

        let body = RigidBodyBuilder::new(rapier_body_type)
            .pose(pose.into())
            .build();
        let body_handle = self.bodies.insert(body);

        let collider = collider_builder.build();
        let collider_handle =
            self.colliders
                .insert_with_parent(collider, body_handle, &mut self.bodies);

        (body_handle, collider_handle)
    }

    /// Remove a dynamic body and its attached collider.
    pub fn remove_body(&mut self, body: RigidBodyHandle, collider: ColliderHandle) {
        self.colliders
            .remove(collider, &mut self.islands, &mut self.bodies, true);
        self.bodies.remove(
            body,
            &mut self.islands,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            true,
        );
    }

    /// Remove a static collider.
    pub fn remove_static_collider(&mut self, collider: ColliderHandle) {
        self.colliders
            .remove(collider, &mut self.islands, &mut self.bodies, true);
    }

    /// Read the world-space position and rotation of a rigid body.
    pub fn body_transform(&self, body: RigidBodyHandle) -> Option<Transform> {
        let body = self.bodies.get(body)?;
        let pos = body.position();
        Some(rapier_pose_to_katla(pos))
    }

    /// Read the linear velocity of a rigid body.
    pub fn body_velocity(&self, body: RigidBodyHandle) -> Option<Vec3> {
        let body = self.bodies.get(body)?;
        let vel = body.linvel();
        Some(Vec3::new(vel.x, vel.y, vel.z))
    }

    /// Apply a force to a dynamic body at its center of mass.
    pub fn apply_force(&mut self, body: RigidBodyHandle, force: Vec3) {
        if let Some(b) = self.bodies.get_mut(body) {
            b.add_force(vec3_to_rapier(&force), true);
        }
    }

    /// Apply an impulse to a dynamic body at its center of mass.
    pub fn apply_impulse(&mut self, body: RigidBodyHandle, impulse: Vec3) {
        if let Some(b) = self.bodies.get_mut(body) {
            b.apply_impulse(vec3_to_rapier(&impulse), true);
        }
    }

    /// Create a joint between two rigid bodies.
    pub fn create_joint(
        &mut self,
        joint: &crate::joint::Joint,
        body_a: RigidBodyHandle,
        body_b: RigidBodyHandle,
    ) -> Option<ImpulseJointHandle> {
        use rapier3d::dynamics::{
            FixedJointBuilder, RevoluteJointBuilder, SphericalJointBuilder, SpringJointBuilder,
        };

        let anchor_a = Vector::new(joint.anchor_a[0], joint.anchor_a[1], joint.anchor_a[2]);
        let anchor_b = Vector::new(joint.anchor_b[0], joint.anchor_b[1], joint.anchor_b[2]);

        let generic: rapier3d::dynamics::GenericJoint = match joint.joint_type {
            crate::joint::JointType::PointToPoint => {
                let j = SphericalJointBuilder::new()
                    .local_anchor1(anchor_a)
                    .local_anchor2(anchor_b)
                    .build();
                j.into()
            }
            crate::joint::JointType::Hinge => {
                let mut builder = RevoluteJointBuilder::new(Vector::new(0.0, 1.0, 0.0))
                    .local_anchor1(anchor_a)
                    .local_anchor2(anchor_b);
                if let Some(limits) = &joint.limits {
                    builder = builder.limits([limits.min, limits.max]);
                }
                let j = builder.build();
                j.into()
            }
            crate::joint::JointType::Distance => {
                let limits = joint
                    .limits
                    .unwrap_or(crate::joint::JointLimits { min: 0.0, max: 1.0 });
                let j = SpringJointBuilder::new((limits.min + limits.max) * 0.5, 1.0, 0.5)
                    .local_anchor1(anchor_a)
                    .local_anchor2(anchor_b)
                    .build();
                j.into()
            }
            crate::joint::JointType::Fixed => {
                let j = FixedJointBuilder::new()
                    .local_anchor1(anchor_a)
                    .local_anchor2(anchor_b)
                    .build();
                j.into()
            }
        };

        let handle = self.impulse_joints.insert(body_a, body_b, generic, true);
        Some(handle)
    }

    /// Remove a joint by its handle.
    pub fn remove_joint(&mut self, handle: ImpulseJointHandle) {
        self.impulse_joints.remove(handle, true);
    }

    /// Cast a shape along a direction and return the first hit.
    pub fn shape_cast(
        &self,
        shape: &ColliderShape,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
    ) -> Option<RayHit> {
        let rapier_shape = collider_shape_to_rapier(shape);
        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            QueryFilter::default(),
        );

        let pose = rapier3d::math::Pose::translation(origin.x(), origin.y(), origin.z());
        let vel = vec3_to_rapier(&direction);
        let options =
            rapier3d::parry::query::ShapeCastOptions::with_max_time_of_impact(max_distance);

        let (collider_handle, hit) =
            query_pipeline.cast_shape(&pose, vel, rapier_shape.as_ref(), options)?;

        let collider = self.colliders.get(collider_handle)?;
        let hit_point = origin + direction * hit.time_of_impact;

        let entity = if collider.user_data != 0 {
            Some(collider.user_data as u64)
        } else {
            None
        };

        Some(RayHit {
            entity,
            point: hit_point,
            normal: Vec3::new(hit.normal1.x, hit.normal1.y, hit.normal1.z),
            distance: hit.time_of_impact,
        })
    }

    /// Cast a ray and return the first hit.
    pub fn raycast(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<RayHit> {
        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            QueryFilter::default(),
        );

        let ray = Ray::new(vec3_to_rapier(&origin), vec3_to_rapier(&direction));

        let (collider_handle, toi) = query_pipeline.cast_ray(&ray, max_distance, true)?;

        let collider = self.colliders.get(collider_handle)?;
        let hit_point = ray.origin + ray.dir * toi;

        let normal = query_pipeline.cast_ray_and_get_normal(&ray, max_distance, true);

        let entity = if collider.user_data != 0 {
            Some(collider.user_data as u64)
        } else {
            None
        };

        Some(RayHit {
            entity,
            point: Vec3::new(hit_point.x, hit_point.y, hit_point.z),
            normal: normal
                .map(|(_, n)| Vec3::new(n.normal.x, n.normal.y, n.normal.z))
                .unwrap_or(Vec3::new(0.0, 1.0, 0.0)),
            distance: toi,
        })
    }
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self::new()
    }
}

fn katla_to_rapier_pose(transform: &Transform) -> na::Isometry3<f32> {
    let q = transform.rotation;
    let (x, y, z, w) = q.xyzw();
    na::Isometry3::from_parts(
        na::Translation3::new(
            transform.position.x(),
            transform.position.y(),
            transform.position.z(),
        ),
        na::UnitQuaternion::new_normalize(na::Quaternion::new(w, x, y, z)),
    )
}

fn rapier_pose_to_katla(pose: &rapier3d::math::Pose) -> Transform {
    let t = pose.translation;
    let r = pose.rotation;
    Transform::from_position_rotation_scale(
        Vec3::new(t.x, t.y, t.z),
        katla_math::Quat::new(r.x, r.y, r.z, r.w),
        Vec3::new(1.0, 1.0, 1.0),
    )
}

fn vec3_to_rapier(v: &Vec3) -> Vector {
    Vector::new(v.x(), v.y(), v.z())
}

fn collider_shape_to_rapier(shape: &ColliderShape) -> rapier3d::geometry::SharedShape {
    match shape {
        ColliderShape::Sphere(s) => rapier3d::geometry::SharedShape::ball(s.radius),
        ColliderShape::Box(b) => {
            let he = b.half_extents_vec();
            rapier3d::geometry::SharedShape::cuboid(he.x(), he.y(), he.z())
        }
        ColliderShape::Capsule(c) => {
            rapier3d::geometry::SharedShape::capsule_y(c.half_height, c.radius)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::SphereShape;

    #[test]
    fn test_physics_world_creation() {
        let world = PhysicsWorld::new();
        assert_eq!(world.bodies.len(), 0);
        assert_eq!(world.colliders.len(), 0);
    }

    #[test]
    fn test_create_static_collider() {
        let mut world = PhysicsWorld::new();
        let shape = ColliderShape::Sphere(SphereShape::new(1.0));
        let transform = Transform::new_from_position(Vec3::new(0.0, 5.0, 0.0));
        let handle = world.create_static_collider(&shape, &transform, 42);
        assert_eq!(world.colliders.len(), 1);
        let collider = world.colliders.get(handle).unwrap();
        assert_eq!(collider.user_data as u64, 42);
    }

    #[test]
    fn test_create_dynamic_body() {
        let mut world = PhysicsWorld::new();
        let shape = ColliderShape::Sphere(SphereShape::new(0.5));
        let transform = Transform::new_from_position(Vec3::new(0.0, 10.0, 0.0));
        let (body, _collider) = world.create_dynamic_body(&shape, &transform, 1);
        assert_eq!(world.bodies.len(), 1);
        assert_eq!(world.colliders.len(), 1);
        assert!(world.bodies.get(body).unwrap().is_dynamic());
    }

    #[test]
    fn test_raycast_hit_static() {
        let mut world = PhysicsWorld::new();
        let shape = ColliderShape::Sphere(SphereShape::new(1.0));
        let transform = Transform::default();
        world.create_static_collider(&shape, &transform, 99);
        world.step(1.0 / 60.0);

        let hit = world.raycast(Vec3::new(0.0, 0.0, -5.0), Vec3::new(0.0, 0.0, 1.0), 10.0);
        assert!(hit.is_some());
        let hit = hit.unwrap();
        assert_eq!(hit.entity, Some(99));
        assert!(hit.distance > 3.0 && hit.distance < 5.0);
    }

    #[test]
    fn test_raycast_miss() {
        let mut world = PhysicsWorld::new();
        let shape = ColliderShape::Sphere(SphereShape::new(1.0));
        let transform = Transform::default();
        world.create_static_collider(&shape, &transform, 1);
        world.step(1.0 / 60.0);

        let hit = world.raycast(Vec3::new(10.0, 0.0, -5.0), Vec3::new(0.0, 0.0, 1.0), 10.0);
        assert!(hit.is_none());
    }

    #[test]
    fn test_gravity_simulation() {
        let mut world = PhysicsWorld::new();
        let shape = ColliderShape::Sphere(SphereShape::new(0.5));
        let transform = Transform::new_from_position(Vec3::new(0.0, 10.0, 0.0));
        let (body, _) = world.create_dynamic_body(&shape, &transform, 1);

        for _ in 0..60 {
            world.step(1.0 / 60.0);
        }

        let new_transform = world.body_transform(body).unwrap();
        assert!(
            new_transform.position.y() < 10.0,
            "Body should have fallen due to gravity"
        );
    }
}
