use crate::rendering::Drawable;
use mikpe_math::Vec3;
use std::rc::Rc;

pub struct Player {
    pub position: Vec3,
}

pub struct SceneObject {
    pub position: Vec3,
    pub drawable: Rc<dyn Drawable>,
    pub child: Option<Rc<SceneObject>>,
}
pub struct Scene {
    pub player: Player,
    pub scene_objects: Vec<SceneObject>,
}

impl SceneObject {
    pub fn new(drawable: Rc<dyn Drawable>) -> Self {
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

    pub fn update(&mut self) {}

    pub fn add_object(&mut self, scene_object: SceneObject) {
        self.scene_objects.push(scene_object);
    }

    pub fn render(&self) {
        for object in &self.scene_objects {
            object.drawable.draw();
        }
    }
}
