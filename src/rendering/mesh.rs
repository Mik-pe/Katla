use crate::gl;

pub struct Mesh {
    buffer: u32,
    vao: u32,
    num_triangles: u32,
    textures: [u32; 4],
    model_matrix: [f32; 16],
}

impl Mesh {
    pub fn new() -> Self {
        Self {
            buffer: 0,
            vao: 0,
            num_triangles: 0,
            textures: [0, 0, 0, 0],
            model_matrix: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
        }
    }

    pub fn add_vertices(&mut self, vertices: Vec<u8>, indices: Vec<u32>) {
        self.num_triangles = (indices.len() / 3) as u32;
        let vert_len_aligned = vertices.len() + vertices.len() % 4;
        let ind_len_aligned = indices.len() * 4;
        let total_buffer_size = vert_len_aligned + ind_len_aligned;
        unsafe {
            gl::CreateBuffers(1, &mut self.buffer);
            gl::NamedBufferStorage(
                self.buffer,
                total_buffer_size as isize,
                std::ptr::null(),
                gl::DYNAMIC_STORAGE_BIT,
            );

            gl::NamedBufferSubData(
                self.buffer,
                0,
                ind_len_aligned as isize,
                indices.as_ptr() as *const _,
            );
            gl::NamedBufferSubData(
                self.buffer,
                ind_len_aligned as isize,
                vertices.len() as isize,
                vertices.as_ptr() as *const _,
            );

            gl::CreateVertexArrays(1, &mut self.vao);
            gl::VertexArrayVertexBuffer(self.vao, 0, self.buffer, ind_len_aligned as isize, 4);
            gl::VertexArrayElementBuffer(self.vao, self.buffer);

            gl::EnableVertexArrayAttrib(self.vao, 0);

            gl::VertexArrayAttribFormat(self.vao, 0, 3, gl::FLOAT, gl::FALSE, 0);

            gl::VertexArrayAttribBinding(self.vao, 0, 0);
        }
    }

    pub fn draw(&self) {
        unsafe {
            gl::BindVertexArray(self.vao);
            gl::DrawArrays(gl::TRIANGLES, 0, (self.num_triangles * 3) as i32);
        }
    }
}
