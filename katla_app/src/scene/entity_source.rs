use super::AssetRef;
use katla_ecs::Component;
use serde::{Deserialize, Serialize};

/// Records how an entity was originally created.
///
/// Attached at spawn time so the scene serializer can round-trip entity origins
/// without serializing GPU handles (MeshHandle, MaterialHandle, etc.).
#[derive(Component, Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum EntitySource {
    /// Transform-only entity; built-in and application components are independent.
    #[default]
    Empty,
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
        path: AssetRef,
    },
    /// Imported model controller; its independently authored primitives are children.
    GltfGroup {
        path: AssetRef,
    },
    /// Exactly one selected-scene node primitive, with its original material and skin.
    GltfPrimitive {
        path: AssetRef,
        node_index: usize,
        primitive_index: usize,
    },
    StlModel {
        path: AssetRef,
    },
    /// A reusable, validated static mesh recipe.
    MeshAsset {
        path: AssetRef,
    },
    ParticleEmitter,
    Light,
    /// Sensor volume without a drawable or GPU allocation.
    Trigger,
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
            Self::Empty => "Entity".to_string(),
            Self::Cube { .. } => "Cube".to_string(),
            Self::Sphere { .. } => "Sphere".to_string(),
            Self::Plane { .. } => "Plane".to_string(),
            Self::Cylinder { .. } => "Cylinder".to_string(),
            Self::Torus { .. } => "Torus".to_string(),
            Self::GltfModel { path }
            | Self::GltfGroup { path }
            | Self::GltfPrimitive { path, .. } => path
                .path()
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Model")
                .to_string(),
            Self::MeshAsset { path } | Self::StlModel { path } => path
                .path()
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("STL Model")
                .to_string(),
            Self::ParticleEmitter => "Particle Emitter".to_string(),
            Self::Light => "Light".to_string(),
            Self::Trigger => "Trigger".to_string(),
        }
    }
}
