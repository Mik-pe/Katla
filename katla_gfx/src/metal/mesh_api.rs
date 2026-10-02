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
        self.persistent_buffers
            .replace(&[], &[&mesh.vertex_buffer.inner, &mesh.index_buffer.inner])?;
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
        self.persistent_buffers
            .replace(&[], &[&mesh.vertex_buffer.inner, &mesh.index_buffer.inner])?;
        Ok(self.meshes.insert(mesh))
    }

    /// Publish immutable vertex/index replacements for a dynamic mesh.
    /// Capacities are retained on shrink and grow geometrically. Allocation,
    /// upload, and residency publication finish before replacing the live mesh;
    /// submitted command buffers retain every previous allocation they use.
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

        let vertex_needed = vertex_data.len() as u64;
        let vertex_capacity = if vertex_needed > m.vertex_buffer.size() {
            vertex_needed.max(m.vertex_buffer.size().saturating_mul(2))
        } else {
            m.vertex_buffer.size()
        };
        let vertex_buffer = self.context.create_buffer(vertex_capacity, true)?;
        unsafe {
            std::ptr::copy_nonoverlapping(
                vertex_data.as_ptr(),
                vertex_buffer.map(),
                vertex_data.len(),
            );
        }
        vertex_buffer.unmap();

        let index_bytes = bytemuck::cast_slice(indices);
        let index_needed = index_bytes.len() as u64;
        let index_capacity = if index_needed > m.index_buffer.size() {
            index_needed.max(m.index_buffer.size().saturating_mul(2))
        } else {
            m.index_buffer.size()
        };
        let index_buffer = self.context.create_buffer(index_capacity, true)?;
        unsafe {
            std::ptr::copy_nonoverlapping(
                index_bytes.as_ptr(),
                index_buffer.map(),
                index_bytes.len(),
            );
        }
        index_buffer.unmap();

        self.persistent_buffers.replace(
            &[&m.vertex_buffer.inner, &m.index_buffer.inner],
            &[&vertex_buffer.inner, &index_buffer.inner],
        )?;
        m.vertex_buffer = vertex_buffer;
        m.index_buffer = index_buffer;
        m.index_count = indices.len() as u32;
        m.vertex_count = vertex_count;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::command::{GpuBlitEncoder, GpuCommandBuffer};
    use crate::renderer::registry::{MeshDescriptor, PrimitiveTopology};
    use crate::vertex::VertexUI;
    use objc2_metal::{MTL4CommandBuffer, MTLBuffer};

    #[test]
    fn test_native_same_capacity_mesh_replacement_preserves_pending_gpu_read() {
        let mut renderer =
            MetalRenderer::new(super::super::context::MetalContext::init_headless().unwrap())
                .unwrap();
        let vertices = [VertexUI::new([1.0, 2.0], [0.0, 1.0], [1, 2, 3, 255], 0); 3];
        let descriptor = MeshDescriptor::from_types::<VertexUI, u32>(
            PrimitiveTopology::TriangleList,
            MeshUsage::Dynamic,
            3,
            3,
        );
        let handle = renderer
            .register_mesh_raw_impl(&descriptor, bytemuck::cast_slice(&vertices), &[0, 1, 2])
            .unwrap();
        let original = renderer.meshes.get(handle).unwrap().vertex_buffer.clone();
        let original_index = renderer
            .meshes
            .get(handle)
            .unwrap()
            .index_buffer
            .inner
            .gpuAddress();
        let snapshot = renderer.persistent_buffers.snapshot();
        let probe = renderer
            .context
            .create_buffer(original.size(), true)
            .unwrap();
        let mut command = renderer.context.create_command_buffer();
        command.begin();
        command.inner.useResidencySet(snapshot.native());
        command
            .resources
            .retain_persistent_residency(snapshot.clone());
        let mut blit = command.begin_blit_pass();
        blit.copy_buffer_to_buffer(&original, 0, &probe, 0, original.size());
        blit.end_encoding();
        command.end();
        let replacements = [VertexUI::new([9.0, 10.0], [1.0, 0.0], [9, 8, 7, 255], 0); 3];
        renderer
            .update_mesh_dynamic_impl(handle, bytemuck::cast_slice(&replacements), 3, &[2, 1, 0])
            .unwrap();
        let replaced = renderer.meshes.get(handle).unwrap();
        assert_ne!(
            replaced.vertex_buffer.inner.gpuAddress(),
            original.inner.gpuAddress()
        );
        assert_ne!(replaced.index_buffer.inner.gpuAddress(), original_index);
        assert_eq!(replaced.vertex_buffer.size(), original.size());
        snapshot.validate_buffer(&original.inner).unwrap();
        assert!(
            snapshot
                .validate_buffer(&replaced.vertex_buffer.inner)
                .is_err()
        );
        command.submit(&renderer.context);
        command.wait_until_completed().unwrap();
        let actual = unsafe { std::slice::from_raw_parts(probe.map(), original.size() as usize) };
        assert_eq!(actual, bytemuck::cast_slice::<VertexUI, u8>(&vertices));
    }
}
