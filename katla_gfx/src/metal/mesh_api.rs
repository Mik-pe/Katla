use crate::backend::resource::GpuBuffer;
use crate::error::RendererError;
use crate::handle::MeshHandle;
use crate::primitives;
use crate::vertex::VertexPBR;

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

    pub(crate) fn create_primitive_mesh(
        &mut self,
        vertices: Vec<VertexPBR>,
        indices: Vec<u32>,
    ) -> MeshHandle {
        let vertex_bytes = bytemuck::cast_slice(&vertices);
        let (vertex_buffer, index_buffer, index_count) = self
            .upload_vertex_index_data(vertex_bytes, &indices)
            .expect("Failed to create primitive mesh buffers");

        let mesh = MetalMesh {
            vertex_buffer,
            index_buffer,
            index_count,
        };
        let id = self.meshes.insert(mesh);
        MeshHandle::new(id)
    }

    pub(crate) fn create_mesh_from_vertices<T, U>(
        &mut self,
        vertices: &[T],
        indices: &[U],
    ) -> MeshHandle
    where
        T: bytemuck::Pod,
        U: bytemuck::Pod,
    {
        let vertex_bytes = bytemuck::cast_slice(vertices);
        let index_u32: Vec<u32> = indices
            .iter()
            .map(|v| {
                let bytes = bytemuck::bytes_of(v);
                match bytes.len() {
                    1 => bytes[0] as u32,
                    2 => u16::from_ne_bytes([bytes[0], bytes[1]]) as u32,
                    4 => u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
                    _ => 0,
                }
            })
            .collect();

        let (vertex_buffer, index_buffer, index_count) = self
            .upload_vertex_index_data(vertex_bytes, &index_u32)
            .expect("Failed to create mesh buffers");

        let mesh = MetalMesh {
            vertex_buffer,
            index_buffer,
            index_count,
        };
        let id = self.meshes.insert(mesh);
        MeshHandle::new(id)
    }

    pub(crate) fn register_mesh_raw_impl(
        &mut self,
        vertex_data: &[u8],
        index_data: &[u32],
    ) -> MeshHandle {
        let (vertex_buffer, index_buffer, index_count) = self
            .upload_vertex_index_data(vertex_data, index_data)
            .expect("Failed to create mesh buffers");

        let mesh = MetalMesh {
            vertex_buffer,
            index_buffer,
            index_count,
        };
        let id = self.meshes.insert(mesh);
        MeshHandle::new(id)
    }

    pub(crate) fn update_mesh_dynamic_impl(
        &mut self,
        mesh: MeshHandle,
        vertex_data: &[u8],
        indices: &[u32],
    ) -> Result<(), RendererError> {
        let Some(m) = self.meshes.get_mut(mesh.index()) else {
            return Err(RendererError::NotFound("Mesh not found".into()));
        };
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

    pub(crate) fn create_cube_mesh_impl(&mut self, size: [f32; 3]) -> MeshHandle {
        let (vertices, indices) = primitives::generate_cube(size);
        self.create_primitive_mesh(vertices, indices)
    }

    pub(crate) fn create_sphere_mesh_impl(
        &mut self,
        radius: f32,
        segments: u32,
        rings: u32,
    ) -> MeshHandle {
        let (vertices, indices) = primitives::generate_sphere(radius, segments, rings);
        self.create_primitive_mesh(vertices, indices)
    }

    pub(crate) fn create_plane_mesh_impl(&mut self, width: f32, height: f32) -> MeshHandle {
        let (vertices, indices) = primitives::generate_plane(width, height);
        self.create_primitive_mesh(vertices, indices)
    }

    pub(crate) fn create_cone_mesh_impl(
        &mut self,
        height: f32,
        base_radius: f32,
        segments: u32,
    ) -> MeshHandle {
        let (vertices, indices) = primitives::generate_cone(height, base_radius, segments);
        self.create_primitive_mesh(vertices, indices)
    }

    pub(crate) fn create_cylinder_mesh_impl(
        &mut self,
        height: f32,
        radius: f32,
        segments: u32,
    ) -> MeshHandle {
        let (vertices, indices) = primitives::generate_cylinder(height, radius, segments);
        self.create_primitive_mesh(vertices, indices)
    }

    pub(crate) fn create_torus_mesh_impl(
        &mut self,
        major_radius: f32,
        minor_radius: f32,
        segments: u32,
        rings: u32,
    ) -> MeshHandle {
        let (vertices, indices) =
            primitives::generate_torus(major_radius, minor_radius, segments, rings);
        self.create_primitive_mesh(vertices, indices)
    }

    pub(crate) fn create_plane_xy_mesh_impl(
        &mut self,
        width: f32,
        height: f32,
        segments: u32,
    ) -> MeshHandle {
        let (vertices, indices) = primitives::generate_plane_xy(width, height, segments);
        self.create_primitive_mesh(vertices, indices)
    }
}
