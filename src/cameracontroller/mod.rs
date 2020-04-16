use bitflags::bitflags;
use mikpe_math::{Mat4, Vec3};
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton, VirtualKeyCode, WindowEvent};

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
            yaw: 0.0,
            pitch: 0.0,
            looking: false,
            current_movement: Movement::STILL,
            last_mouse_pos: PhysicalPosition::new(0.0, 0.0),
        }
    }
    pub fn handle_event(&mut self, event: &WindowEvent) {
        match *event {
            WindowEvent::MouseInput {
                device_id: _,
                state,
                button,
                ..
            } => {
                if button == MouseButton::Right && state == ElementState::Pressed {
                    self.looking = true;
                } else if button == MouseButton::Right && state == ElementState::Released {
                    self.looking = false;
                }
            }
            WindowEvent::CursorMoved {
                device_id: _,
                position,
                ..
            } => {
                if self.looking {
                    let delta_x = position.x - self.last_mouse_pos.x;
                    let delta_y = position.y - self.last_mouse_pos.y;
                    self.yaw -= 0.01 * delta_x;
                    self.pitch -= 0.01 * delta_y;
                    self.pitch = self
                        .pitch
                        .max(-std::f64::consts::FRAC_PI_2)
                        .min(std::f64::consts::FRAC_PI_2);
                }
                self.last_mouse_pos = position;
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
        }
    }

    pub fn update(&mut self, _dt: f32) {
        self.velocity = Vec3::new(0.0, 0.0, 0.0);
        if self.current_movement.contains(Movement::FORWARD) {
            self.velocity[2] += 1.0;
        }
        if self.current_movement.contains(Movement::BACKWARDS) {
            self.velocity[2] -= 1.0;
        }
        if self.current_movement.contains(Movement::DOWN) {
            self.velocity[1] -= 1.0;
        }
        if self.current_movement.contains(Movement::UP) {
            self.velocity[1] += 1.0;
        }
        if self.current_movement.contains(Movement::LEFT) {
            self.velocity[0] -= 1.0;
        }
        if self.current_movement.contains(Movement::RIGHT) {
            self.velocity[0] += 1.0;
        }
        self.velocity =
            mikpe_math::mat4_mul_vec3(&self.get_view_rotation(), &self.velocity.normalize());
        self.pos = self.pos + self.velocity.mul(0.1);
    }

    // Note to self:
    // This is valid since we are doing some assumptions w.r.t. rotation of the surfacetransformation
    fn get_view_rotation(&self) -> Mat4 {
        Mat4::from_rotaxis(&(self.yaw as f32), [0.0, -1.0, 0.0])
            .mul(&Mat4::from_rotaxis(&(self.pitch as f32), [-1.0, 0.0, 0.0]))
    }

    pub fn get_cam_pos(&self) -> Vec3 {
        self.pos.clone()
    }

    pub fn get_view_mat(&self) -> Mat4 {
        let view_rot = self.get_view_rotation();
        Mat4::create_lookat(
            self.pos.clone(),
            self.pos.clone() + mikpe_math::mat4_mul_vec3(&view_rot, &Vec3::new(0.0, 0.0, 1.0)),
            mikpe_math::mat4_mul_vec3(&view_rot, &Vec3::new(0.0, -1.0, 0.0)),
        )
    }
}
