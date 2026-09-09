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
use crate::vulkan::retirement::{FrameRetirements, RetiredBuffer};
use crate::vulkan::staged_upload::{BufferPlacement, StagedUploadBatch};
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
            index_count: descriptor.index_count,
            layout: descriptor.layout,
            attributes: descriptor.attributes,
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
        let split = split_attribute_bytes(
            &descriptor.layout,
            &descriptor.attributes,
            vertex_bytes,
            descriptor.vertex_count,
        )?;

        let index_bytes = unsafe {
            std::slice::from_raw_parts(
                indices.as_ptr() as *const u8,
                std::mem::size_of_val(indices),
            )
        };

        let (attribute_buffers, index_buffer) = self.upload_soa_buffers(SoaUpload {
            usage: MeshUsage::Static,
            split: &split,
            vertex_count: descriptor.vertex_count,
            index_bytes,
            index_type: IndexType::from(U::INDEX_FORMAT),
            index_count: index_bytes.len() as u32 / U::INDEX_FORMAT.size(),
        })?;

        let mesh_asset = self.create_mesh_asset(attribute_buffers, index_buffer, descriptor);
        Ok(registry.register_mesh(mesh_asset))
    }

    /// Upload split attribute data plus index bytes under a placement
    /// policy: static meshes go through one batched staged submission into
    /// device-local memory, dynamic meshes take direct host-visible writes.
    fn upload_soa_buffers(
        &self,
        upload: SoaUpload<'_>,
    ) -> Result<(HashMap<AttributeType, VertexBuffer>, Option<IndexBuffer>), RendererError> {
        match BufferPlacement::for_mesh_usage(upload.usage) {
            BufferPlacement::DeviceLocal => {
                // Canonical attribute order keeps the staging layout
                // deterministic across runs.
                let mut kinds: Vec<AttributeType> = upload.split.keys().copied().collect();
                kinds.sort_by_key(|kind| kind.default_location());

                let mut batch = StagedUploadBatch::new(self.context.clone());
                let mut attribute_buffers = HashMap::new();
                for kind in kinds {
                    let bytes = &upload.split[&kind];
                    let vb = batch.push_vertex(bytes, upload.vertex_count)?;
                    attribute_buffers.insert(kind, vb);
                }
                let index_buffer = if upload.index_bytes.is_empty() {
                    None
                } else {
                    Some(batch.push_index(
                        upload.index_bytes,
                        upload.index_type,
                        upload.index_count,
                    )?)
                };
                batch.finish()?;
                Ok((attribute_buffers, index_buffer))
            }
            BufferPlacement::HostVisible => {
                let mut attribute_buffers = HashMap::new();
                for (kind, bytes) in upload.split {
                    attribute_buffers.insert(*kind, self.create_attr_buffer(bytes));
                }
                let index_buffer = if upload.index_bytes.is_empty() {
                    None
                } else {
                    let mut ib = IndexBuffer::new(
                        self.context.clone(),
                        upload.index_bytes.len() as u64,
                        upload.index_type,
                        upload.index_count,
                    );
                    ib.upload_data(upload.index_bytes);
                    Some(ib)
                };
                Ok((attribute_buffers, index_buffer))
            }
        }
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
        let split = split_attribute_bytes(
            &descriptor.layout,
            &descriptor.attributes,
            vertex_bytes,
            descriptor.vertex_count,
        )?;
        Ok(split
            .into_iter()
            .map(|(kind, bytes)| (kind, self.create_attr_buffer(&bytes)))
            .collect())
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

        let filtered: HashMap<AttributeType, Vec<u8>> = attributes
            .iter()
            .filter(|(_, data)| !data.is_empty())
            .map(|(kind, data)| (*kind, data.clone()))
            .collect();
        let index_bytes = unsafe {
            std::slice::from_raw_parts(
                indices.as_ptr() as *const u8,
                std::mem::size_of_val(indices),
            )
        };

        let (attribute_buffers, index_buffer) = self.upload_soa_buffers(SoaUpload {
            usage: MeshUsage::Static,
            split: &filtered,
            vertex_count,
            index_bytes,
            index_type: IndexType::Uint32,
            index_count: indices.len() as u32,
        })?;

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
    ///
    /// Backend contract for both backends (see
    /// `crate::renderer::registry::validate_dynamic_update`):
    ///
    /// - The interleaved `vertex_data` blob must describe exactly
    ///   `vertex_count` vertices of the mesh's recorded layout stride, and
    ///   every index must be in range; violations fail with typed errors
    ///   before any GPU state changes.
    /// - The mesh's logical vertex/index counts are updated to the new
    ///   values; byte capacity only grows. Shrinking and empty updates never
    ///   reallocate — they publish smaller counts and draw correspondingly
    ///   less (an empty mesh draws nothing).
    /// - Growth replaces buffers whose capacity is insufficient. Replacement
    ///   buffers are fully allocated and filled first; the old buffers enter
    ///   `retirement` tagged with the current frame so they stay alive until
    ///   the submissions that can still read them have completed. An
    ///   allocation failure returns [`RendererError::AllocationFailed`] and
    ///   leaves the previously published mesh state untouched.
    /// - The recorded index width never changes: dynamic meshes store `u32`
    ///   indices and updates stay `u32`.
    pub(crate) fn update_mesh_dynamic(
        &self,
        registry: &mut AssetRegistry,
        retirement: &mut FrameRetirements,
        mesh: MeshHandle,
        vertex_data: &[u8],
        vertex_count: u32,
        indices: &[u32],
    ) -> Result<(), RendererError> {
        let (layout, attributes) = {
            let mesh_asset = registry
                .get_mesh(mesh)
                .ok_or_else(|| RendererError::StaleHandle {
                    resource: "mesh".to_string(),
                    detail: format!("{mesh:?} in update_mesh_dynamic"),
                })?;
            if mesh_asset.usage != MeshUsage::Dynamic {
                return Err(RendererError::InvalidOperation(format!(
                    "mesh {mesh:?} is not dynamic; static meshes are immutable"
                )));
            }
            (mesh_asset.layout.clone(), mesh_asset.attributes.clone())
        };

        crate::renderer::registry::validate_dynamic_update(
            layout.stride(),
            vertex_data,
            vertex_count,
            indices,
        )?;

        // Split the interleaved blob per attribute using the recorded layout
        // (creation and update slice identically).
        let split = split_attribute_bytes(&layout, &attributes, vertex_data, vertex_count)?;

        // Fallible phase: allocate and fill every replacement buffer before
        // any mesh state changes. Old buffers stay live and owned by the
        // asset until the commit below.
        let mut replaced_attributes: Vec<(AttributeType, VertexBuffer)> = Vec::new();
        for (kind, bytes) in &split {
            let needed = bytes.len() as u64;
            let existing = registry
                .get_mesh(mesh)
                .and_then(|asset| asset.attribute_buffers.get(kind))
                .map(|vb| vb.capacity());
            let must_replace = existing.is_none_or(|capacity| capacity < needed);
            if !must_replace {
                continue;
            }
            // Amortized growth: at least double the previous capacity so a
            // gradually growing mesh does not reallocate every update.
            let new_capacity = existing.map_or(needed, |old| needed.max(old * 2));
            let mut vb = VertexBuffer::try_new(self.context.clone(), new_capacity, vertex_count)?;
            vb.upload_data(bytes);
            replaced_attributes.push((*kind, vb));
        }

        let index_bytes = unsafe {
            std::slice::from_raw_parts(
                indices.as_ptr() as *const u8,
                std::mem::size_of_val(indices),
            )
        };
        let needed_index_bytes = index_bytes.len() as u64;
        let existing_index = registry
            .get_mesh(mesh)
            .and_then(|asset| asset.index_buffer.as_ref())
            .map(|ib| ib.capacity());
        let replaced_index = match existing_index {
            Some(capacity) if capacity >= needed_index_bytes => None,
            Some(capacity) => {
                let new_capacity = needed_index_bytes.max(capacity * 2);
                let mut ib = IndexBuffer::try_new(
                    self.context.clone(),
                    new_capacity,
                    IndexType::Uint32,
                    indices.len() as u32,
                )?;
                ib.upload_data(index_bytes);
                Some(ib)
            }
            None if indices.is_empty() => None,
            None => {
                let mut ib = IndexBuffer::try_new(
                    self.context.clone(),
                    needed_index_bytes,
                    IndexType::Uint32,
                    indices.len() as u32,
                )?;
                ib.upload_data(index_bytes);
                Some(ib)
            }
        };

        // Commit phase: infallible pointer writes and field updates. Mapped
        // pointers for in-place buffers are acquired here too — mapping a
        // persistently-mapped CpuToGpu allocation cannot fail, but the error
        // surfaces before any state changes if it ever does.
        let mesh_asset = registry
            .get_mesh_mut(mesh)
            .ok_or_else(|| RendererError::StaleHandle {
                resource: "mesh".to_string(),
                detail: format!("{mesh:?} in update_mesh_dynamic"),
            })?;

        for (kind, new_vb) in replaced_attributes {
            if let Some(old) = mesh_asset.attribute_buffers.insert(kind, new_vb) {
                let (buffer, allocation) = old.into_native_parts();
                retirement.retire(RetiredBuffer::new(buffer, allocation, self.context.clone()));
            }
        }
        for (kind, bytes) in &split {
            if let Some(vb) = mesh_asset.attribute_buffers.get(kind)
                && vb.capacity() >= bytes.len() as u64
            {
                let ptr = vb.mapped_ptr()?;
                unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len()) };
            }
        }

        if let Some(new_ib) = replaced_index {
            if let Some(old) = mesh_asset.index_buffer.replace(new_ib) {
                let (buffer, allocation) = old.into_native_parts();
                retirement.retire(RetiredBuffer::new(buffer, allocation, self.context.clone()));
            }
        } else if let Some(ib) = mesh_asset.index_buffer.as_ref()
            && ib.capacity() >= needed_index_bytes
        {
            let ptr = ib.mapped_ptr()?;
            unsafe { std::ptr::copy_nonoverlapping(index_bytes.as_ptr(), ptr, index_bytes.len()) };
        }

        mesh_asset.vertex_count = vertex_count;
        mesh_asset.index_count = indices.len() as u32;
        Ok(())
    }
}

/// One split-attribute upload request for `MeshManager::upload_soa_buffers`.
struct SoaUpload<'a> {
    usage: MeshUsage,
    split: &'a HashMap<AttributeType, Vec<u8>>,
    vertex_count: u32,
    index_bytes: &'a [u8],
    index_type: IndexType,
    index_count: u32,
}

/// Slice one interleaved vertex blob into per-attribute byte buffers.
///
/// Offsets derive from the layout formats zipped with the attribute
/// semantics; every attribute range is checked against the stride before
/// slicing, and the blob length must describe exactly `vertex_count`
/// vertices. Creation and dynamic updates share this slicing so both paths
/// publish structurally identical attribute data.
fn split_attribute_bytes(
    layout: &crate::vertex::VertexLayout,
    attributes: &[AttributeType],
    vertex_bytes: &[u8],
    vertex_count: u32,
) -> Result<HashMap<AttributeType, Vec<u8>>, RendererError> {
    let stride = layout.stride();
    if vertex_bytes.len() != vertex_count as usize * stride {
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
    for (format, attribute) in layout.formats().iter().zip(attributes.iter()) {
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
        let mut bytes = Vec::with_capacity(vertex_count as usize * size);
        for vertex in vertex_bytes.chunks_exact(stride) {
            bytes.extend_from_slice(&vertex[offset..offset + size]);
        }
        map.insert(*attribute, bytes);
        offset += size;
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ValidationMode;

    /// Allocation failure during growth returns a typed error and leaves the
    /// previously published mesh fully intact (issue #86).
    #[test]
    #[ignore = "requires a Vulkan device"]
    fn test_dynamic_update_allocation_failure_preserves_mesh() {
        use crate::vertex::{VertexAttributeFormat, VertexLayout};

        let mut renderer = crate::renderer::VulkanRenderer::init_headless(
            64,
            48,
            ValidationMode::Disabled,
            std::ffi::CString::new("mesh update failure test").unwrap(),
            std::ffi::CString::new("Katla").unwrap(),
        )
        .unwrap();

        let descriptor = MeshDescriptor {
            layout: VertexLayout::new(vec![VertexAttributeFormat::Float3]),
            attributes: vec![AttributeType::Position],
            topology: PrimitiveTopology::TriangleList,
            usage: MeshUsage::Dynamic,
            vertex_count: 3,
            index_count: 3,
            index_format: crate::backend::command::IndexType::Uint32,
        };
        let blob = [0.25f32; 9].map(f32::to_bits);
        let blob =
            unsafe { std::slice::from_raw_parts(blob.as_ptr() as *const u8, blob.len() * 4) };
        let mesh = renderer
            .create_mesh_dynamic(&descriptor, blob, &[0, 1, 2])
            .unwrap();
        assert_eq!(renderer.mesh_vertex_count(mesh), Some(3));
        assert_eq!(renderer.mesh_index_count(mesh), Some(3));

        // Fail the next allocation: growing to 6 vertices must fail typed
        // without changing counts, buffers, or queuing retirements.
        renderer.context.allocator.inject_allocation_failures(1);
        let grown = [0.25f32; 18].map(f32::to_bits);
        let grown =
            unsafe { std::slice::from_raw_parts(grown.as_ptr() as *const u8, grown.len() * 4) };
        let error = renderer
            .update_mesh_dynamic(mesh, grown, 6, &[0, 1, 2, 3, 4, 5])
            .unwrap_err();
        match error {
            RendererError::AllocationFailed { .. } => {}
            other => panic!("expected AllocationFailed, got {other:?}"),
        }
        assert_eq!(renderer.mesh_vertex_count(mesh), Some(3));
        assert_eq!(renderer.mesh_index_count(mesh), Some(3));
        assert_eq!(renderer.pending_buffer_retirements(), 0);

        // The failure corrupted nothing: the same growth succeeds retrying
        // without injected failures.
        renderer
            .update_mesh_dynamic(mesh, grown, 6, &[0, 1, 2, 3, 4, 5])
            .expect("retry after allocation failure must succeed");
        assert_eq!(renderer.mesh_vertex_count(mesh), Some(6));
        assert_eq!(renderer.mesh_index_count(mesh), Some(6));

        renderer.destroy();
    }
}
