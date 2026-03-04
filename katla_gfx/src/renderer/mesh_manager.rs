//! Mesh manager for mesh creation.
//!
//! MeshManager provides a clean internal API for creating meshes.
//! This module organizes mesh-related functionality away from VulkanRenderer.
//! Meshes are stored in the shared AssetRegistry for compatibility with drawing code.

use crate::handle::MeshHandle;
use crate::renderer::registry::{AssetRegistry, MeshAsset};
use crate::vulkan::{IndexBuffer, IndexType, VertexBuffer};
use crate::{RendererError, VulkanContext};
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
        vertex_buffer: Option<VertexBuffer>,
        index_buffer: Option<IndexBuffer>,
    ) -> MeshAsset {
        MeshAsset {
            vertex_buffer,
            index_buffer,
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
        // Convert vertices to bytes
        let vertex_bytes = unsafe {
            std::slice::from_raw_parts(
                vertices.as_ptr() as *const u8,
                std::mem::size_of_val(vertices),
            )
        };

        // Convert indices to bytes
        let index_bytes = unsafe {
            std::slice::from_raw_parts(
                indices.as_ptr() as *const u8,
                std::mem::size_of_val(indices),
            )
        };

        // Determine index type
        let index_type = match std::mem::size_of::<U>() {
            1 => IndexType::Uint8,
            2 => IndexType::Uint16,
            4 => IndexType::Uint32,
            _ => IndexType::None,
        };

        // Determine index count
        let index_count = match index_type {
            IndexType::Uint8 => index_bytes.len() as u32,
            IndexType::Uint16 => (index_bytes.len() as u32) / 2,
            IndexType::Uint32 => (index_bytes.len() as u32) / 4,
            IndexType::None => 0_u32,
        };

        // Create vertex buffer and upload data
        let vertex_buffer = if !vertex_bytes.is_empty() {
            let mut vb = VertexBuffer::new(
                self.context.clone(),
                vertex_bytes.len() as u64,
                vertices.len() as u32,
            );
            vb.upload_data(vertex_bytes);
            Some(vb)
        } else {
            None
        };

        // Create index buffer and upload data
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

        let mesh_asset = self.create_mesh_asset(vertex_buffer, index_buffer);
        registry.register_mesh(mesh_asset)
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
        let mesh_asset = self.create_mesh_asset(vertex_buffer, index_buffer);
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

        let mesh_asset = self.create_mesh_asset(vertex_buffer, index_buffer);
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
        if let Some(ref mut vb) = mesh_asset.vertex_buffer {
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
