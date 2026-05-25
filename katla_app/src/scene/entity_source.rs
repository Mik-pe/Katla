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

    pub fn display_name(&self) -> String {
        match self {
            Self::Cube { .. } => "Cube".to_string(),
            Self::Sphere { .. } => "Sphere".to_string(),
            Self::Plane { .. } => "Plane".to_string(),
            Self::Cylinder { .. } => "Cylinder".to_string(),
            Self::Torus { .. } => "Torus".to_string(),
            Self::GltfModel { path } => std::path::Path::new(path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Model")
                .to_string(),
            Self::StlModel { path } => std::path::Path::new(path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("STL Model")
                .to_string(),
            Self::ParticleEmitter => "Particle Emitter".to_string(),
            Self::Light => "Light".to_string(),
        }
    }
}
