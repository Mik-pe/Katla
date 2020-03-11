use bitflags::bitflags;
use glutin::event::{ElementState, Event, MouseButton, VirtualKeyCode, WindowEvent};
use mikpe_math::{Mat4, Vec3};

bitflags! {
    struct Movement: u32
    {
        const STILL     = 0b0000_0000;
        const FORWARD   = 0b0000_0001;
        const BACKWARDS = 0b0000_0010;
        const LEFT      = 0b0000_0100;
        const RIGHT     = 0b0000_1000;
        const UP        = 0b0001_0000;
        const DOWN      = 0b0010_0000;
    }
}
pub struct Camera {
    //TODO: Make a quat out of this
    pos: Vec3,
    velocity: Vec3,
    yaw: f32,
    pitch: f32,
    looking: bool,
    current_movement: Movement,
}

impl Camera {
    pub fn new() -> Self {
        Self {
            pos: Vec3::new(0.0, 0.0, 0.0),
            velocity: Vec3::new(0.0, 0.0, 0.0),
            yaw: 0.0,
            pitch: 0.0,
            looking: false,
            current_movement: Movement::STILL,
        }
    }
    pub fn update(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::MouseInput {
                device_id: _,
                state,
                button,
                modifiers: _,
            } => {
                if *button == MouseButton::Right && *state == ElementState::Pressed {
                    self.looking = true;
                } else if *button == MouseButton::Right && *state == ElementState::Released {
                    self.looking = false;
                }
            }
            _ => {}
        }
    }

    pub fn getViewMatrix(&self) {
        let view_rot = Mat4::from_rotaxis(&(self.yaw as f32), Vec3::new(0.0, 1.0, 0.0).0).mul(
            &Mat4::from_rotaxis(&(self.pitch as f32), Vec3::new(1.0, 0.0, 0.0).0),
        );
    }
}
