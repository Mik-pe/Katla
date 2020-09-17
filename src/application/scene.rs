use crate::rendering::Drawable;
use erupt::{vk1_0::CommandBuffer, vk1_0::Vk10DeviceLoaderExt, DeviceLoader};
use mikpe_math::{Mat4, Vec3};
use std::rc::Rc;

pub struct Player {
    pub position: Vec3,
}

pub struct SceneObject {
    pub position: Vec3,
    pub drawable: Box<dyn Drawable>,
    pub child: Option<Rc<SceneObject>>,
}
pub struct Scene {
    pub player: Player,
    pub scene_objects: Vec<SceneObject>,
}

impl SceneObject {
    pub fn new(drawable: Box<dyn Drawable>) -> Self {
        let position = Vec3::new(0.0, 0.0, 0.0);
        Self {
            position,
            drawable,
            child: None,
        }
    }
}

impl Scene {
    pub fn new() -> Self {
        let player = Player {
            position: Vec3::new(0.0, 0.0, 0.0),
        };
        let scene_objects = vec![];
        Self {
            player,
            scene_objects,
        }
    }

    pub fn update(&mut self, device: &DeviceLoader, proj: &Mat4) {
        let lookat = Mat4::create_lookat(
            self.player.position,
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(0.0, -1.0, 0.0),
        );
        let view = lookat.inverse();
        for object in &mut self.scene_objects {
            let draw_mut = &mut object.drawable;
            draw_mut.update(device, &view, &proj);
        }
    }

    pub fn add_object(&mut self, scene_object: SceneObject) {
        self.scene_objects.push(scene_object);
    }

    pub fn render(&self, device: &DeviceLoader, command_buffer: CommandBuffer) {
        for object in &self.scene_objects {
            object.drawable.draw(device, command_buffer);
        }
    }
}
