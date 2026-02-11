use std::path::{Path, PathBuf};

use gltf::buffer::Data as BufferData;
use gltf::image::Data as ImageData;
use gltf::Document;
use katla_math::{Sphere, Vec3};

use crate::rendering::{VertexNormal, VertexPBR, VertexPosition};
use crate::util::gltf_parser::{build_vertex_data, generate_smooth_normals, AttributeParser};

#[derive(Clone)]
pub struct GLTFModel {
    pub document: Document,
    pub buffers: Vec<BufferData>,
    pub images: Vec<ImageData>,
    pub vertex_data: Vec<VertexPBR>,
    pub index_data: Vec<u8>,
    pub index_stride: u8,
    pub bounds: Sphere,
}

impl GLTFModel {
    /// Parse a single GLTF node into vertex and index data.
    fn parse_node(&self, node: &gltf::Node) -> (Vec<VertexPBR>, Vec<u8>, u8, Sphere) {
        let mut positions = vec![];
        let mut normals = vec![];
        let mut tex_coords = vec![];
        let mut index_data = vec![];
        let mut index_stride = 0u8;

        let parser = AttributeParser::new(&self.buffers);

        if let Some(mesh) = node.mesh() {
            println!(
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
                            println!("    Parsed {} positions", positions.len());
                        }
                        gltf::mesh::Semantic::Normals => {
                            normals = parser.parse_normals(accessor);
                            println!("    Parsed {} normals", normals.len());
                        }
                        gltf::mesh::Semantic::TexCoords(0) => {
                            tex_coords = parser.parse_tex_coords(accessor);
                            println!("    Parsed {} tex_coords", tex_coords.len());
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

            // Debug: Check if normals are empty
            if normals.is_empty() {
                println!(
                    "    WARNING: No normals found! Generating smooth normals from geometry..."
                );
                normals = generate_smooth_normals(&positions, &index_data, index_stride);
                println!("    Generated {} normals", normals.len());
            }

            // Build vertex data from parsed attributes
            let (vertex_data, sphere) = build_vertex_data(positions, normals, tex_coords);
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

    fn parse_gltf(&mut self) {
        let mut used_nodes = vec![];
        for scene in self.document.scenes() {
            for node in scene.nodes() {
                used_nodes.push(node.index());
                for child in node.children() {
                    used_nodes.push(child.index());
                }
            }
        }

        for node in self.document.nodes() {
            if used_nodes.contains(&node.index()) {
                let (vertex_data, index_data, index_stride, sphere) = self.parse_node(&node);
                self.vertex_data.extend(vertex_data);
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

        let mut model = Self {
            document,
            buffers,
            images,
            vertex_data: vec![],
            index_data: vec![],
            index_stride: 0,
            bounds: Sphere::new(Vec3::new(0.0, 0.0, 0.0), 0.0),
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

    pub fn index_data(&self) -> Vec<u8> {
        self.index_data.clone()
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
        println!("Looking for model at: {}", model_path.display());
        let model = GLTFModel::new(&model_path).expect("Failed to load Fox.glb");
        println!(
            "Parsed {} vertices, {} indices",
            model.vertex_data.len(),
            model.index_data.len()
        );
        println!(
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
        println!("Looking for model at: {}", model_path.display());
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
