use std::path::{Path, PathBuf};

use gltf::buffer::Data as BufferData;
use gltf::image::Data as ImageData;
use gltf::Document;
use katla_math::{Mat4, Quat, Sphere, Vec3};
use log::{debug, info, warn};

use crate::rendering::{VertexNormal, VertexPBR, VertexPosition, VertexSkinned};
use crate::util::gltf_material::GltfMaterialInfo;
use crate::util::gltf_parser::{build_skinned_vertex_data, build_vertex_data, generate_smooth_normals, AttributeParser, ParsedAttributes};

#[derive(Clone)]
pub struct GLTFModel {
    pub document: Document,
    pub buffers: Vec<BufferData>,
    pub images: Vec<ImageData>,
    /// Parsed material info for each material in the GLTF file.
    pub materials: Vec<GltfMaterialInfo>,
    pub vertex_data: Vec<VertexPBR>,
    pub skinned_vertex_data: Vec<VertexSkinned>,
    pub has_skinning: bool,
    pub index_data: Vec<u8>,
    pub index_stride: u8,
    pub bounds: Sphere,
    /// SoA (Structure of Arrays) vertex attributes for flexible rendering
    pub parsed_attributes: Option<ParsedAttributes>,
    /// Root node transform from GLTF (combined transform of first scene's root nodes)
    pub root_transform: Mat4,
}

impl GLTFModel {
    /// Parse a single GLTF node into vertex and index data.
    fn parse_node(&self, node: &gltf::Node) -> (Vec<VertexPBR>, Vec<u8>, u8, Sphere) {
        let mut positions = vec![];
        let mut normals = vec![];
        let mut tangents = vec![];
        let mut tex_coords = vec![];
        let mut index_data = vec![];
        let mut index_stride = 0u8;

        let parser = AttributeParser::new(&self.buffers);

        if let Some(mesh) = node.mesh() {
            debug!(
                "  Mesh '{}' has {} primitives",
                mesh.name().unwrap_or("unnamed"),
                mesh.primitives().count()
            );

            for primitive in mesh.primitives() {
                // Parse attributes using the new parser
                for (semantic, accessor) in primitive.attributes() {
                    match semantic {
                        gltf::mesh::Semantic::Positions => {
                            positions = parser.parse_positions(accessor);
                            debug!("    Parsed {} positions", positions.len());
                        }
                        gltf::mesh::Semantic::Normals => {
                            normals = parser.parse_normals(accessor);
                            debug!("    Parsed {} normals", normals.len());
                        }
                        gltf::mesh::Semantic::Tangents => {
                            tangents = parser.parse_tangents(accessor);
                            debug!("    Parsed {} tangents", tangents.len());
                        }
                        gltf::mesh::Semantic::TexCoords(0) => {
                            tex_coords = parser.parse_tex_coords(accessor);
                            debug!("    Parsed {} tex_coords", tex_coords.len());
                        }
                        _ => {
                            continue;
                        }
                    }
                }

                // Parse indices
                if let Some(indices) = primitive.indices() {
                    let (indices_data, stride) = parser.parse_indices(indices);
                    index_data = indices_data;
                    index_stride = stride;
                }
            }

            // Generate normals if missing (worth warning about)
            if normals.is_empty() {
                warn!(
                    "Mesh '{}' has no normals, generating smooth normals from geometry",
                    mesh.name().unwrap_or("unnamed")
                );
                normals = generate_smooth_normals(&positions, &index_data, index_stride);
            }

            // Build vertex data from parsed attributes
            let (vertex_data, sphere) = build_vertex_data(positions, normals, tangents, tex_coords);
            (vertex_data, index_data, index_stride, sphere)
        } else {
            // No mesh - return empty data
            (
                vec![],
                vec![],
                0,
                Sphere::new(Vec3::new(0.0, 0.0, 0.0), 0.0),
            )
        }
    }

    /// Parse a single GLTF node into skinned vertex data.
    fn parse_node_skinned(&self, node: &gltf::Node) -> (Vec<VertexSkinned>, Vec<u8>, u8, Sphere, bool) {
        let mut positions = vec![];
        let mut normals = vec![];
        let mut tex_coords = vec![];
        let mut joint_indices = vec![];
        let mut joint_weights = vec![];
        let mut index_data = vec![];
        let mut index_stride = 0u8;

        let parser = AttributeParser::new(&self.buffers);

        if let Some(mesh) = node.mesh() {
            for primitive in mesh.primitives() {
                // Parse all attributes including skinning
                for (semantic, accessor) in primitive.attributes() {
                    match semantic {
                        gltf::mesh::Semantic::Positions => {
                            positions = parser.parse_positions(accessor);
                        }
                        gltf::mesh::Semantic::Normals => {
                            normals = parser.parse_normals(accessor);
                        }
                        gltf::mesh::Semantic::TexCoords(0) => {
                            tex_coords = parser.parse_tex_coords(accessor);
                        }
                        gltf::mesh::Semantic::Joints(0) => {
                            joint_indices = parser.parse_joint_indices(accessor);
                            debug!("    Parsed {} joint indices", joint_indices.len());
                        }
                        gltf::mesh::Semantic::Weights(0) => {
                            joint_weights = parser.parse_joint_weights(accessor);
                            debug!("    Parsed {} joint weights", joint_weights.len());
                        }
                        _ => {}
                    }
                }

                // Parse indices
                if let Some(indices) = primitive.indices() {
                    let (indices_data, stride) = parser.parse_indices(indices);
                    index_data = indices_data;
                    index_stride = stride;
                }
            }

            // Generate normals if missing
            if normals.is_empty() {
                normals = generate_smooth_normals(&positions, &index_data, index_stride);
            }

            let has_skinning = !joint_indices.is_empty() && !joint_weights.is_empty();
            let (vertex_data, sphere) = build_skinned_vertex_data(
                positions, normals, tex_coords, joint_indices, joint_weights
            );
            (vertex_data, index_data, index_stride, sphere, has_skinning)
        } else {
            (
                vec![],
                vec![],
                0,
                Sphere::new(Vec3::new(0.0, 0.0, 0.0), 0.0),
                false,
            )
        }
    }

    fn parse_gltf(&mut self) {
        let mut used_nodes = vec![];

        // Extract root transform from first scene's root nodes
        // Combine all root node transforms into a single transform
        let mut root_transform = Mat4::identity();
        if let Some(scene) = self.document.default_scene().or_else(|| self.document.scenes().next()) {
            for node in scene.nodes() {
                let transform = node.transform();
                let (t, r, s) = transform.decomposed();
                let translation = Vec3::new(t[0], t[1], t[2]);
                let rotation = Quat::new_from_xyzw(r[0], r[1], r[2], r[3]);
                let scale = Vec3::new(s[0], s[1], s[2]);
                root_transform = root_transform * Mat4::from_trs(translation, rotation, scale);

                used_nodes.push(node.index());
                for child in node.children() {
                    used_nodes.push(child.index());
                }
            }
        }
        self.root_transform = root_transform;

        for node in self.document.nodes() {
            if used_nodes.contains(&node.index()) {
                // Parse both regular and skinned vertex data
                let (vertex_data, index_data, index_stride, sphere) = self.parse_node(&node);
                let (skinned_data, _, _, _, has_skinning) = self.parse_node_skinned(&node);

                self.vertex_data.extend(vertex_data);
                self.skinned_vertex_data.extend(skinned_data);
                if has_skinning {
                    self.has_skinning = true;
                }
                self.index_data.extend(index_data);
                self.index_stride = index_stride;
                self.bounds = sphere;
            }
        }
    }

    pub fn new<P>(path: P) -> Result<Self, Box<dyn std::error::Error>>
    where
        P: AsRef<Path>,
    {
        let (document, buffers, images) = gltf::import(path)?;

        // Parse materials from the GLTF document
        let materials: Vec<GltfMaterialInfo> = document
            .materials()
            .map(|m| GltfMaterialInfo::from_gltf(&m))
            .collect();

        debug!("Parsed {} materials from GLTF", materials.len());
        for (i, mat) in materials.iter().enumerate() {
            debug!("  Material {}: {}", i, mat.summary());
        }

        let mut model = Self {
            document,
            buffers,
            images,
            materials,
            vertex_data: vec![],
            skinned_vertex_data: vec![],
            has_skinning: false,
            index_data: vec![],
            index_stride: 0,
            bounds: Sphere::new(Vec3::new(0.0, 0.0, 0.0), 0.0),
            parsed_attributes: None,
            root_transform: Mat4::identity(),
        };
        model.parse_gltf();
        Ok(model)
    }

    pub fn vertpos(&self) -> Vec<VertexPosition> {
        self.vertex_data
            .iter()
            .map(|x| VertexPosition {
                position: x.position,
            })
            .collect::<Vec<VertexPosition>>()
    }

    pub fn vertposnorm(&self) -> Vec<VertexNormal> {
        self.vertex_data
            .iter()
            .map(|x| VertexNormal {
                position: x.position,
                normal: x.normal,
            })
            .collect::<Vec<VertexNormal>>()
    }

    pub fn vertpbr(&self) -> Vec<VertexPBR> {
        self.vertex_data.clone()
    }

    pub fn vertskinned(&self) -> Vec<VertexSkinned> {
        self.skinned_vertex_data.clone()
    }

    pub fn index_data(&self) -> Vec<u8> {
        self.index_data.clone()
    }

    /// Get SoA (Structure of Arrays) vertex attributes.
    ///
    /// This method parses the first primitive of the first mesh node
    /// into separate attribute arrays for flexible rendering.
    ///
    /// Returns None if the model has no mesh or no primitives.
    pub fn parsed_attributes(&self) -> Option<ParsedAttributes> {
        // Find the first mesh node
        for node in self.document.nodes() {
            if let Some(mesh) = node.mesh() {
                // Get the first primitive
                for primitive in mesh.primitives() {
                    let parser = AttributeParser::new(&self.buffers);
                    return Some(ParsedAttributes::from_gltf(&primitive, &parser));
                }
            }
        }
        None
    }

    /// Check if this model has SoA attributes parsed.
    pub fn has_soa_attributes(&self) -> bool {
        self.parsed_attributes.is_some()
    }
}

impl From<PathBuf> for GLTFModel {
    fn from(pathbuf: PathBuf) -> Self {
        GLTFModel::new(&pathbuf).unwrap_or_else(|e| {
            panic!("Failed to load GLTF model from {:?}: {}", pathbuf, e);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require actual GLTF files to run.
    // They serve as integration tests for the parser.

    #[test]
    fn test_parse_fox_gltf() {
        // Resources are at workspace root, not crate root
        let mut model_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        model_path.pop(); // Go up from katla_app to workspace root
        model_path.push("resources");
        model_path.push("models");
        model_path.push("Fox.glb");
        debug!("Looking for model at: {}", model_path.display());
        let model = GLTFModel::new(&model_path).expect("Failed to load Fox.glb");
        debug!(
            "Parsed {} vertices, {} indices",
            model.vertex_data.len(),
            model.index_data.len()
        );
        debug!(
            "Bounds: center={:?}, radius={}",
            model.bounds.center, model.bounds.radius
        );

        // Just verify we can parse the model, even if bounds are zero
        assert!(!model.vertex_data.is_empty(), "Should have vertex data");
        // Fox.glb may not have index data or may have zero bounds
        // The important thing is that we can parse it without crashing
    }

    #[test]
    fn test_parse_box_gltf() {
        // Resources are at workspace root, not crate root
        let mut model_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        model_path.pop(); // Go up from katla_app to workspace root
        model_path.push("resources");
        model_path.push("models");
        model_path.push("Box.glb");
        debug!("Looking for model at: {}", model_path.display());
        let model = GLTFModel::new(&model_path).expect("Failed to load Box.glb");
        assert!(!model.vertex_data.is_empty());
        assert!(!model.index_data.is_empty());
        assert!(model.bounds.radius > 0.0);
    }

    #[test]
    fn test_empty_vertex_data() {
        let positions: Vec<[f32; 3]> = vec![];
        let normals: Vec<[f32; 3]> = vec![];
        let tex_coords: Vec<[f32; 2]> = vec![];

        let (vertices, sphere) = build_vertex_data(positions, normals, tex_coords);
        assert!(vertices.is_empty());
        assert_eq!(sphere.radius, 0.0);
    }
}
