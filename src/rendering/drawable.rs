use ash::{vk, Device};
use mikpe_math::Mat4;

pub trait Drawable {
    fn update(&mut self, device: &Device, view: &Mat4, proj: &Mat4);
    fn draw(&self, device: &Device, command_buffer: vk::CommandBuffer);
}
