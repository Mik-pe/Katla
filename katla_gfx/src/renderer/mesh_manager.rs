//! Mesh manager for mesh creation.
//!
//! MeshManager provides a clean internal API for creating meshes.
//! This module organizes mesh-related functionality away from VulkanRenderer.
//! Meshes are stored in the shared AssetRegistry for compatibility with drawing code.

use crate::handle::MeshHandle;
use crate::renderer::registry::{
    AssetRegistry, MeshAsset, MeshDescriptor, MeshIndexElement, MeshUsage, PrimitiveTopology,
};
use crate::vertex::Vertex;
use crate::vulkan::vertex_attribute::AttributeType;
use crate::vulkan::{IndexBuffer, IndexType, VertexBuffer};
use crate::{RendererError, VulkanContext};
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
        descriptor: MeshDescriptor,
    ) -> MeshAsset {
        MeshAsset {
            attribute_buffers,
            index_buffer,
            index_format: descriptor.index_format,
            vertex_count: descriptor.vertex_count,
            layout: descriptor.layout,
            topology: descriptor.topology,
            usage: descriptor.usage,
        }
    }

    // ========================================================================
    // Generic Mesh Creation
    // ========================================================================

    /// Create a mesh from vertex and index data.
    ///
    /// The vertex type's trusted [`Vertex`] implementation declares the
    /// layout and attribute semantics; the index width comes from
    /// [`MeshIndexElement`]. Nothing is guessed from byte shapes: unknown
    /// shapes cannot arrive (the trait bound), and inconsistent declarations
    /// fail with [`RendererError::InvalidDescriptor`] before any GPU upload.
    ///
    /// # Arguments
    /// * `registry` - The asset registry to store the mesh in
    /// * `vertices` - Vertex data (a [`Vertex`] implementation)
    /// * `indices` - Index data (`u16` or `u32`)
    /// * `topology` - Primitive topology (only `TriangleList` encodable)
    ///
    /// # Returns
    /// A `MeshHandle` that references the registered mesh, or a typed error.
    /// Failed creation registers nothing.
    pub(crate) fn create_mesh<T, U>(
        &self,
        registry: &mut AssetRegistry,
        vertices: &[T],
        indices: &[U],
        topology: PrimitiveTopology,
    ) -> Result<MeshHandle, RendererError>
    where
        T: Vertex,
        U: MeshIndexElement,
    {
        let descriptor =
            MeshDescriptor::describe_typed_upload(topology, MeshUsage::Static, vertices, indices)?;

        // Deinterleave the trusted layout into SOA attribute buffers.
        let vertex_bytes = bytemuck::cast_slice(vertices);
        let attribute_buffers = self.deinterleave(&descriptor, vertex_bytes)?;

        // Create index buffer
        let index_bytes = unsafe {
            std::slice::from_raw_parts(
                indices.as_ptr() as *const u8,
                std::mem::size_of_val(indices),
            )
        };

        let index_type = IndexType::from(U::INDEX_FORMAT);

        let index_count = index_bytes.len() as u32 / U::INDEX_FORMAT.size();

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

        let mesh_asset = self.create_mesh_asset(attribute_buffers, index_buffer, descriptor);
        Ok(registry.register_mesh(mesh_asset))
    }

    /// Validate every index against the vertex count before upload.
    ///
    /// An out-of-range index would read another mesh's object data (or
    /// uninitialized memory) on the GPU while rendering successfully.
    fn validate_index_range<U>(
        &self,
        descriptor: &MeshDescriptor,
        indices: &[U],
    ) -> Result<(), RendererError>
    where
        U: MeshIndexElement,
    {
        for (position, index) in indices.iter().enumerate() {
            let value = index.to_u32();
            if value >= descriptor.vertex_count {
                return Err(RendererError::InvalidDescriptor {
                    resource: "mesh".to_string(),
                    reason: format!(
                        "index {position} references vertex {value} of {}",
                        descriptor.vertex_count
                    ),
                });
            }
        }
        Ok(())
    }

    /// Deinterleave one interleaved vertex blob into per-attribute buffers
    /// using the descriptor's explicit layout and semantics.
    ///
    /// Offsets derive from the layout formats; every attribute range is
    /// checked against the stride before slicing.
    fn deinterleave(
        &self,
        descriptor: &MeshDescriptor,
        vertex_bytes: &[u8],
    ) -> Result<HashMap<AttributeType, VertexBuffer>, RendererError> {
        let stride = descriptor.layout.stride();
        let vertex_count = descriptor.vertex_count as usize;
        if vertex_bytes.len() != vertex_count * stride {
            return Err(RendererError::InvalidDescriptor {
                resource: "mesh".to_string(),
                reason: format!(
                    "vertex blob {} bytes disagrees with {vertex_count} vertices of stride {stride}",
                    vertex_bytes.len()
                ),
            });
        }
        let mut map = HashMap::new();
        let mut offset = 0usize;
        for (format, attribute) in descriptor
            .layout
            .formats()
            .iter()
            .zip(descriptor.attributes.iter())
        {
            let size = format.size_bytes();
            if offset + size > stride {
                return Err(RendererError::InvalidDescriptor {
                    resource: "mesh".to_string(),
                    reason: format!(
                        "attribute {attribute:?} range {offset}..{} exceeds stride {stride}",
                        offset + size
                    ),
                });
            }
            let mut bytes = Vec::with_capacity(vertex_count * size);
            for vertex in vertex_bytes.chunks_exact(stride) {
                bytes.extend_from_slice(&vertex[offset..offset + size]);
            }
            map.insert(*attribute, self.create_attr_buffer(&bytes));
            offset += size;
        }
        Ok(map)
    }

    fn create_attr_buffer(&self, bytes: &[u8]) -> VertexBuffer {
        let mut vb = VertexBuffer::new(self.context.clone(), bytes.len() as u64, 0);
        vb.upload_data(bytes);
        vb
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
    ) -> Result<MeshHandle, RendererError> {
        // The layout derives from the declared attribute semantics in
        // canonical order — the same vocabulary interleaved upload uses.
        // Empty buffers carry no data and are filtered before the
        // descriptor is built.
        let kinds: Vec<AttributeType> = {
            let mut kinds: Vec<AttributeType> = attributes
                .iter()
                .filter(|(_, data)| !data.is_empty())
                .map(|(kind, _)| *kind)
                .collect();
            kinds.sort_by_key(|kind| kind.default_location());
            kinds
        };
        let descriptor = MeshDescriptor {
            layout: crate::vertex::VertexLayout::for_attributes(&kinds),
            attributes: kinds,
            topology: PrimitiveTopology::TriangleList,
            usage: MeshUsage::Static,
            vertex_count,
            index_count: indices.len() as u32,
            index_format: crate::backend::command::IndexType::Uint32,
        };
        // Stride check needs a concrete stride; SOA data arrives split, so
        // validate per-attribute lengths against the layout instead.
        descriptor.validate_split(attributes)?;
        self.validate_index_range(&descriptor, indices)?;

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

        let mesh_asset = self.create_mesh_asset(attribute_buffers, index_buffer, descriptor);
        Ok(registry.register_mesh(mesh_asset))
    }

    // ========================================================================
    // Primitive Meshes
    // ========================================================================

    /// Create a cube mesh with the given size.
    pub(crate) fn create_cube(
        &self,
        registry: &mut AssetRegistry,
        size: [f32; 3],
    ) -> Result<MeshHandle, RendererError> {
        let (vertices, indices) = crate::primitives::generate_cube(size);
        self.create_mesh(
            registry,
            &vertices,
            &indices,
            PrimitiveTopology::TriangleList,
        )
    }

    /// Create a UV sphere mesh.
    pub(crate) fn create_sphere(
        &self,
        registry: &mut AssetRegistry,
        radius: f32,
        segments: u32,
        rings: u32,
    ) -> Result<MeshHandle, RendererError> {
        let (vertices, indices) = crate::primitives::generate_sphere(radius, segments, rings);
        self.create_mesh(
            registry,
            &vertices,
            &indices,
            PrimitiveTopology::TriangleList,
        )
    }

    /// Create a plane mesh on the XZ plane.
    pub(crate) fn create_plane(
        &self,
        registry: &mut AssetRegistry,
        width: f32,
        height: f32,
    ) -> Result<MeshHandle, RendererError> {
        let (vertices, indices) = crate::primitives::generate_plane(width, height);
        self.create_mesh(
            registry,
            &vertices,
            &indices,
            PrimitiveTopology::TriangleList,
        )
    }

    /// Create a cone mesh with base at y=0 and apex at y=height.
    pub(crate) fn create_cone(
        &self,
        registry: &mut AssetRegistry,
        height: f32,
        base_radius: f32,
        segments: u32,
    ) -> Result<MeshHandle, RendererError> {
        let (vertices, indices) = crate::primitives::generate_cone(height, base_radius, segments);
        self.create_mesh(
            registry,
            &vertices,
            &indices,
            PrimitiveTopology::TriangleList,
        )
    }

    /// Create a cylinder mesh standing on Y axis.
    pub(crate) fn create_cylinder(
        &self,
        registry: &mut AssetRegistry,
        height: f32,
        radius: f32,
        segments: u32,
    ) -> Result<MeshHandle, RendererError> {
        let (vertices, indices) = crate::primitives::generate_cylinder(height, radius, segments);
        self.create_mesh(
            registry,
            &vertices,
            &indices,
            PrimitiveTopology::TriangleList,
        )
    }

    /// Create a torus (donut) mesh on the XZ plane.
    pub(crate) fn create_torus(
        &self,
        registry: &mut AssetRegistry,
        major_radius: f32,
        minor_radius: f32,
        segments: u32,
        rings: u32,
    ) -> Result<MeshHandle, RendererError> {
        let (vertices, indices) =
            crate::primitives::generate_torus(major_radius, minor_radius, segments, rings);
        self.create_mesh(
            registry,
            &vertices,
            &indices,
            PrimitiveTopology::TriangleList,
        )
    }

    /// Create a plane on the XY axis (vertical, facing +Z).
    pub(crate) fn create_plane_xy(
        &self,
        registry: &mut AssetRegistry,
        width: f32,
        height: f32,
        segments: u32,
    ) -> Result<MeshHandle, RendererError> {
        let (vertices, indices) = crate::primitives::generate_plane_xy(width, height, segments);
        self.create_mesh(
            registry,
            &vertices,
            &indices,
            PrimitiveTopology::TriangleList,
        )
    }

    // ========================================================================
    // Dynamic Meshes
    // ========================================================================

    /// Create a dynamic mesh from an explicit descriptor plus raw blobs.
    ///
    /// Unlike [`MeshManager::create_mesh`], the vertex bytes arrive without a
    /// Rust type, so the caller supplies the descriptor (layout, semantics,
    /// topology, counts) and this method validates the blobs against it
    /// before upload. Usage is always [`MeshUsage::Dynamic`].
    pub(crate) fn create_mesh_dynamic(
        &self,
        registry: &mut AssetRegistry,
        descriptor: &MeshDescriptor,
        vertex_data: &[u8],
        indices: &[u32],
    ) -> Result<MeshHandle, RendererError> {
        if descriptor.topology != PrimitiveTopology::TriangleList {
            return Err(RendererError::UnsupportedFeature(format!(
                "mesh topology {:?} is not encodable; only TriangleList is supported",
                descriptor.topology
            )));
        }
        if descriptor.usage != MeshUsage::Dynamic {
            return Err(RendererError::InvalidDescriptor {
                resource: "mesh".to_string(),
                reason: "dynamic upload requires MeshUsage::Dynamic".to_string(),
            });
        }
        let stride = descriptor.layout.stride();
        if vertex_data.len() != descriptor.vertex_count as usize * stride {
            return Err(RendererError::InvalidDescriptor {
                resource: "mesh".to_string(),
                reason: format!(
                    "dynamic vertex blob {} bytes disagrees with {} vertices of stride {stride}",
                    vertex_data.len(),
                    descriptor.vertex_count
                ),
            });
        }
        if indices.len() as u32 != descriptor.index_count {
            return Err(RendererError::InvalidDescriptor {
                resource: "mesh".to_string(),
                reason: format!(
                    "dynamic index count {} disagrees with descriptor count {}",
                    indices.len(),
                    descriptor.index_count
                ),
            });
        }
        let descriptor = MeshDescriptor {
            usage: MeshUsage::Dynamic,
            ..descriptor.clone()
        };
        descriptor.validate(descriptor.layout.stride())?;
        self.validate_index_range(&descriptor, indices)?;
        let attribute_buffers = self.deinterleave(&descriptor, vertex_data)?;

        // Create index buffer (always u32 for dynamic meshes)
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

        let mesh_asset = self.create_mesh_asset(attribute_buffers, index_buffer, descriptor);
        Ok(registry.register_mesh(mesh_asset))
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
            .ok_or_else(|| RendererError::StaleHandle {
                resource: "mesh".to_string(),
                detail: format!("{mesh:?} in update_mesh_dynamic"),
            })?;

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
