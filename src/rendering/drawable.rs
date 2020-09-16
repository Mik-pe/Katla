use erupt::{vk1_0::CommandBuffer, DeviceLoader};
use mikpe_math::Mat4;

pub trait Drawable {
    fn update(&mut self, device: &DeviceLoader, view: &Mat4, proj: &Mat4);
    fn draw(&self, device: &DeviceLoader, command_buffer: CommandBuffer);
}
