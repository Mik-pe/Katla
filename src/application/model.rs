use katla_math::{Mat4, Transform};
use katla_vulkan::CommandBuffer;

use crate::rendering::{Drawable, Material, Mesh};

pub struct Model {
    pub meshes: Vec<Mesh>,
    pub material: Material,
    pub transform: Transform,
}

impl Model {
    // pub fn new() -> Self {}
}

impl Drawable for Model {
    fn update(&mut self, view: &Mat4, proj: &Mat4) {
        for mesh in &mut self.meshes {
            mesh.update(view, proj);
        }
    }

    fn draw(&self, command_buffer: &CommandBuffer) {
        for mesh in &self.meshes {
            mesh.draw(command_buffer);
        }
    }
}
