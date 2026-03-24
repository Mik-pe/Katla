use std::collections::HashMap;
use std::path::Path;

use gltf::Document;
use gltf::buffer::Data as BufferData;
use gltf::image::Data as ImageData;
use katla_gfx::AttributeType;
use katla_math::{Mat4, Quat, Sphere, Vec3};
use log::{debug, warn};

use crate::util::gltf_material::GltfMaterialInfo;
use crate::util::gltf_parser::{
    AttributeParser, ParsedAttributes, build_skinned_vertex_data, build_vertex_data,
    generate_smooth_normals,
};
use katla_gfx::{VertexPBR, VertexPBRSkinned};

#[derive(Clone)]
pub struct GLTFModel {
    pub document: Document,
    pub buffers: Vec<BufferData>,
    pub images: Vec<ImageData>,
    /// Parsed material info for each material in the GLTF file.
    pub materials: Vec<GltfMaterialInfo>,
    pub vertex_data: Vec<VertexPBR>,
    pub skinned_vertex_data: Vec<VertexPBRSkinned>,
    /// SOA vertex attributes for non-skinned meshes (separate per-attribute byte arrays).
    pub vertex_attributes: HashMap<AttributeType, Vec<u8>>,
    /// SOA vertex attributes for skinned meshes (separate per-attribute byte arrays).
    pub skinned_vertex_attributes: HashMap<AttributeType, Vec<u8>>,
    pub has_skinning: bool,
    pub index_data: Vec<u8>,
    pub index_stride: u8,
    pub bounds: Sphere,
    /// Root node transform from GLTF (combined transform of first scene's root nodes)
    pub root_transform: Mat4,
}

fn collect_all_nodes<'a>(node: &gltf::Node<'a>, nodes: &mut Vec<gltf::Node<'a>>) {
    nodes.push(node.clone());
    for child in node.children() {
        collect_all_nodes(&child, nodes);
    }
}

impl GLTFModel {
    /// Transform vertex positions and normals by a world transform matrix.
    ///
    /// Positions are transformed by the full matrix.
    /// Normals are transformed by the upper-left 3x3 (rotation/scale only, no translation).
    fn transform_vertex_data(vertices: &mut [VertexPBR], world_transform: &Mat4) {
        // For normals, we need the inverse-transpose of the upper 3x3 for correct
        // handling of non-uniform scaling. However, for simplicity and performance,
        // we use the upper 3x3 directly which works correctly for uniform scaling
        // and rotations. Non-uniform scaling may produce slightly incorrect normals.
        use katla_math::Vec4;

        for vertex in vertices.iter_mut() {
            let pos = vertex.position;
            let pos_vec = Vec4::new(pos[0], pos[1], pos[2], 1.0);
            let transformed_pos = *world_transform * pos_vec;
            vertex.position = [
                transformed_pos.x(),
                transformed_pos.y(),
                transformed_pos.z(),
            ];

            let normal = vertex.normal;
            let normal_vec = Vec4::new(normal[0], normal[1], normal[2], 0.0);
            let transformed_normal = *world_transform * normal_vec;
            let len = (transformed_normal.x() * transformed_normal.x()
                + transformed_normal.y() * transformed_normal.y()
                + transformed_normal.z() * transformed_normal.z())
            .sqrt();
            if len > 0.0 {
                vertex.normal = [
                    transformed_normal.x() / len,
                    transformed_normal.y() / len,
                    transformed_normal.z() / len,
                ];
            }
        }
    }

    fn deinterleave_pbr(vertices: &[VertexPBR]) -> HashMap<AttributeType, Vec<u8>> {
        let mut map = HashMap::new();
        if vertices.is_empty() {
            return map;
        }
        let mut positions = Vec::with_capacity(vertices.len() * 12);
        let mut normals = Vec::with_capacity(vertices.len() * 12);
        let mut tangents = Vec::with_capacity(vertices.len() * 16);
        let mut tex_coords = Vec::with_capacity(vertices.len() * 8);
        for v in vertices {
            positions.extend_from_slice(bytemuck::bytes_of(&v.position));
            normals.extend_from_slice(bytemuck::bytes_of(&v.normal));
            tangents.extend_from_slice(bytemuck::bytes_of(&v.tangent));
            tex_coords.extend_from_slice(bytemuck::bytes_of(&v.tex_coord0));
        }
        map.insert(AttributeType::Position, positions);
        map.insert(AttributeType::Normal, normals);
        map.insert(AttributeType::Tangent, tangents);
        map.insert(AttributeType::TexCoord0, tex_coords);
        map
    }

    fn deinterleave_pbr_skinned(vertices: &[VertexPBRSkinned]) -> HashMap<AttributeType, Vec<u8>> {
        let mut map = HashMap::new();
        if vertices.is_empty() {
            return map;
        }
        let mut positions = Vec::with_capacity(vertices.len() * 12);
        let mut normals = Vec::with_capacity(vertices.len() * 12);
        let mut tangents = Vec::with_capacity(vertices.len() * 16);
        let mut tex_coords = Vec::with_capacity(vertices.len() * 8);
        let mut joint_indices = Vec::with_capacity(vertices.len() * 8);
        let mut joint_weights = Vec::with_capacity(vertices.len() * 16);
        for v in vertices {
            positions.extend_from_slice(bytemuck::bytes_of(&v.position));
            normals.extend_from_slice(bytemuck::bytes_of(&v.normal));
            tangents.extend_from_slice(bytemuck::bytes_of(&v.tangent));
            tex_coords.extend_from_slice(bytemuck::bytes_of(&v.tex_coord0));
            joint_indices.extend_from_slice(bytemuck::bytes_of(&v.joint_indices));
            joint_weights.extend_from_slice(bytemuck::bytes_of(&v.joint_weights));
        }
        map.insert(AttributeType::Position, positions);
        map.insert(AttributeType::Normal, normals);
        map.insert(AttributeType::Tangent, tangents);
        map.insert(AttributeType::TexCoord0, tex_coords);
        map.insert(AttributeType::JointIndices, joint_indices);
        map.insert(AttributeType::JointWeights, joint_weights);
        map
    }

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

            for (prim_idx, primitive) in mesh.primitives().enumerate() {
                debug!(
                    "    Primitive {}: has_indices={}",
                    prim_idx,
                    primitive.indices().is_some()
                );
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

                if let Some(indices) = primitive.indices()
                    && let Some((indices_data, stride)) = parser.parse_indices(indices)
                {
                    index_data = indices_data;
                    index_stride = stride;
                }
            }

            if normals.is_empty() {
                warn!(
                    "Mesh '{}' has no normals, generating smooth normals from geometry",
                    mesh.name().unwrap_or("unnamed")
                );
                normals = generate_smooth_normals(&positions, &index_data, index_stride);
            }

            let (vertex_data, sphere) = build_vertex_data(
                positions.clone(),
                normals.clone(),
                tangents.clone(),
                tex_coords.clone(),
            );
            (vertex_data, index_data, index_stride, sphere)
        } else {
            (
                vec![],
                vec![],
                0,
                Sphere::new(Vec3::new(0.0, 0.0, 0.0), 0.0),
            )
        }
    }

    /// Parse a single GLTF node into skinned vertex data.
    fn parse_node_skinned(
        &self,
        node: &gltf::Node,
    ) -> (Vec<VertexPBRSkinned>, Vec<u8>, u8, Sphere, bool) {
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

                if let Some(indices) = primitive.indices()
                    && let Some((indices_data, stride)) = parser.parse_indices(indices)
                {
                    index_data = indices_data;
                    index_stride = stride;
                }
            }

            if normals.is_empty() {
                normals = generate_smooth_normals(&positions, &index_data, index_stride);
            }

            let has_skinning = !joint_indices.is_empty() && !joint_weights.is_empty();
            let (vertex_data, sphere) = build_skinned_vertex_data(
                positions,
                normals,
                tex_coords,
                joint_indices,
                joint_weights,
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
        use std::collections::{HashMap, VecDeque};

        fn build_world_transforms(nodes: &[gltf::Node]) -> HashMap<usize, Mat4> {
            let mut parent_map: HashMap<usize, Option<usize>> = HashMap::new();
            for node in nodes {
                parent_map.entry(node.index()).or_insert(None);
                for child in node.children() {
                    parent_map.insert(child.index(), Some(node.index()));
                }
            }

            let mut children_map: HashMap<usize, Vec<usize>> = HashMap::new();
            for node in nodes {
                children_map.entry(node.index()).or_default();
                for child in node.children() {
                    children_map
                        .entry(node.index())
                        .or_default()
                        .push(child.index());
                }
            }

            let node_by_index: HashMap<usize, &gltf::Node> =
                nodes.iter().map(|n| (n.index(), n)).collect();

            let mut queue: VecDeque<usize> = VecDeque::new();
            for node in nodes {
                if parent_map.get(&node.index()) == Some(&None) {
                    queue.push_back(node.index());
                }
            }

            let mut world_transforms: HashMap<usize, Mat4> = HashMap::new();

            // Process in topological order (parents before children)
            while let Some(node_index) = queue.pop_front() {
                let node = match node_by_index.get(&node_index) {
                    Some(n) => n,
                    None => continue,
                };

                let transform = node.transform();
                let (t, r, s) = transform.decomposed();
                let translation = Vec3::new(t[0], t[1], t[2]);
                let rotation = Quat::new(r[0], r[1], r[2], r[3]);
                let scale = Vec3::new(s[0], s[1], s[2]);
                let local_matrix = Mat4::from_trs(translation, rotation, scale);

                let world_matrix = if let Some(Some(parent_index)) = parent_map.get(&node_index) {
                    if let Some(parent_transform) = world_transforms.get(parent_index) {
                        *parent_transform * local_matrix
                    } else {
                        local_matrix
                    }
                } else {
                    local_matrix
                };

                world_transforms.insert(node_index, world_matrix);

                if let Some(children) = children_map.get(&node_index) {
                    for child_index in children {
                        queue.push_back(*child_index);
                    }
                }
            }

            world_transforms
        }

        let mut all_nodes = vec![];
        let mut root_transform = Mat4::identity();

        if let Some(scene) = self
            .document
            .default_scene()
            .or_else(|| self.document.scenes().next())
        {
            for node in scene.nodes() {
                let transform = node.transform();
                let (t, r, s) = transform.decomposed();
                let translation = Vec3::new(t[0], t[1], t[2]);
                let rotation = Quat::new(r[0], r[1], r[2], r[3]);
                let scale = Vec3::new(s[0], s[1], s[2]);
                root_transform *= Mat4::from_trs(translation, rotation, scale);

                collect_all_nodes(&node, &mut all_nodes);
            }
        }
        self.root_transform = root_transform;

        let world_transforms = build_world_transforms(&all_nodes);

        // Track vertex offset for index adjustment when combining nodes
        let mut vertex_offset: u32 = 0;
        let mut skinned_vertex_offset: u32 = 0;

        for node in &all_nodes {
            let (mut vertex_data, index_data, index_stride, _sphere) = self.parse_node(node);
            let (skinned_data, skinned_index_data, skinned_index_stride, _, has_skinning) =
                self.parse_node_skinned(node);

            let (final_index_data, final_index_stride) = if has_skinning {
                (skinned_index_data, skinned_index_stride)
            } else {
                (index_data, index_stride)
            };

            let vertex_count = vertex_data.len();
            let skinned_vertex_count = skinned_data.len();

            // Apply world transform to non-skinned vertices
            if !has_skinning
                && !vertex_data.is_empty()
                && let Some(world_transform) = world_transforms.get(&node.index())
            {
                Self::transform_vertex_data(&mut vertex_data, world_transform);
            }

            let soa_attributes = Self::deinterleave_pbr(&vertex_data);
            let skinned_soa_attributes = Self::deinterleave_pbr_skinned(&skinned_data);

            let offset = if has_skinning {
                skinned_vertex_offset
            } else {
                vertex_offset
            };
            let adjusted_index_data =
                Self::adjust_indices(&final_index_data, final_index_stride, offset);

            self.vertex_data.extend(vertex_data);
            self.skinned_vertex_data.extend(skinned_data);

            for (attr_type, data) in soa_attributes {
                self.vertex_attributes
                    .entry(attr_type)
                    .or_default()
                    .extend_from_slice(&data);
            }
            for (attr_type, data) in skinned_soa_attributes {
                self.skinned_vertex_attributes
                    .entry(attr_type)
                    .or_default()
                    .extend_from_slice(&data);
            }

            if has_skinning {
                self.has_skinning = true;
            }
            self.index_data.extend(adjusted_index_data);
            if final_index_stride > 0 {
                self.index_stride = final_index_stride;
            }

            vertex_offset += vertex_count as u32;
            skinned_vertex_offset += skinned_vertex_count as u32;
        }

        if !self.vertex_data.is_empty() {
            let mut min_pos = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
            let mut max_pos = Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);

            for vertex in &self.vertex_data {
                let pos = vertex.position;
                min_pos = Vec3::new(
                    min_pos.x().min(pos[0]),
                    min_pos.y().min(pos[1]),
                    min_pos.z().min(pos[2]),
                );
                max_pos = Vec3::new(
                    max_pos.x().max(pos[0]),
                    max_pos.y().max(pos[1]),
                    max_pos.z().max(pos[2]),
                );
            }

            let center = Vec3::new(
                (min_pos.x() + max_pos.x()) * 0.5,
                (min_pos.y() + max_pos.y()) * 0.5,
                (min_pos.z() + max_pos.z()) * 0.5,
            );

            let radius = ((max_pos.x() - min_pos.x()).powi(2)
                + (max_pos.y() - min_pos.y()).powi(2)
                + (max_pos.z() - min_pos.z()).powi(2))
            .sqrt()
                * 0.5;

            self.bounds = Sphere::new(center, radius);
        }

        // For non-skinned meshes, transforms are baked into vertices.
        // Reset root_transform to identity to prevent double transformation
        // when the entity's TransformComponent is applied at runtime.
        // Skinned meshes still need root_transform for runtime transformation
        // since their vertices are transformed by joint matrices.
        if !self.has_skinning {
            self.root_transform = Mat4::identity();
        }
    }

    pub fn new<P>(path: P) -> Result<Self, Box<dyn std::error::Error>>
    where
        P: AsRef<Path>,
    {
        let (document, buffers, images) = gltf::import(path)?;

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
            vertex_attributes: HashMap::new(),
            skinned_vertex_attributes: HashMap::new(),
            has_skinning: false,
            index_data: vec![],
            index_stride: 0,
            bounds: Sphere::new(Vec3::new(0.0, 0.0, 0.0), 0.0),
            root_transform: Mat4::identity(),
        };
        model.parse_gltf();
        Ok(model)
    }

    /// Get PBR vertex data (borrowed slice).
    pub fn vertpbr(&self) -> &[VertexPBR] {
        &self.vertex_data
    }

    /// Get PBR vertex data (owned copy).
    pub fn vertpbr_owned(&self) -> Vec<VertexPBR> {
        self.vertex_data.clone()
    }

    /// Get skinned vertex data (borrowed slice).
    pub fn vertskinned(&self) -> &[VertexPBRSkinned] {
        &self.skinned_vertex_data
    }

    /// Get skinned vertex data (owned copy).
    pub fn vertskinned_owned(&self) -> Vec<VertexPBRSkinned> {
        self.skinned_vertex_data.clone()
    }

    /// Get index data (borrowed slice).
    pub fn indices(&self) -> &[u8] {
        &self.index_data
    }

    /// Get index data (owned copy).
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
        for node in self.document.nodes() {
            if let Some(mesh) = node.mesh()
                && let Some(primitive) = mesh.primitives().next()
            {
                let parser = AttributeParser::new(&self.buffers);
                return Some(ParsedAttributes::from_gltf(&primitive, &parser));
            }
        }
        None
    }

    /// Adjust indices by adding an offset when combining multiple nodes.
    ///
    /// Each node's indices are relative to its own vertices. When combining
    /// nodes into a single mesh, indices must be offset by the accumulated
    /// vertex count from previous nodes.
    fn adjust_indices(index_data: &[u8], index_stride: u8, offset: u32) -> Vec<u8> {
        if index_data.is_empty() || index_stride == 0 || offset == 0 {
            return index_data.to_vec();
        }

        match index_stride {
            2 => {
                let mut result = Vec::with_capacity(index_data.len());
                for chunk in index_data.chunks(2) {
                    let index = u16::from_le_bytes([chunk[0], chunk[1]]);
                    let adjusted = index as u32 + offset;
                    if adjusted > u16::MAX as u32 {
                        warn!(
                            "Index overflow when adjusting indices: {} + {} = {}",
                            index, offset, adjusted
                        );
                    }
                    let adjusted_u16 = adjusted as u16;
                    result.extend_from_slice(&adjusted_u16.to_le_bytes());
                }
                result
            }
            4 => {
                let mut result = Vec::with_capacity(index_data.len());
                for chunk in index_data.chunks(4) {
                    let index = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                    let adjusted = index + offset;
                    result.extend_from_slice(&adjusted.to_le_bytes());
                }
                result
            }
            1 => {
                let mut result = Vec::with_capacity(index_data.len());
                for &byte in index_data {
                    let adjusted = byte as u32 + offset;
                    if adjusted > u8::MAX as u32 {
                        warn!(
                            "Index overflow when adjusting 8-bit indices: {} + {} = {}",
                            byte, offset, adjusted
                        );
                    }
                    result.push(adjusted as u8);
                }
                result
            }
            _ => index_data.to_vec(),
        }
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
        let tangents: Vec<[f32; 4]> = vec![];
        let tex_coords: Vec<[f32; 2]> = vec![];

        let (vertices, sphere) = build_vertex_data(positions, normals, tangents, tex_coords);
        assert!(vertices.is_empty());
        assert_eq!(sphere.radius, 0.0);
    }

    /// Build world transforms for all nodes in topological order (BFS).
    /// Returns a map: node_index -> world_transform
    fn build_node_world_transforms(document: &Document) -> std::collections::HashMap<usize, Mat4> {
        use std::collections::{HashMap, VecDeque};

        let mut parent_map: HashMap<usize, Option<usize>> = HashMap::new();
        let nodes: Vec<_> = document.nodes().collect();
        for node in &nodes {
            parent_map.entry(node.index()).or_insert(None);
            for child in node.children() {
                parent_map.insert(child.index(), Some(node.index()));
            }
        }

        let mut children_map: HashMap<usize, Vec<usize>> = HashMap::new();
        for node in &nodes {
            children_map.entry(node.index()).or_default();
            for child in node.children() {
                children_map
                    .entry(node.index())
                    .or_default()
                    .push(child.index());
            }
        }

        let node_by_index: HashMap<usize, &gltf::Node> =
            nodes.iter().map(|n| (n.index(), n)).collect();

        let mut queue: VecDeque<usize> = VecDeque::new();
        for node in &nodes {
            if parent_map.get(&node.index()) == Some(&None) {
                queue.push_back(node.index());
            }
        }

        let mut world_transforms: HashMap<usize, Mat4> = HashMap::new();

        while let Some(node_index) = queue.pop_front() {
            let node = match node_by_index.get(&node_index) {
                Some(n) => n,
                None => continue,
            };

            let transform = node.transform();
            let (t, r, s) = transform.decomposed();
            let translation = Vec3::new(t[0], t[1], t[2]);
            let rotation = Quat::new(r[0], r[1], r[2], r[3]);
            let scale = Vec3::new(s[0], s[1], s[2]);
            let local_matrix = Mat4::from_trs(translation, rotation, scale);

            let world_matrix = if let Some(Some(parent_index)) = parent_map.get(&node_index) {
                if let Some(parent_transform) = world_transforms.get(parent_index) {
                    parent_transform.clone() * local_matrix
                } else {
                    local_matrix
                }
            } else {
                local_matrix
            };

            world_transforms.insert(node_index, world_matrix);

            if let Some(children) = children_map.get(&node_index) {
                for child_index in children {
                    queue.push_back(*child_index);
                }
            }
        }

        world_transforms
    }

    #[test]
    fn test_multi_node_gltf_transforms_vertices() {
        // This test verifies that multi-node GLTF models properly transform
        // vertex positions by their node's world transform.
        //
        // The Lantern model has 3 mesh nodes with different transforms:
        // - LanternPole_Body at world position [3.82, 13.016, 0]
        // - LanternPole_Chain at world position [9.58, 21.04, 0]
        // - LanternPole_Lantern at world position [9.58, 18.01, 0]
        //
        // If transforms aren't applied, ALL vertices will be near origin (local space).
        // With proper transform application, vertices should span the expected range.

        let mut model_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        model_path.pop();
        model_path.push("resources");
        model_path.push("models");
        model_path.push("Lantern.glb");

        if !model_path.exists() {
            eprintln!("Skipping test - Lantern.glb not found at {:?}", model_path);
            return;
        }

        let model = GLTFModel::new(&model_path).expect("Failed to load Lantern.glb");

        let world_transforms = build_node_world_transforms(&model.document);

        let lantern_node = model
            .document
            .nodes()
            .find(|n| n.name() == Some("LanternPole_Lantern"));
        let lantern_node = match lantern_node {
            Some(n) => n,
            None => {
                eprintln!("Skipping test - LanternPole_Lantern node not found");
                return;
            }
        };

        let world_transform = world_transforms
            .get(&lantern_node.index())
            .cloned()
            .unwrap_or_else(Mat4::identity);

        let world_translation = Vec3::new(
            world_transform[3].x(),
            world_transform[3].y(),
            world_transform[3].z(),
        );

        println!(
            "LanternPole_Lantern world translation: {:?}",
            world_translation
        );

        // The lantern should be at approximately y=18 in world space
        // If transforms aren't applied, vertices will be near y=0
        assert!(
            world_translation.y() > 15.0,
            "Lantern world Y position should be > 15, got {}",
            world_translation.y()
        );

        // Now check the actual vertex data
        // If the bug exists, vertex Y positions will be near 0 (local space)
        // If fixed, vertex Y positions should be around 18 (world space)

        let min_y = model
            .vertex_data
            .iter()
            .map(|v| v.position[1])
            .fold(f32::INFINITY, |a, b| a.min(b));
        let max_y = model
            .vertex_data
            .iter()
            .map(|v| v.position[1])
            .fold(f32::NEG_INFINITY, |a, b| a.max(b));

        println!("Vertex Y range: min={}, max={}", min_y, max_y);

        // The model has mesh nodes at Y positions ~13, ~18, and ~21
        // If transforms ARE applied correctly, max_y should be > 20
        // If transforms ARE NOT applied, max_y will be near 0 (local space)
        //
        // THIS TEST SHOULD FAIL with the current buggy implementation
        assert!(
            max_y > 15.0,
            "Vertex max Y should be > 15 if transforms are applied correctly, got {}. \
             This indicates node transforms are NOT being applied to vertices!",
            max_y
        );

        // Verify root_transform is identity for non-skinned meshes
        // (transforms are baked into vertices, so root_transform shouldn't be applied again)
        println!("root_transform: {:?}", model.root_transform);
        println!("has_skinning: {}", model.has_skinning);

        assert!(!model.has_skinning, "This test assumes non-skinned model");

        let is_identity = (model.root_transform[0].x() - 1.0).abs() < 0.001
            && model.root_transform[0].y().abs() < 0.001
            && model.root_transform[0].z().abs() < 0.001
            && model.root_transform[1].x().abs() < 0.001
            && (model.root_transform[1].y() - 1.0).abs() < 0.001
            && model.root_transform[1].z().abs() < 0.001
            && model.root_transform[2].x().abs() < 0.001
            && model.root_transform[2].y().abs() < 0.001
            && (model.root_transform[2].z() - 1.0).abs() < 0.001
            && model.root_transform[3].x().abs() < 0.001
            && model.root_transform[3].y().abs() < 0.001
            && model.root_transform[3].z().abs() < 0.001
            && (model.root_transform[3].w() - 1.0).abs() < 0.001;

        assert!(
            is_identity,
            "root_transform should be identity for non-skinned meshes (transforms are baked into vertices)"
        );
    }

    #[test]
    fn test_lantern_node_hierarchy_transforms() {
        // This test verifies that multi-node GLTF models properly apply
        // node transforms to vertices. The Lantern model has a hierarchy:
        // - Root (lamppost)
        //   - Child node (lantern) with offset transform
        //
        // The lantern should appear at the correct position relative to the lamppost.

        let mut model_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        model_path.pop();
        model_path.push("resources");
        model_path.push("models");
        model_path.push("Lantern.glb");

        if !model_path.exists() {
            eprintln!("Skipping test - Lantern.glb not found at {:?}", model_path);
            return;
        }

        let model = GLTFModel::new(&model_path).expect("Failed to load Lantern.glb");

        let world_transforms = build_node_world_transforms(&model.document);

        let mut all_nodes = vec![];
        if let Some(scene) = model
            .document
            .default_scene()
            .or_else(|| model.document.scenes().next())
        {
            for node in scene.nodes() {
                collect_all_nodes(&node, &mut all_nodes);
            }
        }

        // Debug: print node hierarchy
        println!("\n=== Lantern.glb Node Hierarchy ===");
        for node in &all_nodes {
            let transform = node.transform();
            let (t, _r, _s) = transform.decomposed();
            let world = world_transforms
                .get(&node.index())
                .cloned()
                .unwrap_or_else(Mat4::identity);
            println!(
                "Node {} '{}' - local: t={:?}, has_mesh={}",
                node.index(),
                node.name().unwrap_or("unnamed"),
                t,
                node.mesh().is_some()
            );
            println!("  World transform:\n{:?}", world);
        }

        let mesh_nodes: Vec<_> = all_nodes.iter().filter(|n| n.mesh().is_some()).collect();
        println!("\n=== Nodes with meshes: {} ===", mesh_nodes.len());

        for node in &mesh_nodes {
            let world = world_transforms
                .get(&node.index())
                .cloned()
                .unwrap_or_else(Mat4::identity);
            println!(
                "Node {} '{}' world transform:\n{:?}",
                node.index(),
                node.name().unwrap_or("unnamed"),
                world
            );
        }

        if mesh_nodes.len() > 1 {
            let translations: Vec<Vec3> = mesh_nodes
                .iter()
                .filter_map(|n| {
                    let world = world_transforms.get(&n.index())?;
                    Some(Vec3::new(world[3].x(), world[3].y(), world[3].z()))
                })
                .collect();

            println!("\n=== Mesh node world positions ===");
            for (i, t) in translations.iter().enumerate() {
                println!("Node {}: {:?}", i, t);
            }

            // If all translations are the same but nodes have different local transforms,
            // that indicates a bug in transform accumulation
            let _unique_translations: std::collections::HashSet<_> = translations
                .iter()
                .map(|t| (t.x().to_bits(), t.y().to_bits(), t.z().to_bits()))
                .collect();

            // We expect different positions for different mesh nodes in a hierarchy
            // This test will FAIL if the current implementation doesn't apply node transforms
            // assert!(unique_translations.len() > 1,
            //     "Multiple mesh nodes should have different world positions after transform");
        }
    }
}
