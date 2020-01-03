use crate::gl;

pub struct Mesh {
    vbo: u32,
    ibo: u32,
    num_triangles: u32,
    textures: [u32; 4],
    model_matrix: [f32; 16],
}

impl Mesh {
    pub fn new() -> Self {
        Self {
            vbo: 0,
            ibo: 0,
            num_triangles: 0,
            textures: [0, 0, 0, 0],
            model_matrix: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
        }
    }
    pub fn add_vertices(&mut self, vertices: Vec<f32>, indices: Vec<u32>) {
        let mut vbo = 0u32;
        let total_buffer_size = vertices.len() * 4 + indices.len() * 4;
        unsafe {
            gl::CreateBuffers(1, &mut vbo);
            gl::NamedBufferStorage(
                vbo,
                total_buffer_size as isize,
                0 as *const _,
                gl::DYNAMIC_STORAGE_BIT,
            );
        }
    }
}
