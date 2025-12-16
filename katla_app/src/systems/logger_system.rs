use katla_ecs::{ComponentStorageManager, System};

pub struct LoggerSystem;

impl System for LoggerSystem {
    fn update(&mut self, _storage: &mut ComponentStorageManager, _delta_time: f32) {
        // println!("LoggerSystem updated");
    }
}
