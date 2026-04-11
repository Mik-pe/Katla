use katla_ecs::scene_tool::SceneOp;

use super::placement;

/// A scene template: a named collection of entity spawn operations
/// with predefined component configurations.
pub struct SceneTemplate {
    pub name: &'static str,
    pub description: &'static str,
    pub ops: Vec<SceneOp>,
}

/// Instantiate a "campfire" template: a single named entity at the given position.
pub fn campfire(position: [f32; 3]) -> SceneTemplate {
    SceneTemplate {
        name: "campfire",
        description: "Warm point light simulating a campfire",
        ops: vec![SceneOp::SpawnEntity {
            position,
            rotation: [0.0, 0.0, 0.0],
            scale: [1.0, 1.0, 1.0],
            name: Some("Campfire".to_string()),
        }],
    }
}

/// Instantiate a "street lamp" template: a light at height.
pub fn street_lamp(position: [f32; 3]) -> SceneTemplate {
    SceneTemplate {
        name: "street_lamp",
        description: "Street lamp with overhead light",
        ops: vec![SceneOp::SpawnEntity {
            position,
            rotation: [0.0, 0.0, 0.0],
            scale: [1.0, 1.0, 1.0],
            name: Some("StreetLamp".to_string()),
        }],
    }
}

/// Instantiate a "village square" template: a grid of lights around a central point.
pub fn village_square(center: [f32; 3], size: usize, spacing: f32) -> SceneTemplate {
    let ops = placement::place_grid(size, size, center, [spacing, spacing], "SquareLight");
    SceneTemplate {
        name: "village_square",
        description: "Grid of lights around a central point",
        ops,
    }
}

/// Instantiate a "forest clearing" template: ring of entities around a center.
pub fn forest_clearing(center: [f32; 3], tree_count: usize, radius: f32) -> SceneTemplate {
    let ops = placement::place_ring(tree_count, center, radius, "Tree");
    SceneTemplate {
        name: "forest_clearing",
        description: "Ring of trees around a clearing",
        ops,
    }
}

/// List all available template names and descriptions.
pub fn available_templates() -> Vec<(&'static str, &'static str)> {
    vec![
        ("campfire", "Warm point light simulating a campfire"),
        ("street_lamp", "Street lamp with overhead light"),
        ("village_square", "Grid of lights around a central point"),
        ("forest_clearing", "Ring of trees around a clearing"),
    ]
}
