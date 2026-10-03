//! Selected-scene glTF geometry with independent primitive and material identity.

use crate::util::gltf_material::GltfMaterialInfo;
use gltf::{Document, buffer::Data as BufferData, image::Data as ImageData};
use katla_gfx::{VertexPBR, VertexPBRSkinned};
use katla_math::{AABB, Mat4, Vec3};
use std::{collections::HashMap, path::Path};

/// Vertex data for one glTF primitive, with its skin attributes kept together.
#[derive(Clone)]
pub enum GltfVertices {
    Static(Vec<VertexPBR>),
    Skinned(Vec<VertexPBRSkinned>),
}

impl GltfVertices {
    /// Positions in model space for static geometry or mesh space for skinned geometry.
    pub fn positions(&self) -> Vec<[f32; 3]> {
        match self {
            Self::Static(vertices) => vertices.iter().map(|v| v.position).collect(),
            Self::Skinned(vertices) => vertices.iter().map(|v| v.position).collect(),
        }
    }
}

/// One selected-scene node's primitive, retaining its own indices, surface and skin.
#[derive(Clone)]
pub struct GltfPrimitive {
    /// Document node identity within the selected scene.
    pub node_index: usize,
    /// Index within this node's mesh primitive list.
    pub primitive_index: usize,
    /// Editor label derived from the node and primitive identity.
    pub name: String,
    /// This primitive's assigned or implicit glTF surface.
    pub material: GltfMaterialInfo,
    /// Geometry and skin attributes belonging exclusively to this primitive.
    pub vertices: GltfVertices,
    /// Triangle-list indices into this primitive's vertex array.
    pub indices: Vec<u32>,
    /// Static baked bounds or the skinned mesh's undeformed bounds.
    pub bounds: AABB,
    /// Skin selected by the primitive's node.
    pub skin_index: Option<usize>,
}

/// Decoded glTF resources and the selected scene's independently renderable primitives.
#[derive(Clone)]
pub struct GLTFModel {
    /// Original document for animation and asset identity.
    pub document: Document,
    /// Decoded accessor buffers.
    pub buffers: Vec<BufferData>,
    /// Decoded images referenced by texture roles.
    pub images: Vec<ImageData>,
    /// Selected-scene primitives in depth-first node order.
    pub primitives: Vec<GltfPrimitive>,
}

/// Build world transforms for all nodes in topological order (BFS).
///
/// Returns a map: node_index -> world_transform.
/// Processes parent nodes before children so each node accumulates its parent's transform.
pub fn build_world_transforms(nodes: &[gltf::Node]) -> HashMap<usize, Mat4> {
    use std::collections::{HashMap, VecDeque};

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

    let node_by_index: HashMap<usize, &gltf::Node> = nodes.iter().map(|n| (n.index(), n)).collect();

    let mut queue: VecDeque<usize> = VecDeque::new();
    for node in nodes {
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

        let columns = node.transform().matrix();
        let local_matrix = Mat4(columns.map(katla_math::Vec4::from));

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

impl GLTFModel {
    /// Bake affine positions and inverse-transpose surface frames into static geometry.
    pub(crate) fn transform_vertex_data(vertices: &mut [VertexPBR], world_transform: &Mat4) {
        use katla_math::{Mat3, Vec4};

        let transform = Mat3::from(*world_transform);
        let cofactors = Mat3::from_columns(
            transform[1].cross(transform[2]),
            transform[2].cross(transform[0]),
            transform[0].cross(transform[1]),
        );
        let orientation = if transform[0].dot(cofactors[0]) < 0.0 {
            -1.0
        } else {
            1.0
        };
        let normalized_or = |value: Vec3, fallback: Vec3| {
            let magnitude_sq = value.dot(value);
            if magnitude_sq > 1e-12 {
                value / magnitude_sq.sqrt()
            } else {
                fallback
            }
        };
        for vertex in vertices {
            let position = vertex.position;
            let world = *world_transform * Vec4::new(position[0], position[1], position[2], 1.0);
            vertex.position = [world.x(), world.y(), world.z()];
            let normal = Vec3::new(vertex.normal[0], vertex.normal[1], vertex.normal[2]);
            let normal = normalized_or(
                cofactors * normal * orientation,
                normalized_or(transform * normal, Vec3::Y_AXIS),
            );
            let tangent =
                transform * Vec3::new(vertex.tangent[0], vertex.tangent[1], vertex.tangent[2]);
            let axis = if normal.x().abs() > 0.9 {
                Vec3::Z_AXIS
            } else {
                Vec3::X_AXIS
            };
            let tangent = normalized_or(
                tangent - normal * normal.dot(tangent),
                axis.cross(normal).normalize(),
            );
            vertex.normal = normal.to_array();
            vertex.tangent = [
                tangent.x(),
                tangent.y(),
                tangent.z(),
                vertex.tangent[3] * orientation,
            ];
        }
    }

    /// Import the default scene, or the first scene when no default is specified.
    pub fn new(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let (document, buffers, images) = gltf::import(path)?;
        Self::from_parts(document, buffers, images).map_err(Into::into)
    }

    pub(crate) fn from_parts(
        document: Document,
        buffers: Vec<BufferData>,
        images: Vec<ImageData>,
    ) -> Result<Self, String> {
        let mut nodes = Vec::new();
        if let Some(scene) = document
            .default_scene()
            .or_else(|| document.scenes().next())
        {
            let mut pending: Vec<_> = scene.nodes().collect();
            pending.reverse();
            let mut visited = std::collections::HashSet::new();
            while let Some(node) = pending.pop() {
                if !visited.insert(node.index()) {
                    return Err(format!(
                        "Selected scene repeats node {}; cycles and shared parents are unsupported",
                        node.index()
                    ));
                }
                let mut children: Vec<_> = node.children().collect();
                children.reverse();
                pending.extend(children);
                nodes.push(node);
            }
        }
        let transforms = build_world_transforms(&nodes);
        let mut primitives = Vec::new();
        for node in nodes {
            if let Some(skin) = node.skin() {
                if skin
                    .joints()
                    .any(|joint| !transforms.contains_key(&joint.index()))
                {
                    return Err(format!(
                        "Node {} skin refers to a joint outside the selected scene",
                        node.index()
                    ));
                }
                if skin
                    .reader(|buffer| buffers.get(buffer.index()).map(|data| &data.0[..]))
                    .read_inverse_bind_matrices()
                    .is_some_and(|values| values.count() != skin.joints().count())
                {
                    return Err(format!(
                        "Node {} inverse bind matrix count differs from its skin",
                        node.index()
                    ));
                }
            }
            if let Some(mesh) = node.mesh() {
                for (primitive_index, primitive) in mesh.primitives().enumerate() {
                    primitives.push(
                        super::gltf_primitive::decode(
                            &buffers,
                            &node,
                            primitive_index,
                            &primitive,
                            transforms[&node.index()],
                        )
                        .map_err(|error| {
                            format!("node {} primitive {primitive_index}: {error}", node.index())
                        })?,
                    );
                }
            }
        }
        Ok(Self {
            document,
            buffers,
            images,
            primitives,
        })
    }
}

#[cfg(test)]
#[path = "modelcache_tests.rs"]
mod tests;
