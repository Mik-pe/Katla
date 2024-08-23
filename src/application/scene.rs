use crate::rendering::Drawable;
use katla_math::{Mat4, Sphere, Vec3};
use katla_vulkan::CommandBuffer;
use std::{rc::Rc, sync::Arc};

pub struct Player {
    pub position: Vec3,
}

pub struct SceneObject {
    pub position: Vec3,
    pub drawable: Box<dyn Drawable>,
    pub child: Option<Rc<SceneObject>>,
    pub bounds: Sphere,
}
pub struct Scene {
    pub player: Player,
    pub scene_objects: Vec<SceneObject>,
}

impl SceneObject {
    pub fn new(drawable: Box<dyn Drawable>, bounds: Sphere) -> Self {
        let position = Vec3::new(0.0, 0.0, 0.0);
        Self {
            position,
            drawable,
            child: None,
            bounds,
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

    // pub fn add_model(&mut self, _model_path: PathBuf) {
    //     todo!("Add a model to the scene, talk with the renderer?");
    // }

    pub fn teardown(&mut self) {
        self.scene_objects.clear();
    }

    pub fn update(&mut self, proj: &Mat4, view: &Mat4) {
        for object in &mut self.scene_objects {
            object.drawable.update(&view, &proj);
        }
    }

    pub fn add_object(&mut self, scene_object: SceneObject) {
        self.scene_objects.push(scene_object);
    }

    pub fn render(&self, command_buffer: &CommandBuffer) {
        for object in &self.scene_objects {
            object.drawable.draw(command_buffer);
        }
        command_buffer.end_render_pass();
        command_buffer.end_command();
    }
}
