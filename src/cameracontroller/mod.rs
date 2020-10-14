use bitflags::bitflags;
use mikpe_math::{Mat4, Vec3};
use winit::event::{DeviceEvent, ElementState, MouseButton, VirtualKeyCode, WindowEvent};
use winit::{dpi::PhysicalPosition, event::Event};

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
    speed: f32,
    yaw: f64,
    pitch: f64,
    looking: bool,
    current_movement: Movement,
    last_mouse_pos: PhysicalPosition<f64>,
}

impl Camera {
    pub fn new() -> Self {
        Self {
            pos: Vec3::new(0.0, 0.0, -1.0),
            velocity: Vec3::new(0.0, 0.0, 0.0),
            speed: 100.0,
            yaw: 0.0,
            pitch: 0.0,
            looking: false,
            current_movement: Movement::STILL,
            last_mouse_pos: PhysicalPosition::new(0.0, 0.0),
        }
    }
    pub fn handle_event(&mut self, event: &Event<()>) {
        match event {
            Event::WindowEvent { window_id, event } => match event {
                WindowEvent::MouseInput {
                    device_id: _,
                    state,
                    button,
                    ..
                } => {
                    if button == &MouseButton::Right && state == &ElementState::Pressed {
                        self.looking = true;
                    } else if button == &MouseButton::Right && state == &ElementState::Released {
                        self.looking = false;
                    }
                }
                WindowEvent::CursorMoved {
                    device_id: _,
                    position,
                    ..
                } => {
                    self.last_mouse_pos = *position;
                }
                WindowEvent::KeyboardInput {
                    device_id: _,
                    input,
                    is_synthetic: _,
                } => {
                    if input.state == ElementState::Pressed {
                        match input.virtual_keycode {
                            Some(keycode) => match keycode {
                                VirtualKeyCode::W => {
                                    self.current_movement |= Movement::FORWARD;
                                }
                                VirtualKeyCode::S => {
                                    self.current_movement |= Movement::BACKWARDS;
                                }
                                VirtualKeyCode::A => {
                                    self.current_movement |= Movement::LEFT;
                                }
                                VirtualKeyCode::D => {
                                    self.current_movement |= Movement::RIGHT;
                                }
                                VirtualKeyCode::Q => {
                                    self.current_movement |= Movement::DOWN;
                                }
                                VirtualKeyCode::E => {
                                    self.current_movement |= Movement::UP;
                                }
                                _ => {}
                            },
                            None => {}
                        }
                    }
                    if input.state == ElementState::Released {
                        match input.virtual_keycode {
                            Some(keycode) => match keycode {
                                VirtualKeyCode::W => {
                                    self.current_movement -= Movement::FORWARD;
                                }
                                VirtualKeyCode::S => {
                                    self.current_movement -= Movement::BACKWARDS;
                                }
                                VirtualKeyCode::A => {
                                    self.current_movement -= Movement::LEFT;
                                }
                                VirtualKeyCode::D => {
                                    self.current_movement -= Movement::RIGHT;
                                }
                                VirtualKeyCode::Q => {
                                    self.current_movement -= Movement::DOWN;
                                }
                                VirtualKeyCode::E => {
                                    self.current_movement -= Movement::UP;
                                }
                                _ => {}
                            },
                            None => {}
                        }
                    }
                }
                _ => {}
            },
            Event::DeviceEvent { device_id, event } => match event {
                DeviceEvent::MouseMotion { delta } => {
                    if self.looking {
                        //Since -y is up for now, this is valid:
                        self.yaw += 0.005 * delta.0;
                        self.pitch -= 0.005 * delta.1;
                        self.pitch = self
                            .pitch
                            .max(-std::f64::consts::FRAC_PI_2 + 0.01)
                            .min(std::f64::consts::FRAC_PI_2 - 0.01);
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn lerp_vec3(old_velocity: Vec3, to_velocity: Vec3, ratio: f32) -> Vec3 {
        let new_velocity = old_velocity + (to_velocity - old_velocity).mul(ratio);
        new_velocity
    }

    pub fn update(&mut self, dt: f32) {
        // let mut up_velocity = 0.0f32;
        let mut velocity = Vec3::new(0.0, 0.0, 0.0);
        if self.current_movement.contains(Movement::FORWARD) {
            velocity[2] += 1.0;
        }
        if self.current_movement.contains(Movement::BACKWARDS) {
            velocity[2] -= 1.0;
        }
        if self.current_movement.contains(Movement::DOWN) {
            velocity[1] -= 1.0;
        }
        if self.current_movement.contains(Movement::UP) {
            velocity[1] += 1.0;
        }
        if self.current_movement.contains(Movement::LEFT) {
            velocity[0] -= 1.0;
        }
        if self.current_movement.contains(Movement::RIGHT) {
            velocity[0] += 1.0;
        }
        velocity = mikpe_math::mat4_mul_vec3(&self.get_view_rotation(), &velocity).mul(self.speed);

        self.velocity = Self::lerp_vec3(self.velocity, velocity, 10.0 * dt);

        self.pos = self.pos + self.velocity.mul(dt);
    }

    // Note to self:
    // This is valid since we are doing some assumptions w.r.t. rotation of the surfacetransformation
    fn get_view_rotation(&self) -> Mat4 {
        let yaw = Mat4::from_rotaxis(&(self.yaw as f32), [0.0, 1.0, 0.0]);
        let pitch = Mat4::from_rotaxis(&(self.pitch as f32), [-1.0, 0.0, 0.0]);
        yaw.mul(&pitch)
    }

    // pub fn get_cam_pos(&self) -> Vec3 {
    //     self.pos.clone()
    // }

    pub fn get_view_mat(&self) -> Mat4 {
        let fwd = Vec3::new(0.0, 0.0, 1.0);
        let to = mikpe_math::mat4_mul_vec3(&self.get_view_rotation(), &fwd);

        Mat4::create_lookat(
            self.pos.clone(),
            self.pos.clone() + to,
            Vec3::new(0.0, -1.0, 0.0),
        )
    }
}
