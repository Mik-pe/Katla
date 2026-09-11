use crate::backend::resource::GpuBuffer;
use crate::error::RendererError;
use crate::handle::MeshHandle;
use crate::renderer::registry::{MeshIndexElement, MeshUsage};

use super::buffer::MetalBuffer;
use super::metal_renderer::MetalMesh;
use super::metal_renderer::MetalRenderer;

impl MetalRenderer {
    pub(crate) fn upload_vertex_index_data(
        &mut self,
        vertex_data: &[u8],
        index_data: &[u32],
    ) -> Result<(MetalBuffer, MetalBuffer, u32), RendererError> {
        let vertex_buffer = self.context.create_buffer(vertex_data.len() as u64, true)?;
        let index_buffer = self
            .context
            .create_buffer((index_data.len() * 4) as u64, true)?;

        {
            let ptr = vertex_buffer.map();
            unsafe {
                std::ptr::copy_nonoverlapping(vertex_data.as_ptr(), ptr, vertex_data.len());
            }
            vertex_buffer.unmap();
        }
        {
            let ptr = index_buffer.map();
            let index_bytes = unsafe {
                std::slice::from_raw_parts(index_data.as_ptr() as *const u8, index_data.len() * 4)
            };
            unsafe {
                std::ptr::copy_nonoverlapping(index_bytes.as_ptr(), ptr, index_bytes.len());
            }
            index_buffer.unmap();
        }

        Ok((vertex_buffer, index_buffer, index_data.len() as u32))
    }

    pub(crate) fn create_mesh_from_vertices<T, U>(
        &mut self,
        vertices: &[T],
        indices: &[U],
        topology: crate::renderer::registry::PrimitiveTopology,
    ) -> Result<MeshHandle, RendererError>
    where
        T: crate::vertex::Vertex,
        U: MeshIndexElement,
    {
        // Validate bytes against the typed layout; the descriptor itself is
        // not stored.
        crate::renderer::registry::MeshDescriptor::describe_typed_upload(
            topology,
            MeshUsage::Static,
            vertices,
            indices,
        )?;
        let vertex_bytes = bytemuck::cast_slice(vertices);
        // Metal storage keeps a single index width; conversion is keyed off the
        // typed element format rather than guessed from byte sizes. Ranges
        // were validated above, so widening cannot introduce aliasing.
        let index_u32: Vec<u32> = indices.iter().map(|&v| U::to_u32(v)).collect();

        let (vertex_buffer, index_buffer, index_count) =
            self.upload_vertex_index_data(vertex_bytes, &index_u32)?;

        let mesh = MetalMesh {
            vertex_buffer,
            index_buffer,
            index_count,
            vertex_count: vertices.len() as u32,
            vertex_stride: std::mem::size_of::<T>() as u32,
            usage: MeshUsage::Static,
        };
        Ok(self.meshes.insert(mesh))
    }

    pub(crate) fn register_mesh_raw_impl(
        &mut self,
        descriptor: &crate::renderer::registry::MeshDescriptor,
        vertex_data: &[u8],
        index_data: &[u32],
    ) -> Result<MeshHandle, RendererError> {
        use crate::renderer::registry::PrimitiveTopology;
        if descriptor.topology != PrimitiveTopology::TriangleList {
            return Err(RendererError::UnsupportedFeature(format!(
                "mesh topology {:?} is not encodable; only TriangleList is supported",
                descriptor.topology
            )));
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
        if index_data.len() as u32 != descriptor.index_count {
            return Err(RendererError::InvalidDescriptor {
                resource: "mesh".to_string(),
                reason: format!(
                    "dynamic index count {} disagrees with descriptor count {}",
                    index_data.len(),
                    descriptor.index_count
                ),
            });
        }
        for (position, index) in index_data.iter().enumerate() {
            if *index >= descriptor.vertex_count {
                return Err(RendererError::InvalidDescriptor {
                    resource: "mesh".to_string(),
                    reason: format!(
                        "index {position} references vertex {index} of {}",
                        descriptor.vertex_count
                    ),
                });
            }
        }
        let (vertex_buffer, index_buffer, index_count) =
            self.upload_vertex_index_data(vertex_data, index_data)?;

        let mesh = MetalMesh {
            vertex_buffer,
            index_buffer,
            index_count,
            vertex_count: descriptor.vertex_count,
            vertex_stride: stride as u32,
            usage: descriptor.usage,
        };
        Ok(self.meshes.insert(mesh))
    }

    /// Update a dynamic mesh with new vertex and index data.
    ///
    /// Implements the same backend-neutral contract as Vulkan's
    /// `update_mesh_dynamic` (see
    /// `crate::renderer::registry::validate_dynamic_update`): the blob must
    /// describe exactly `vertex_count` vertices of the recorded stride and
    /// every index must be in range; success publishes one consistent mesh,
    /// shrinking never reallocates, growth allocates replacement buffers
    /// before any state changes, and failure leaves the previous mesh intact.
    ///
    /// Retirement: replaced `MTLBuffer`s are dropped immediately, which is
    /// safe on Metal — command buffers retain the resources their encoded
    /// commands reference until the command buffer completes, so in-flight
    /// submissions keep the old buffers alive.
    pub(crate) fn update_mesh_dynamic_impl(
        &mut self,
        mesh: MeshHandle,
        vertex_data: &[u8],
        vertex_count: u32,
        indices: &[u32],
    ) -> Result<(), RendererError> {
        let Some(m) = self.meshes.get_mut(mesh) else {
            return Err(RendererError::StaleHandle {
                resource: "mesh".to_string(),
                detail: format!("{mesh:?} in Metal update_mesh_dynamic"),
            });
        };
        if m.usage != MeshUsage::Dynamic {
            return Err(RendererError::InvalidOperation(format!(
                "mesh {mesh:?} is not dynamic; static meshes are immutable"
            )));
        }
        crate::renderer::registry::validate_dynamic_update(
            m.vertex_stride as usize,
            vertex_data,
            vertex_count,
            indices,
        )?;

        // Fallible phase: allocate and fill replacement buffers for anything
        // that no longer fits, before touching the mesh.
        let needed_vertex = vertex_data.len() as u64;
        let vertex_replacement = if m.vertex_buffer.size() < needed_vertex {
            let new_capacity = needed_vertex.max(m.vertex_buffer.size() * 2);
            let buffer = self.context.create_buffer(new_capacity, true)?;
            let ptr = buffer.map();
            unsafe {
                std::ptr::copy_nonoverlapping(vertex_data.as_ptr(), ptr, vertex_data.len());
            }
            buffer.unmap();
            Some(buffer)
        } else {
            None
        };

        let index_bytes_len = indices.len() * 4;
        let needed_index = index_bytes_len as u64;
        let index_replacement = if m.index_buffer.size() < needed_index {
            let new_capacity = needed_index.max(m.index_buffer.size() * 2);
            let buffer = self.context.create_buffer(new_capacity, true)?;
            let index_bytes = unsafe {
                std::slice::from_raw_parts(indices.as_ptr() as *const u8, index_bytes_len)
            };
            let ptr = buffer.map();
            unsafe {
                std::ptr::copy_nonoverlapping(index_bytes.as_ptr(), ptr, index_bytes.len());
            }
            buffer.unmap();
            Some(buffer)
        } else {
            None
        };

        // Commit phase: infallible copies through mapped shared storage and
        // field updates. Dropped old buffers stay alive through any in-flight
        // command buffer referencing them (Metal resource retention).
        if let Some(new_vertex) = vertex_replacement {
            m.vertex_buffer = new_vertex;
        } else {
            let ptr = m.vertex_buffer.map();
            unsafe {
                std::ptr::copy_nonoverlapping(vertex_data.as_ptr(), ptr, vertex_data.len());
            }
            m.vertex_buffer.unmap();
        }
        if let Some(new_index) = index_replacement {
            m.index_buffer = new_index;
        } else {
            let index_bytes = unsafe {
                std::slice::from_raw_parts(indices.as_ptr() as *const u8, index_bytes_len)
            };
            let ptr = m.index_buffer.map();
            unsafe {
                std::ptr::copy_nonoverlapping(index_bytes.as_ptr(), ptr, index_bytes.len());
            }
            m.index_buffer.unmap();
        }
        m.index_count = indices.len() as u32;
        m.vertex_count = vertex_count;
        Ok(())
    }
}
