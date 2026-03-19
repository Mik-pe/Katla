use katla_ecs::{System, World};

use crate::components::{PointLight, TransformComponent};
use katla_math::Vec3;

const INFINITY_RADIUS_X: f32 = 6.0;
const INFINITY_RADIUS_Z: f32 = 4.0;
const ORBIT_SPEED: f32 = 0.4;
const BASE_HEIGHT: f32 = 3.0;

/// Moves all point lights along a lemniscate (infinity-8) path, staggered evenly.
pub struct LightOrbitSystem {
    elapsed: f32,
}

impl LightOrbitSystem {
    pub fn new() -> Self {
        Self { elapsed: 0.0 }
    }

    fn lemniscate(&self, t: f32) -> Vec3 {
        let sin_t = t.sin();
        let cos_t = t.cos();
        let denom = 1.0 + sin_t * sin_t;
        Vec3::new(
            INFINITY_RADIUS_X * cos_t / denom,
            BASE_HEIGHT,
            INFINITY_RADIUS_Z * sin_t * cos_t / denom,
        )
    }
}

impl System for LightOrbitSystem {
    fn update(&mut self, world: &mut World, delta_time: f32) {
        self.elapsed += delta_time;
        let t = self.elapsed * ORBIT_SPEED;

        // First pass: count lights
        let mut light_count = 0usize;
        for _ in world.query::<&PointLight>() {
            light_count += 1;
        }

        if light_count == 0 {
            return;
        }

        let step = std::f32::consts::TAU / light_count as f32;

        // Second pass: update positions
        for (i, (_entity, _point_light, transform)) in
            world.query::<(&PointLight, &mut TransformComponent)>().enumerate()
        {
            transform.transform.position = self.lemniscate(t + i as f32 * step);
        }
    }
}
