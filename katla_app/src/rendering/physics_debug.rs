//! Physics debug visualization.
//!
//! Generates draw calls for wireframe collider rendering, contact point
//! visualization, and raycast visualization. Uses thin cylinder and torus
//! meshes to approximate wireframe edges, following the same pattern as
//! the gizmo system.

use katla_gfx::primitives;
use katla_gfx::renderer::DrawCall;
use katla_gfx::{GpuRenderer, MaterialHandle, MeshHandle};
use katla_math::{Mat4, Quat, Vec3};

use crate::components::TransformComponent;

mod color {
    pub fn static_body() -> [f32; 4] {
        [0.3, 0.5, 1.0, 1.0]
    }
    pub fn dynamic() -> [f32; 4] {
        [0.2, 0.9, 0.3, 1.0]
    }
    pub fn kinematic() -> [f32; 4] {
        [1.0, 0.85, 0.1, 1.0]
    }
    pub fn trigger() -> [f32; 4] {
        [0.7, 0.3, 1.0, 1.0]
    }
    pub fn contact_normal() -> [f32; 4] {
        [1.0, 1.0, 0.2, 1.0]
    }
    pub fn contact_point() -> [f32; 4] {
        [1.0, 0.3, 0.1, 1.0]
    }
    pub fn ray_line() -> [f32; 4] {
        [0.0, 1.0, 1.0, 0.7]
    }
    pub fn ray_hit() -> [f32; 4] {
        [1.0, 0.0, 0.0, 1.0]
    }
}

/// GPU resources for physics debug rendering.
pub struct PhysicsDebugResources {
    pub shaft_mesh: MeshHandle,
    pub ring_mesh: MeshHandle,
    pub sphere_mesh: MeshHandle,
    pub material: MaterialHandle,
    pub initialized: bool,
}

impl Default for PhysicsDebugResources {
    fn default() -> Self {
        Self {
            shaft_mesh: MeshHandle::NONE,
            ring_mesh: MeshHandle::NONE,
            sphere_mesh: MeshHandle::NONE,
            material: MaterialHandle::NONE,
            initialized: false,
        }
    }
}

pub fn init_physics_debug_resources(
    renderer: &mut impl GpuRenderer,
    resources: &mut PhysicsDebugResources,
    unlit_material: MaterialHandle,
) {
    let shaft_mesh = primitives::create_cylinder(renderer, 1.0, 0.02, 8);
    let ring_mesh = primitives::create_torus(renderer, 1.0, 0.015, 64, 8);
    let sphere_mesh = primitives::create_sphere(renderer, 1.0, 8, 8);

    *resources = PhysicsDebugResources {
        shaft_mesh,
        ring_mesh,
        sphere_mesh,
        material: unlit_material,
        initialized: true,
    };
}

/// Generate wireframe draw calls for all collider entities in the world.
pub fn generate_collider_wireframe(
    world: &mut katla_ecs::World,
    resources: &PhysicsDebugResources,
    next_instance: &mut u32,
) -> Vec<DrawCall> {
    use katla_physics::{BodyType, ColliderShape, RigidBody, TriggerVolume};

    let entities: Vec<(katla_ecs::EntityId, ColliderShape)> = world
        .query::<&ColliderShape>()
        .map(|(entity, shape)| (entity, shape.clone()))
        .collect();

    let mut draws = Vec::new();

    for (entity, shape) in entities {
        let transform = world
            .get_component::<TransformComponent>(entity)
            .map(|t| t.transform)
            .unwrap_or_default();

        let body_type = world
            .get_component::<RigidBody>(entity)
            .map(|rb| rb.body_type)
            .unwrap_or(BodyType::Static);

        let is_trigger = world.get_component::<TriggerVolume>(entity).is_some();

        let color = if is_trigger {
            color::trigger()
        } else {
            match body_type {
                BodyType::Static => color::static_body(),
                BodyType::Dynamic => color::dynamic(),
                BodyType::Kinematic => color::kinematic(),
            }
        };

        let mat = transform.make_mat4();
        let pos = transform.position;

        match &shape {
            ColliderShape::Sphere(s) => {
                draw_sphere_wireframe(resources, pos, s.radius, color, next_instance, &mut draws);
            }
            ColliderShape::Box(b) => {
                let he = b.half_extents_vec();
                draw_box_wireframe(resources, &mat, he, color, next_instance, &mut draws);
            }
            ColliderShape::Capsule(c) => {
                draw_capsule_wireframe(
                    resources,
                    &mat,
                    c.half_height,
                    c.radius,
                    color,
                    next_instance,
                    &mut draws,
                );
            }
        }
    }

    draws
}

fn make_trs(translation: Vec3, rotation: Quat, scale: Vec3) -> Mat4 {
    Mat4::from_trs(translation, rotation, scale)
}

fn draw_sphere_wireframe(
    resources: &PhysicsDebugResources,
    center: Vec3,
    radius: f32,
    color: [f32; 4],
    next_instance: &mut u32,
    draws: &mut Vec<DrawCall>,
) {
    let r = radius.max(0.01);

    // XY ring (identity rotation)
    let idx = *next_instance;
    *next_instance += 1;
    draws.push(
        DrawCall::new(resources.ring_mesh, resources.material)
            .with_transform(make_trs(center, Quat::identity(), Vec3::new(r, r, r)).to_array())
            .with_color(color)
            .with_instance_index(idx),
    );

    // XZ ring (rotate 90 degrees around X)
    let rot_x = Quat::from_axis_angle(Vec3::X_AXIS, std::f32::consts::FRAC_PI_2);
    let idx = *next_instance;
    *next_instance += 1;
    draws.push(
        DrawCall::new(resources.ring_mesh, resources.material)
            .with_transform(make_trs(center, rot_x, Vec3::new(r, r, r)).to_array())
            .with_color(color)
            .with_instance_index(idx),
    );

    // YZ ring (rotate 90 degrees around Z)
    let rot_z = Quat::from_axis_angle(Vec3::Z_AXIS, std::f32::consts::FRAC_PI_2);
    let idx = *next_instance;
    *next_instance += 1;
    draws.push(
        DrawCall::new(resources.ring_mesh, resources.material)
            .with_transform(make_trs(center, rot_z, Vec3::new(r, r, r)).to_array())
            .with_color(color)
            .with_instance_index(idx),
    );
}

fn draw_box_wireframe(
    resources: &PhysicsDebugResources,
    world_transform: &Mat4,
    half_extents: Vec3,
    color: [f32; 4],
    next_instance: &mut u32,
    draws: &mut Vec<DrawCall>,
) {
    let hx = half_extents.x();
    let hy = half_extents.y();
    let hz = half_extents.z();

    let corners = [
        Vec3::new(-hx, -hy, -hz),
        Vec3::new(hx, -hy, -hz),
        Vec3::new(hx, -hy, hz),
        Vec3::new(-hx, -hy, hz),
        Vec3::new(-hx, hy, -hz),
        Vec3::new(hx, hy, -hz),
        Vec3::new(hx, hy, hz),
        Vec3::new(-hx, hy, hz),
    ];

    let edges: [(usize, usize); 12] = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ];

    for (a, b) in &edges {
        let p0 = *world_transform * corners[*a];
        let p1 = *world_transform * corners[*b];
        draw_edge(resources, p0, p1, color, next_instance, draws);
    }
}

fn draw_capsule_wireframe(
    resources: &PhysicsDebugResources,
    world_transform: &Mat4,
    half_height: f32,
    radius: f32,
    color: [f32; 4],
    next_instance: &mut u32,
    draws: &mut Vec<DrawCall>,
) {
    let r = radius.max(0.01);

    // Top ring
    let top_center = *world_transform * Vec3::new(0.0, half_height, 0.0);
    let idx = *next_instance;
    *next_instance += 1;
    draws.push(
        DrawCall::new(resources.ring_mesh, resources.material)
            .with_transform(make_trs(top_center, Quat::identity(), Vec3::new(r, r, r)).to_array())
            .with_color(color)
            .with_instance_index(idx),
    );

    // Bottom ring
    let bottom_center = *world_transform * Vec3::new(0.0, -half_height, 0.0);
    let idx = *next_instance;
    *next_instance += 1;
    draws.push(
        DrawCall::new(resources.ring_mesh, resources.material)
            .with_transform(
                make_trs(bottom_center, Quat::identity(), Vec3::new(r, r, r)).to_array(),
            )
            .with_color(color)
            .with_instance_index(idx),
    );

    // 4 vertical edges connecting top and bottom rings
    let angles = [
        0.0,
        std::f32::consts::FRAC_PI_2,
        std::f32::consts::PI,
        std::f32::consts::FRAC_PI_2 * 3.0,
    ];
    for &angle in &angles {
        let x = radius * angle.cos();
        let z = radius * angle.sin();
        let p0 = *world_transform * Vec3::new(x, -half_height, z);
        let p1 = *world_transform * Vec3::new(x, half_height, z);
        draw_edge(resources, p0, p1, color, next_instance, draws);
    }
}

fn draw_edge(
    resources: &PhysicsDebugResources,
    p0: Vec3,
    p1: Vec3,
    color: [f32; 4],
    next_instance: &mut u32,
    draws: &mut Vec<DrawCall>,
) {
    let mid = (p0 + p1) * 0.5;
    let diff = p1 - p0;
    let length = diff.length();
    if length < 1e-6 {
        return;
    }

    let dir = diff * (1.0 / length);
    let up = Vec3::new(0.0, 1.0, 0.0);
    let rot = Quat::from_rotation_between(up, dir);

    let idx = *next_instance;
    *next_instance += 1;
    draws.push(
        DrawCall::new(resources.shaft_mesh, resources.material)
            .with_transform(make_trs(mid, rot, Vec3::new(1.0, length, 1.0)).to_array())
            .with_color(color)
            .with_instance_index(idx),
    );
}

/// Generate draw calls for raycast visualization.
pub fn generate_raycast_vis(
    resources: &PhysicsDebugResources,
    origin: Vec3,
    hit: Option<&katla_physics::RayHit>,
    max_distance: f32,
    direction: Vec3,
    next_instance: &mut u32,
) -> Vec<DrawCall> {
    let mut draws = Vec::new();

    let end = match hit {
        Some(h) => h.point,
        None => origin + direction * max_distance,
    };
    draw_edge(
        resources,
        origin,
        end,
        color::ray_line(),
        next_instance,
        &mut draws,
    );

    if let Some(h) = hit {
        let r = 0.05;
        let idx = *next_instance;
        *next_instance += 1;
        draws.push(
            DrawCall::new(resources.sphere_mesh, resources.material)
                .with_transform(make_trs(h.point, Quat::identity(), Vec3::new(r, r, r)).to_array())
                .with_color(color::ray_hit())
                .with_instance_index(idx),
        );

        let normal_end = h.point + h.normal * 0.3;
        draw_edge(
            resources,
            h.point,
            normal_end,
            color::contact_normal(),
            next_instance,
            &mut draws,
        );
    }

    draws
}

/// Generate draw calls for contact point visualization.
///
/// Renders small spheres at each contact point and lines showing the contact normal.
pub fn generate_contact_vis(
    resources: &PhysicsDebugResources,
    physics_world: &katla_physics::PhysicsWorld,
    next_instance: &mut u32,
) -> Vec<DrawCall> {
    let contacts = physics_world.active_contacts();
    let mut draws = Vec::new();

    for (_e1, _e2, point, normal, _depth) in &contacts {
        // Contact point sphere
        let r = 0.03;
        let idx = *next_instance;
        *next_instance += 1;
        draws.push(
            DrawCall::new(resources.sphere_mesh, resources.material)
                .with_transform(make_trs(*point, Quat::identity(), Vec3::new(r, r, r)).to_array())
                .with_color(color::contact_point())
                .with_instance_index(idx),
        );

        // Contact normal line
        let normal_end = *point + *normal * 0.2;
        draw_edge(
            resources,
            *point,
            normal_end,
            color::contact_normal(),
            next_instance,
            &mut draws,
        );
    }

    draws
}
