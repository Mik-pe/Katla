use crate::rendering::shaders::Program;
use gl;

pub struct Gui {
    gui_program: Program,
    vao: u32,
    vbo: u32,
    vertex_stride: u32,
}

impl Gui {
    pub fn new() -> Self {
        let gui_program = Program::new(
            include_bytes!("../resources/shaders/gui.vert"),
            include_bytes!("../resources/shaders/gui.frag"),
        );
        let mut vao = 0;
        let mut vbo = 0;
        let mut vertex_stride = 0;
        unsafe {
            gl::CreateBuffers(1, &mut vbo);
            gl::CreateVertexArrays(1, &mut vao);

            gl::EnableVertexArrayAttrib(vao, 0);
            gl::VertexArrayAttribFormat(vao, 0, 2, gl::FLOAT, gl::FALSE, 0);
            gl::VertexArrayAttribBinding(vao, 0, 0);
            vertex_stride += 8;
            gl::EnableVertexArrayAttrib(vao, 1);
            gl::VertexArrayAttribFormat(vao, 1, 2, gl::FLOAT, gl::FALSE, vertex_stride);
            gl::VertexArrayAttribBinding(vao, 1, 0);
            vertex_stride += 8;
            gl::EnableVertexArrayAttrib(vao, 2);
            gl::VertexArrayAttribFormat(vao, 2, 4, gl::UNSIGNED_BYTE, gl::TRUE, vertex_stride);
            gl::VertexArrayAttribBinding(vao, 2, 0);
            vertex_stride += 4;
        }
        Self {
            gui_program,
            vao,
            vbo,
            vertex_stride,
        }
    }

    pub unsafe fn render_gui(&mut self, ui: imgui::Ui) {
        let display_size = ui.io().display_size;
        let _fb_width = display_size[0] * ui.io().display_framebuffer_scale[0];
        let fb_height = display_size[1] * ui.io().display_framebuffer_scale[1];
        let draw_data = ui.render();

        for draw_list in draw_data.draw_lists() {
            let vtx_buffer = draw_list.vtx_buffer();
            let idx_buffer = draw_list.idx_buffer();
            let vtx_buf_stride = std::mem::size_of::<imgui::sys::ImDrawVert>();
            let idx_buf_stride = std::mem::size_of::<imgui::sys::ImDrawIdx>();
            let idx_buf_size =
                idx_buf_stride * idx_buffer.len() % 16 + idx_buf_stride * idx_buffer.len();
            let vtx_buf_size = vtx_buf_stride * vtx_buffer.len();
            let total_buf_size = idx_buf_size + vtx_buf_size;

            //TODO: Fetch previous state and reset it afterwards
            gl::Enable(gl::BLEND);
            gl::BlendEquation(gl::FUNC_ADD);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
            gl::Disable(gl::CULL_FACE);
            gl::Disable(gl::DEPTH_TEST);
            gl::Enable(gl::SCISSOR_TEST);

            gl::NamedBufferStorage(
                self.vbo,
                (total_buf_size) as isize,
                std::ptr::null(),
                gl::DYNAMIC_STORAGE_BIT,
            );
            gl::NamedBufferSubData(
                self.vbo,
                0,
                idx_buf_size as isize,
                idx_buffer.as_ptr() as *const _,
            );
            gl::NamedBufferSubData(
                self.vbo,
                idx_buf_size as isize,
                vtx_buf_size as isize,
                vtx_buffer.as_ptr() as *const _,
            );

            gl::VertexArrayElementBuffer(self.vao, self.vbo);
            let gui_proj = mikpe_math::Mat4([
                mikpe_math::Vec4([2.0 / display_size[0] as f32, 0.0, 0.0, 0.0]),
                mikpe_math::Vec4([0.0, 2.0 / -display_size[1] as f32, 0.0, 0.0]),
                mikpe_math::Vec4([0.0, 0.0, -1.0, 0.0]),
                mikpe_math::Vec4([-1.0, 1.0, 0.0, 1.0]),
            ]);
            self.gui_program
                .uniform_mat(&"u_projMatrix".to_owned(), &gui_proj);
            self.gui_program.bind();

            gl::VertexArrayVertexBuffer(
                self.vao,
                0,
                self.vbo,
                idx_buf_size as isize,
                self.vertex_stride as i32,
            );
            gl::BindVertexArray(self.vao);

            for cmd_list in draw_list.commands() {
                match cmd_list {
                    imgui::DrawCmd::Elements { count, cmd_params } => {
                        gl::BindTextureUnit(0, cmd_params.texture_id.id() as _);
                        gl::Scissor(
                            cmd_params.clip_rect[0] as i32,
                            (fb_height - cmd_params.clip_rect[3]) as i32,
                            (cmd_params.clip_rect[2] - cmd_params.clip_rect[0]) as i32,
                            (cmd_params.clip_rect[3] - cmd_params.clip_rect[1]) as i32,
                        );

                        gl::DrawElements(
                            gl::TRIANGLES,
                            count as i32,
                            gl::UNSIGNED_SHORT,
                            (cmd_params.idx_offset * idx_buf_stride) as *const _,
                        );
                    }
                    _ => {}
                }
            }
        }
    }
}
