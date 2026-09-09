use crate::backend::resource::GpuBuffer;
use crate::error::RendererError;
use crate::handle::MeshHandle;
use crate::renderer::registry::MeshIndexElement;

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
        use crate::renderer::registry::MeshUsage;
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
        };
        let id = self.meshes.insert(mesh);
        Ok(MeshHandle::new(id))
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
        };
        let id = self.meshes.insert(mesh);
        Ok(MeshHandle::new(id))
    }

    pub(crate) fn update_mesh_dynamic_impl(
        &mut self,
        mesh: MeshHandle,
        vertex_data: &[u8],
        indices: &[u32],
    ) -> Result<(), RendererError> {
        let Some(m) = self.meshes.get_mut(mesh.index()) else {
            return Err(RendererError::StaleHandle {
                resource: "mesh".to_string(),
                detail: format!("{mesh:?} in Metal update_mesh_dynamic"),
            });
        };
        // Validate both payloads before copying either: oversized data used
        // to be silently truncated while index_count recorded the full
        // length. Buffer growth policy belongs to dynamic-mesh capacity
        // design; here a too-large update fails instead of corrupting.
        let vertex_capacity = m.vertex_buffer.size() as usize;
        if vertex_data.len() > vertex_capacity {
            return Err(RendererError::UploadFailed {
                resource: "mesh".to_string(),
                expected_bytes: vertex_capacity,
                actual_bytes: vertex_data.len(),
                detail: "vertex data exceeds buffer capacity".to_string(),
            });
        }
        let index_capacity = m.index_buffer.size() as usize;
        let index_bytes_len = indices.len() * 4;
        if index_bytes_len > index_capacity {
            return Err(RendererError::UploadFailed {
                resource: "mesh".to_string(),
                expected_bytes: index_capacity,
                actual_bytes: index_bytes_len,
                detail: "index data exceeds buffer capacity".to_string(),
            });
        }
        {
            let ptr = m.vertex_buffer.map();
            unsafe {
                std::ptr::copy_nonoverlapping(
                    vertex_data.as_ptr(),
                    ptr,
                    vertex_data.len().min(m.vertex_buffer.size() as usize),
                );
            }
            m.vertex_buffer.unmap();
        }
        {
            let index_bytes = unsafe {
                std::slice::from_raw_parts(indices.as_ptr() as *const u8, indices.len() * 4)
            };
            let ptr = m.index_buffer.map();
            unsafe {
                std::ptr::copy_nonoverlapping(
                    index_bytes.as_ptr(),
                    ptr,
                    index_bytes.len().min(m.index_buffer.size() as usize),
                );
            }
            m.index_buffer.unmap();
        }
        m.index_count = indices.len() as u32;
        Ok(())
    }
}
