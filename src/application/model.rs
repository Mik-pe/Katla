use ash::Device;
use katla_math::{Mat4, Transform};

use crate::{
    renderer::vulkan::CommandBuffer,
    rendering::{Drawable, Material, Mesh},
};

pub struct Model {
    pub mesh: Mesh,
    pub material: Material,
    pub transform: Transform,
}

impl Model {
    // pub fn new() -> Self {}
}

impl Drawable for Model {
    fn update(&mut self, device: &Device, view: &Mat4, proj: &Mat4) {
        let model = self.transform.make_mat4();
        self.material
            .upload_pipeline_data(device, view.clone(), proj.clone(), model);
    }

    fn draw(&self, command_buffer: &CommandBuffer) {
        self.material.bind(command_buffer);
        self.mesh.draw(command_buffer);
    }
}
