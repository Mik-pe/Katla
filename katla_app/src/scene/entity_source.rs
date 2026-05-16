use katla_ecs::Component;
use serde::{Deserialize, Serialize};

/// Records how an entity was originally created.
///
/// Attached at spawn time so the scene serializer can round-trip entity origins
/// without serializing GPU handles (MeshHandle, MaterialHandle, etc.).
#[derive(Component, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EntitySource {
    Cube {
        size: [f32; 3],
    },
    Sphere {
        radius: f32,
        segments: u32,
        rings: u32,
    },
    Plane {
        width: f32,
        height: f32,
    },
    Cylinder {
        height: f32,
        radius: f32,
        segments: u32,
    },
    Torus {
        radius: f32,
        tube_radius: f32,
        segments: u32,
        tube_segments: u32,
    },
    GltfModel {
        path: String,
    },
    StlModel {
        path: String,
    },
    ParticleEmitter,
    Light,
}

impl EntitySource {
    /// Returns `true` if this source is a mesh primitive that can be spawned
    /// through the generic mesh creation path.
    pub fn is_mesh_primitive(&self) -> bool {
        matches!(
            self,
            Self::Cube { .. }
                | Self::Sphere { .. }
                | Self::Plane { .. }
                | Self::Cylinder { .. }
                | Self::Torus { .. }
        )
    }
}
