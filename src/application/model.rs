use ash::{vk::CommandBuffer, Device};
use mikpe_math::{Mat4, Vec3};

use crate::rendering::{Drawable, Material, Mesh};

pub struct Model {
    pub mesh: Mesh,
    pub material: Material,
    pub position: Vec3,
}

impl Model {
    // pub fn new() -> Self {}
}

impl Drawable for Model {
    fn update(&mut self, device: &Device, view: &Mat4, proj: &Mat4) {
        let model = Mat4::from_translation(self.position.0);
        self.material
            .upload_pipeline_data(device, view.clone(), proj.clone(), model);
    }

    fn draw(&self, device: &Device, command_buffer: CommandBuffer) {
        self.material.bind(device, command_buffer);
        self.mesh.draw(device, command_buffer);
    }
}
