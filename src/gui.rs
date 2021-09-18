// use crate::rendering::shaders::Program;
// use gl;

// pub struct Gui {
//     gui_program: Program,
//     vao: u32,
//     vertex_stride: u32,
//     idx_stride: usize,
//     offset_alignment: i32,
// }

// impl Gui {
//     pub fn new() -> Self {
//         let gui_program = Program::new(
//             include_bytes!("../resources/shaders/gui.vert"),
//             include_bytes!("../resources/shaders/gui.frag"),
//         );
//         let mut vao = 0;
//         let mut vertex_stride = 0;
//         let idx_stride = std::mem::size_of::<imgui::sys::ImDrawIdx>();
//         let mut offset_alignment = 0;
//         unsafe {
//             gl::GetIntegerv(gl::UNIFORM_BUFFER_OFFSET_ALIGNMENT, &mut offset_alignment);

//             gl::CreateVertexArrays(1, &mut vao);

//             gl::EnableVertexArrayAttrib(vao, 0);
//             gl::VertexArrayAttribFormat(vao, 0, 2, gl::FLOAT, gl::FALSE, 0);
//             gl::VertexArrayAttribBinding(vao, 0, 0);
//             vertex_stride += 8;
//             gl::EnableVertexArrayAttrib(vao, 1);
//             gl::VertexArrayAttribFormat(vao, 1, 2, gl::FLOAT, gl::FALSE, vertex_stride);
//             gl::VertexArrayAttribBinding(vao, 1, 0);
//             vertex_stride += 8;
//             gl::EnableVertexArrayAttrib(vao, 2);
//             gl::VertexArrayAttribFormat(vao, 2, 4, gl::UNSIGNED_BYTE, gl::TRUE, vertex_stride);
//             gl::VertexArrayAttribBinding(vao, 2, 0);
//             vertex_stride += 4;
//         }
//         Self {
//             gui_program,
//             vao,
//             vertex_stride,
//             idx_stride,
//             offset_alignment,
//         }
//     }

//     fn align_length(in_len: isize, alignment: isize) -> isize {
//         let remainder = alignment - in_len % alignment;
//         if remainder == alignment {
//             in_len
//         } else {
//             let new_len = in_len + remainder;
//             new_len
//         }
//     }

//     pub unsafe fn render_gui(&mut self, ui: imgui::Ui) {
//         let display_size = ui.io().display_size;
//         let fb_scale = ui.io().display_framebuffer_scale;
//         let _fb_width = display_size[0] * fb_scale[0];
//         let fb_height = display_size[1] * fb_scale[1];
//         let draw_data = ui.render();

//         for draw_list in draw_data.draw_lists() {
//             let vtx_buffer = draw_list.vtx_buffer();
//             let idx_buffer = draw_list.idx_buffer();
//             let vtx_buf_stride = std::mem::size_of::<imgui::sys::ImDrawVert>();
//             let idx_len = (self.idx_stride * idx_buffer.len()) as isize;
//             let vtx_len = (vtx_buf_stride * vtx_buffer.len()) as isize;
//             let aligned_idx_len = Self::align_length(idx_len, self.offset_alignment as isize);
//             let aligned_vtx_len = Self::align_length(vtx_len, self.offset_alignment as isize);
//             let total_buf_size = aligned_idx_len + aligned_vtx_len;

//             //TODO: Fetch previous state and reset it afterwards
//             gl::Enable(gl::BLEND);
//             gl::BlendEquation(gl::FUNC_ADD);
//             gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
//             gl::Disable(gl::CULL_FACE);
//             gl::Disable(gl::DEPTH_TEST);
//             gl::Enable(gl::SCISSOR_TEST);

//             //TODO: Should not have to recreate buffer every frame like this, but what gives:
//             let mut vbo = 0;
//             gl::CreateBuffers(1, &mut vbo);
//             gl::NamedBufferStorage(
//                 vbo,
//                 total_buf_size,
//                 std::ptr::null(),
//                 gl::DYNAMIC_STORAGE_BIT,
//             );

//             gl::NamedBufferSubData(vbo, 0, idx_len, idx_buffer.as_ptr() as *const _);
//             gl::NamedBufferSubData(
//                 vbo,
//                 aligned_idx_len,
//                 vtx_len,
//                 vtx_buffer.as_ptr() as *const _,
//             );

//             let gui_proj = katla_math::Mat4([
//                 katla_math::Vec4([2.0 / display_size[0] as f32, 0.0, 0.0, 0.0]),
//                 katla_math::Vec4([0.0, 2.0 / -display_size[1] as f32, 0.0, 0.0]),
//                 katla_math::Vec4([0.0, 0.0, -1.0, 0.0]),
//                 katla_math::Vec4([-1.0, 1.0, 0.0, 1.0]),
//             ]);
//             self.gui_program.uniform_mat("u_projMatrix", &gui_proj);
//             self.gui_program.bind();

//             gl::VertexArrayVertexBuffer(
//                 self.vao,
//                 0,
//                 vbo,
//                 aligned_idx_len,
//                 self.vertex_stride as i32,
//             );
//             gl::VertexArrayElementBuffer(self.vao, vbo);

//             gl::BindVertexArray(self.vao);
//             for cmd_list in draw_list.commands() {
//                 match cmd_list {
//                     imgui::DrawCmd::Elements { count, cmd_params } => {
//                         gl::BindTextureUnit(0, cmd_params.texture_id.id() as _);
//                         gl::Scissor(
//                             (fb_scale[0] * cmd_params.clip_rect[0]) as i32,
//                             (fb_height - fb_scale[1] * cmd_params.clip_rect[3]) as i32,
//                             (fb_scale[0] * cmd_params.clip_rect[2] - cmd_params.clip_rect[0])
//                                 as i32,
//                             (fb_scale[1] * cmd_params.clip_rect[3] - cmd_params.clip_rect[1])
//                                 as i32,
//                         );
//                         let offset = (cmd_params.idx_offset * self.idx_stride) as usize;

//                         gl::DrawElements(
//                             gl::TRIANGLES,
//                             count as i32,
//                             gl::UNSIGNED_SHORT,
//                             offset as *const _,
//                         );
//                     }
//                     _ => {}
//                 }
//             }
//             gl::DeleteBuffers(1, &vbo);
//         }
//     }
// }
