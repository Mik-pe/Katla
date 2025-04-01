use katla_math::Mat4;
use katla_vulkan::CommandBuffer;

pub trait Drawable {
    fn update(&mut self, view: &Mat4, proj: &Mat4, dt: f32);
    fn draw(&self, command_buffer: &CommandBuffer);
}
