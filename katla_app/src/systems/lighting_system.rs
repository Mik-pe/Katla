use katla_ecs::{System, World};
use crate::components::{DirectionalLight, PointLight, SpotLight};

/// Collection of all active lights in the scene.
///
/// This resource is updated every frame by the LightingSystem and used by the
/// rendering system to pass light data to shaders.
#[derive(Debug, Clone)]
pub struct LightCollection {
    /// All directional lights (sun-like lights)
    pub directional_lights: Vec<DirectionalLightData>,
    /// All point lights (omnidirectional lights from a point)
    pub point_lights: Vec<PointLightData>,
    /// All spot lights (cone-shaped lights)
    pub spot_lights: Vec<SpotLightData>,
}

/// Directional light data ready for shader upload
#[derive(Debug, Copy, Clone)]
pub struct DirectionalLightData {
    pub direction: [f32; 3],
    pub color: [f32; 3],
    pub intensity: f32,
}

impl From<DirectionalLight> for DirectionalLightData {
    fn from(light: DirectionalLight) -> Self {
        Self {
            direction: [light.direction[0], light.direction[1], light.direction[2]],
            color: light.color,
            intensity: light.intensity,
        }
    }
}

/// Point light data ready for shader upload
#[derive(Debug, Copy, Clone)]
pub struct PointLightData {
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub intensity: f32,
    pub range: f32,
    pub constant: f32,
    pub linear: f32,
    pub quadratic: f32,
}

/// Spot light data ready for shader upload
#[derive(Debug, Copy, Clone)]
pub struct SpotLightData {
    pub position: [f32; 3],
    pub direction: [f32; 3],
    pub color: [f32; 3],
    pub intensity: f32,
    pub range: f32,
    pub cutoff_cos: f32,
    pub outer_cutoff_cos: f32,
    pub constant: f32,
    pub linear: f32,
    pub quadratic: f32,
}

impl Default for LightCollection {
    fn default() -> Self {
        Self {
            directional_lights: Vec::new(),
            point_lights: Vec::new(),
            spot_lights: Vec::new(),
        }
    }
}

impl LightCollection {
    /// Count total number of lights
    pub fn total_lights(&self) -> usize {
        self.directional_lights.len() + self.point_lights.len() + self.spot_lights.len()
    }

    /// Check if there are any lights
    pub fn is_empty(&self) -> bool {
        self.total_lights() == 0
    }

    /// Maximum lights of each type we support
    pub const MAX_DIRECTIONAL_LIGHTS: usize = 4;
    pub const MAX_POINT_LIGHTS: usize = 16;
    pub const MAX_SPOT_LIGHTS: usize = 8;
}

/// Collects and manages all lights in the scene.
///
/// This system runs every frame to collect all active light components
/// and update the LightCollection resource for use by the rendering system.
///
/// **Execution Order**: Should run BEFORE rendering systems.
pub struct LightingSystem;

impl LightingSystem {
    /// Collect all lights from the world and update the resource
    fn collect_lights(world: &mut World) {
        let mut light_collection = LightCollection::default();

        // Collect directional lights
        for (_, light) in world.query::<&DirectionalLight>() {
            if light_collection.directional_lights.len() < LightCollection::MAX_DIRECTIONAL_LIGHTS {
                light_collection.directional_lights.push((*light).into());
            }
        }

        // Collect point lights with their world positions
        for (_entity, light, transform) in world.query::<(&PointLight, &crate::components::TransformComponent)>() {
            if light_collection.point_lights.len() < LightCollection::MAX_POINT_LIGHTS {
                let pos = transform.transform.position;
                light_collection.point_lights.push(PointLightData {
                    position: [pos[0], pos[1], pos[2]],
                    color: light.color,
                    intensity: light.intensity,
                    range: light.range,
                    constant: light.constant,
                    linear: light.linear,
                    quadratic: light.quadratic,
                });
            }
        }

        // Collect spot lights with their world positions
        for (_entity, light, transform) in world.query::<(&SpotLight, &crate::components::TransformComponent)>() {
            if light_collection.spot_lights.len() < LightCollection::MAX_SPOT_LIGHTS {
                let pos = transform.transform.position;
                light_collection.spot_lights.push(SpotLightData {
                    position: [pos[0], pos[1], pos[2]],
                    direction: [light.direction[0], light.direction[1], light.direction[2]],
                    color: light.color,
                    intensity: light.intensity,
                    range: light.range,
                    cutoff_cos: light.cutoff_cos(),
                    outer_cutoff_cos: light.outer_cutoff_cos(),
                    constant: light.constant,
                    linear: light.linear,
                    quadratic: light.quadratic,
                });
            }
        }

        // Update or insert the resource
        if world.contains_resource::<LightCollection>() {
            *world.get_resource_mut::<LightCollection>().unwrap() = light_collection;
        } else {
            world.insert_resource(light_collection);
        }
    }
}

impl System for LightingSystem {
    fn update(&mut self, world: &mut World, _delta_time: f32) {
        Self::collect_lights(world);
    }

    fn name(&self) -> &str {
        "LightingSystem"
    }
}
