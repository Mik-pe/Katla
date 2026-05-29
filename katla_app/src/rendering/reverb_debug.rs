//! Reverb zone debug visualization.
//!
//! Generates wireframe box draw calls for ReverbZone components, color-coded
//! by decay and wet parameters. Reuses PhysicsDebugResources for mesh/material
//! handles since the wireframe primitives are identical.

use katla_gfx::renderer::DrawCall;
use katla_math::{Mat4, Vec3};

use crate::components::TransformComponent;

mod color {
    pub fn reverb_zone(decay: f32, wet: f32) -> [f32; 4] {
        let blue = decay.clamp(0.0, 1.0);
        let red = wet.clamp(0.0, 1.0);
        let green = 0.15 + 0.15 * (1.0 - blue).min(1.0 - red);
        [red * 0.6 + 0.2, green, blue * 0.7 + 0.3, 0.85]
    }
}

pub fn generate_reverb_zone_wireframe(
    world: &mut katla_ecs::World,
    resources: &crate::rendering::physics_debug::PhysicsDebugResources,
    next_instance: &mut u32,
) -> Vec<DrawCall> {
    use crate::components::audio::ReverbZone;

    let zones: Vec<(katla_ecs::EntityId, ReverbZone)> = world
        .query::<&ReverbZone>()
        .map(|(entity, zone)| (entity, zone.clone()))
        .collect();

    let mut draws = Vec::new();

    for (entity, zone) in zones {
        let transform = world
            .get_component::<TransformComponent>(entity)
            .map(|t| t.transform)
            .unwrap_or_default();

        let color = color::reverb_zone(zone.decay, zone.wet);
        let mat = transform.make_mat4();
        let he = Vec3::new(
            zone.half_extents[0],
            zone.half_extents[1],
            zone.half_extents[2],
        );
        draw_reverb_box(resources, &mat, he, color, next_instance, &mut draws);
    }

    draws
}

fn draw_reverb_box(
    resources: &crate::rendering::physics_debug::PhysicsDebugResources,
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
        crate::rendering::physics_debug::draw_edge(resources, p0, p1, color, next_instance, draws);
    }
}
