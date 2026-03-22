//! Mesh manager for mesh creation.
//!
//! MeshManager provides a clean internal API for creating meshes.
//! This module organizes mesh-related functionality away from VulkanRenderer.
//! Meshes are stored in the shared AssetRegistry for compatibility with drawing code.

use crate::handle::MeshHandle;
use crate::renderer::registry::{AssetRegistry, MeshAsset};
use crate::vertex::{VertexPBR, VertexPBRSkinned};
use crate::vulkan::vertex_attribute::AttributeType;
use crate::vulkan::{IndexBuffer, IndexType, VertexBuffer};
use crate::{RendererError, VulkanContext};
use std::any::TypeId;
use std::collections::HashMap;
use std::rc::Rc;

/// Mesh manager for creating meshes.
///
/// Handles all mesh creation including primitive generators and dynamic mesh updates.
/// Meshes are stored in the shared AssetRegistry.
pub(crate) struct MeshManager {
    /// Vulkan context for buffer creation.
    context: Rc<VulkanContext>,
}

impl MeshManager {
    /// Create a new mesh manager.
    pub(crate) fn new(context: Rc<VulkanContext>) -> Self {
        Self { context }
    }

    /// Create a mesh using the shared asset registry.
    fn create_mesh_asset(
        &self,
        attribute_buffers: HashMap<AttributeType, VertexBuffer>,
        index_buffer: Option<IndexBuffer>,
        vertex_count: u32,
    ) -> MeshAsset {
        MeshAsset {
            attribute_buffers,
            index_buffer,
            vertex_count,
        }
    }

    // ========================================================================
    // Generic Mesh Creation
    // ========================================================================

    /// Create a mesh from vertex and index data.
    ///
    /// # Arguments
    /// * `registry` - The asset registry to store the mesh in
    /// * `vertices` - Vertex data (any Pod type)
    /// * `indices` - Index data (any Pod type: u8, u16, u32)
    ///
    /// # Returns
    /// A `MeshHandle` that references the registered mesh.
    pub(crate) fn create_mesh<T, U>(
        &self,
        registry: &mut AssetRegistry,
        vertices: &[T],
        indices: &[U],
    ) -> MeshHandle
    where
        T: bytemuck::Pod,
        U: bytemuck::Pod,
    {
        let type_id = TypeId::of::<T>();

        // Deinterleave known vertex types into SOA attribute buffers
        let attribute_buffers = if type_id == TypeId::of::<VertexPBR>() {
            self.deinterleave_pbr(unsafe {
                std::slice::from_raw_parts(vertices.as_ptr() as *const VertexPBR, vertices.len())
            })
        } else if type_id == TypeId::of::<VertexPBRSkinned>() {
            self.deinterleave_pbr_skinned(unsafe {
                std::slice::from_raw_parts(
                    vertices.as_ptr() as *const VertexPBRSkinned,
                    vertices.len(),
                )
            })
        } else {
            // Fallback: store entire blob as position buffer
            let vertex_bytes = unsafe {
                std::slice::from_raw_parts(
                    vertices.as_ptr() as *const u8,
                    std::mem::size_of_val(vertices),
                )
            };
            let mut map = HashMap::new();
            if !vertex_bytes.is_empty() {
                let mut vb = VertexBuffer::new(
                    self.context.clone(),
                    vertex_bytes.len() as u64,
                    vertices.len() as u32,
                );
                vb.upload_data(vertex_bytes);
                map.insert(AttributeType::Position, vb);
            }
            map
        };

        // Create index buffer
        let index_bytes = unsafe {
            std::slice::from_raw_parts(
                indices.as_ptr() as *const u8,
                std::mem::size_of_val(indices),
            )
        };

        let index_type = match std::mem::size_of::<U>() {
            1 => IndexType::Uint8,
            2 => IndexType::Uint16,
            4 => IndexType::Uint32,
            _ => IndexType::None,
        };

        let index_count = match index_type {
            IndexType::Uint8 => index_bytes.len() as u32,
            IndexType::Uint16 => (index_bytes.len() as u32) / 2,
            IndexType::Uint32 => (index_bytes.len() as u32) / 4,
            IndexType::None => 0_u32,
        };

        let index_buffer = if !index_bytes.is_empty() {
            let mut ib = IndexBuffer::new(
                self.context.clone(),
                index_bytes.len() as u64,
                index_type,
                index_count,
            );
            ib.upload_data(index_bytes);
            Some(ib)
        } else {
            None
        };

        let mesh_asset =
            self.create_mesh_asset(attribute_buffers, index_buffer, vertices.len() as u32);
        registry.register_mesh(mesh_asset)
    }

    fn create_attr_buffer(&self, bytes: &[u8]) -> VertexBuffer {
        let mut vb = VertexBuffer::new(self.context.clone(), bytes.len() as u64, 0);
        vb.upload_data(bytes);
        vb
    }

    fn deinterleave_pbr(&self, vertices: &[VertexPBR]) -> HashMap<AttributeType, VertexBuffer> {
        let n = vertices.len();
        let mut map = HashMap::new();

        if n == 0 {
            return map;
        }

        let mut positions = Vec::with_capacity(n * 12);
        let mut normals = Vec::with_capacity(n * 12);
        let mut tangents = Vec::with_capacity(n * 16);
        let mut tex_coords = Vec::with_capacity(n * 8);

        for v in vertices {
            positions.extend_from_slice(bytemuck::bytes_of(&v.position));
            normals.extend_from_slice(bytemuck::bytes_of(&v.normal));
            tangents.extend_from_slice(bytemuck::bytes_of(&v.tangent));
            tex_coords.extend_from_slice(bytemuck::bytes_of(&v.tex_coord0));
        }

        map.insert(AttributeType::Position, self.create_attr_buffer(&positions));
        map.insert(AttributeType::Normal, self.create_attr_buffer(&normals));
        map.insert(AttributeType::Tangent, self.create_attr_buffer(&tangents));
        map.insert(
            AttributeType::TexCoord0,
            self.create_attr_buffer(&tex_coords),
        );

        map
    }

    fn deinterleave_pbr_skinned(
        &self,
        vertices: &[VertexPBRSkinned],
    ) -> HashMap<AttributeType, VertexBuffer> {
        let n = vertices.len();
        let mut map = HashMap::new();

        if n == 0 {
            return map;
        }

        let mut positions = Vec::with_capacity(n * 12);
        let mut normals = Vec::with_capacity(n * 12);
        let mut tangents = Vec::with_capacity(n * 16);
        let mut tex_coords = Vec::with_capacity(n * 8);
        let mut joint_indices = Vec::with_capacity(n * 8);
        let mut joint_weights = Vec::with_capacity(n * 16);

        for v in vertices {
            positions.extend_from_slice(bytemuck::bytes_of(&v.position));
            normals.extend_from_slice(bytemuck::bytes_of(&v.normal));
            tangents.extend_from_slice(bytemuck::bytes_of(&v.tangent));
            tex_coords.extend_from_slice(bytemuck::bytes_of(&v.tex_coord0));
            joint_indices.extend_from_slice(bytemuck::bytes_of(&v.joint_indices));
            joint_weights.extend_from_slice(bytemuck::bytes_of(&v.joint_weights));
        }

        map.insert(AttributeType::Position, self.create_attr_buffer(&positions));
        map.insert(AttributeType::Normal, self.create_attr_buffer(&normals));
        map.insert(AttributeType::Tangent, self.create_attr_buffer(&tangents));
        map.insert(
            AttributeType::TexCoord0,
            self.create_attr_buffer(&tex_coords),
        );
        map.insert(
            AttributeType::JointIndices,
            self.create_attr_buffer(&joint_indices),
        );
        map.insert(
            AttributeType::JointWeights,
            self.create_attr_buffer(&joint_weights),
        );

        map
    }

    /// Register a mesh with pre-existing buffers.
    ///
    /// This is useful when you've already created buffers and want to register them.
    ///
    /// # Arguments
    /// * `registry` - The asset registry to store the mesh in
    /// * `vertex_buffer` - The vertex buffer (or None if no vertices)
    /// * `index_buffer` - The index buffer (or None if no indices)
    ///
    /// # Returns
    /// A `MeshHandle` that references the registered mesh.
    pub(crate) fn register_mesh(
        &self,
        registry: &mut AssetRegistry,
        vertex_buffer: Option<VertexBuffer>,
        index_buffer: Option<IndexBuffer>,
    ) -> MeshHandle {
        let attribute_buffers = vertex_buffer
            .map(|vb| {
                let mut map = HashMap::new();
                map.insert(AttributeType::Position, vb);
                map
            })
            .unwrap_or_default();

        let vertex_count = 0;
        let mesh_asset = self.create_mesh_asset(attribute_buffers, index_buffer, vertex_count);
        registry.register_mesh(mesh_asset)
    }

    /// Create a mesh with separate per-attribute vertex buffers (SOA layout).
    ///
    /// Each attribute type (Position, Normal, Tangent, etc.) gets its own buffer.
    /// Indices are always u32.
    pub(crate) fn create_mesh_soa(
        &self,
        registry: &mut AssetRegistry,
        attributes: &HashMap<AttributeType, Vec<u8>>,
        vertex_count: u32,
        indices: &[u32],
    ) -> MeshHandle {
        let mut attribute_buffers = HashMap::new();

        for (attr_type, data) in attributes {
            if !data.is_empty() {
                let mut vb =
                    VertexBuffer::new(self.context.clone(), data.len() as u64, vertex_count);
                vb.upload_data(data);
                attribute_buffers.insert(*attr_type, vb);
            }
        }

        let index_buffer = if !indices.is_empty() {
            let index_bytes = unsafe {
                std::slice::from_raw_parts(
                    indices.as_ptr() as *const u8,
                    std::mem::size_of_val(indices),
                )
            };
            let mut ib = IndexBuffer::new(
                self.context.clone(),
                index_bytes.len() as u64,
                IndexType::Uint32,
                indices.len() as u32,
            );
            ib.upload_data(index_bytes);
            Some(ib)
        } else {
            None
        };

        let mesh_asset = self.create_mesh_asset(attribute_buffers, index_buffer, vertex_count);
        registry.register_mesh(mesh_asset)
    }

    // ========================================================================
    // Primitive Meshes
    // ========================================================================

    /// Create a cube mesh with the given size.
    pub(crate) fn create_cube(&self, registry: &mut AssetRegistry, size: [f32; 3]) -> MeshHandle {
        let (vertices, indices) = crate::primitives::generate_cube(size);
        self.create_mesh(registry, &vertices, &indices)
    }

    /// Create a UV sphere mesh.
    pub(crate) fn create_sphere(
        &self,
        registry: &mut AssetRegistry,
        radius: f32,
        segments: u32,
        rings: u32,
    ) -> MeshHandle {
        let (vertices, indices) = crate::primitives::generate_sphere(radius, segments, rings);
        self.create_mesh(registry, &vertices, &indices)
    }

    /// Create a plane mesh on the XZ plane.
    pub(crate) fn create_plane(
        &self,
        registry: &mut AssetRegistry,
        width: f32,
        height: f32,
    ) -> MeshHandle {
        let (vertices, indices) = crate::primitives::generate_plane(width, height);
        self.create_mesh(registry, &vertices, &indices)
    }

    /// Create a cylinder mesh standing on Y axis.
    pub(crate) fn create_cylinder(
        &self,
        registry: &mut AssetRegistry,
        height: f32,
        radius: f32,
        segments: u32,
    ) -> MeshHandle {
        let (vertices, indices) = crate::primitives::generate_cylinder(height, radius, segments);
        self.create_mesh(registry, &vertices, &indices)
    }

    /// Create a torus (donut) mesh on the XZ plane.
    pub(crate) fn create_torus(
        &self,
        registry: &mut AssetRegistry,
        major_radius: f32,
        minor_radius: f32,
        segments: u32,
        rings: u32,
    ) -> MeshHandle {
        let (vertices, indices) =
            crate::primitives::generate_torus(major_radius, minor_radius, segments, rings);
        self.create_mesh(registry, &vertices, &indices)
    }

    /// Create a plane on the XY axis (vertical, facing +Z).
    pub(crate) fn create_plane_xy(
        &self,
        registry: &mut AssetRegistry,
        width: f32,
        height: f32,
        segments: u32,
    ) -> MeshHandle {
        let (vertices, indices) = crate::primitives::generate_plane_xy(width, height, segments);
        self.create_mesh(registry, &vertices, &indices)
    }

    // ========================================================================
    // Dynamic Meshes
    // ========================================================================

    /// Create a dynamic mesh from raw vertex and index data.
    pub(crate) fn create_mesh_dynamic(
        &self,
        registry: &mut AssetRegistry,
        vertex_data: &[u8],
        vertex_count: u32,
        indices: &[u32],
    ) -> MeshHandle {
        // Create vertex buffer
        let vertex_buffer = if !vertex_data.is_empty() {
            let mut vb =
                VertexBuffer::new(self.context.clone(), vertex_data.len() as u64, vertex_count);
            vb.upload_data(vertex_data);
            Some(vb)
        } else {
            None
        };

        // Create index buffer (always u32 for UI)
        let index_bytes = unsafe {
            std::slice::from_raw_parts(
                indices.as_ptr() as *const u8,
                std::mem::size_of_val(indices),
            )
        };

        let index_buffer = if !indices.is_empty() {
            let mut ib = IndexBuffer::new(
                self.context.clone(),
                index_bytes.len() as u64,
                IndexType::Uint32,
                indices.len() as u32,
            );
            ib.upload_data(index_bytes);
            Some(ib)
        } else {
            None
        };

        let attribute_buffers = vertex_buffer
            .map(|vb| {
                let mut map = HashMap::new();
                map.insert(AttributeType::Position, vb);
                map
            })
            .unwrap_or_default();

        let mesh_asset = self.create_mesh_asset(attribute_buffers, index_buffer, vertex_count);
        registry.register_mesh(mesh_asset)
    }

    /// Update a dynamic mesh with new vertex and index data.
    pub(crate) fn update_mesh_dynamic(
        &self,
        registry: &mut AssetRegistry,
        mesh: MeshHandle,
        vertex_data: &[u8],
        _vertex_count: u32,
        indices: &[u32],
    ) -> Result<(), RendererError> {
        let mesh_asset = registry
            .get_mesh_mut(mesh)
            .ok_or_else(|| RendererError::NotFound("Mesh handle not found".to_string()))?;

        // Update vertex buffer
        if let Some(ref mut vb) = mesh_asset
            .attribute_buffers
            .get_mut(&AttributeType::Position)
        {
            vb.upload_data(vertex_data);
        }

        // Update index buffer
        if let Some(ref mut ib) = mesh_asset.index_buffer {
            let index_bytes = unsafe {
                std::slice::from_raw_parts(
                    indices.as_ptr() as *const u8,
                    std::mem::size_of_val(indices),
                )
            };
            ib.upload_data(index_bytes);
        }

        Ok(())
    }
}
