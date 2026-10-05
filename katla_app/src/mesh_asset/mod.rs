//! Editable static mesh recipes, independent of GPU handles and scene instances.

mod cache;
mod compile;
#[cfg(test)]
mod tests;

use crate::scene::{EntityDescriptor, EntitySource, Scene, SceneEntityId, TransformDescriptor};
pub(crate) use cache::{MeshAssetCache, upload};
pub use compile::CompiledMesh;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::Path};

/// Current `.katmesh` recipe version.
pub const MESH_VERSION: u32 = 1;
pub(crate) const MAX_VERTICES: u64 = 1_000_000;
pub(crate) const MAX_INDICES: u64 = 6_000_000;

/// Named, editable parts compiled into one static mesh and one material draw.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeshAsset {
    pub version: u32,
    pub name: String,
    pub parts: Vec<MeshPart>,
}

/// Part IDs are authoring labels. Transforms are baked into the final geometry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeshPart {
    pub id: String,
    #[serde(default = "TransformDescriptor::default_transform")]
    pub transform: TransformDescriptor,
    pub geometry: Geometry,
}

/// Compact primitives or explicit indexed triangles; no executable generators.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Geometry {
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
        radius: f32,
        height: f32,
        segments: u32,
    },
    Cone {
        radius: f32,
        height: f32,
        segments: u32,
    },
    Torus {
        radius: f32,
        tube_radius: f32,
        segments: u32,
        tube_segments: u32,
    },
    /// CCW triangles. Omitted normals are area-weighted; omitted UVs are zero.
    Triangles {
        positions: Vec<[f32; 3]>,
        indices: Vec<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        normals: Option<Vec<[f32; 3]>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        uvs: Option<Vec<[f32; 2]>>,
    },
}

impl MeshAsset {
    /// Validate the entire recipe and combined allocation budget before generation.
    pub fn validate(&self) -> Result<(), String> {
        if self.version != MESH_VERSION {
            return Err(format!(
                "Unsupported mesh version {}; expected {MESH_VERSION}",
                self.version
            ));
        }
        if self.name.trim().is_empty() || self.name.len() > 256 {
            return Err("Mesh name requires 1..256 bytes".into());
        }
        if self.parts.is_empty() || self.parts.len() > 1024 {
            return Err("Mesh requires 1..1024 parts".into());
        }
        let mut ids = HashSet::new();
        let mut vertices = 0u64;
        let mut indices = 0u64;
        for part in &self.parts {
            let mut check = || -> Result<(u64, u64), String> {
                if part.id.trim().is_empty() || part.id.len() > 128 || !ids.insert(&part.id) {
                    return Err("Part IDs require 1..128 bytes and must be unique".into());
                }
                let source = part.geometry.source();
                let mut scene = Scene::new("Mesh part");
                scene.next_entity_id = 2;
                let mut entity = EntityDescriptor::new(SceneEntityId(1), source);
                entity.transform = part.transform.clone();
                scene.entities.push(entity);
                scene.validate().map_err(|error| error.to_string())?;
                // Bound arithmetic before f32 generator math, including cone slope squares.
                if part
                    .transform
                    .position
                    .iter()
                    .chain(&part.transform.scale)
                    .any(|value| value.abs() > 1_000_000.0)
                {
                    return Err("Transform magnitudes must not exceed 1,000,000".into());
                }
                part.geometry.budget()
            };
            let (v, i) = check().map_err(|error| format!("part '{}': {error}", part.id))?;
            vertices = vertices.saturating_add(v);
            indices = indices.saturating_add(i);
            if vertices > MAX_VERTICES || indices > MAX_INDICES {
                return Err("Combined mesh exceeds 1,000,000 vertices or 6,000,000 indices".into());
            }
        }
        Ok(())
    }

    /// Validate and bake a deterministic PBR vertex/index stream with CPU geometry.
    pub fn compile(&self) -> Result<CompiledMesh, String> {
        self.validate()?;
        compile::compile(self)
    }

    /// Read a bounded, strict RON recipe.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = crate::util::asset_io::read_text(path)?;
        Self::parse(&text)
    }

    /// Parse without creating native resources.
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() > crate::util::asset_io::MAX_ASSET_BYTES {
            return Err("Mesh file exceeds the 64 MiB limit".into());
        }
        let asset: Self = ron::from_str(text).map_err(|error| error.to_string())?;
        asset.validate()?;
        Ok(asset)
    }

    /// Atomically write a validated recipe. Existing files survive failed validation.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        // Compilation also validates generated normals, tangents and transformed bounds.
        self.compile()?;
        let text = ron::ser::to_string_pretty(self, crate::scene::ron_pretty_config())
            .map_err(|error| error.to_string())?;
        crate::util::asset_io::write_text(path, &text)
    }

    pub(crate) fn geometry_key(&self) -> Result<String, String> {
        let geometry: Vec<_> = self
            .parts
            .iter()
            .map(|part| (&part.geometry, &part.transform))
            .collect();
        ron::to_string(&(MESH_VERSION, geometry)).map_err(|error| error.to_string())
    }
}

impl Geometry {
    fn source(&self) -> EntitySource {
        match self {
            Self::Cube { size } => EntitySource::Cube { size: *size },
            Self::Sphere {
                radius,
                segments,
                rings,
            } => EntitySource::Sphere {
                radius: *radius,
                segments: *segments,
                rings: *rings,
            },
            Self::Plane { width, height } => EntitySource::Plane {
                width: *width,
                height: *height,
            },
            Self::Cylinder {
                radius,
                height,
                segments,
            }
            | Self::Cone {
                radius,
                height,
                segments,
            } => EntitySource::Cylinder {
                radius: *radius,
                height: *height,
                segments: *segments,
            },
            Self::Torus {
                radius,
                tube_radius,
                segments,
                tube_segments,
            } => EntitySource::Torus {
                radius: *radius,
                tube_radius: *tube_radius,
                segments: *segments,
                tube_segments: *tube_segments,
            },
            Self::Triangles { .. } => EntitySource::Empty,
        }
    }

    fn budget(&self) -> Result<(u64, u64), String> {
        let bounded = |v: f32| v.is_finite() && v.abs() <= 1_000_000.0;
        let result = match self {
            Self::Cube { size } if size.iter().all(|v| bounded(*v)) => (24, 36),
            Self::Plane { width, height } if bounded(*width) && bounded(*height) => (4, 6),
            Self::Sphere { radius, segments, rings } if bounded(*radius) && *rings >= 3 => {
                ((*segments as u64 + 1).saturating_mul(*rings as u64 + 1), 6u64.saturating_mul(*segments as u64).saturating_mul(*rings as u64 - 1))
            },
            Self::Cylinder { radius, height, segments } if bounded(*radius) && bounded(*height) => (4u64 * *segments as u64 + 6, 12u64 * *segments as u64),
            Self::Cone { radius, height, segments } if bounded(*radius) && bounded(*height) => (2u64 * *segments as u64 + 4, 6u64 * *segments as u64),
            Self::Torus { radius, tube_radius, segments, tube_segments } if bounded(*radius) && bounded(*tube_radius) && *tube_segments >= 3 => {
                ((*segments as u64 + 1).saturating_mul(*tube_segments as u64 + 1), 6u64.saturating_mul(*segments as u64).saturating_mul(*tube_segments as u64))
            },
            Self::Triangles { positions, indices, normals, uvs } => {
                if positions.len() < 3 || positions.len() as u64 > MAX_VERTICES || indices.is_empty()
                    || !indices.len().is_multiple_of(3) || indices.len() as u64 > MAX_INDICES
                    || positions.iter().flatten().any(|v| !bounded(*v))
                    || indices.iter().any(|index| *index as usize >= positions.len()) {
                    return Err("Triangles need finite bounded positions and in-range CCW index triples within the mesh budget".into());
                }
                if normals.as_ref().is_some_and(|values| values.len() != positions.len()
                    || values.iter().any(|normal| compile::unit(*normal).is_none())) {
                    return Err("Normals must match the position count and be finite nonzero vectors".into());
                }
                if uvs.as_ref().is_some_and(|values| values.len() != positions.len()
                    || values.iter().flatten().any(|v| !bounded(*v))) {
                    return Err("UVs must match the position count and be finite bounded pairs".into());
                }
                (positions.len() as u64, indices.len() as u64)
            },
            _ => return Err("Geometry magnitudes must not exceed 1,000,000; sphere rings and torus tube_segments must be at least 3".into()),
        };
        Ok(result)
    }
}
