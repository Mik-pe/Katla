use crate::inputcontroller::InputController;
use mikpe_math::{Mat4, Vec3};
use std::{cell::RefCell, rc::Rc};
use winit::event::{DeviceEvent, ElementState, MouseButton, WindowEvent};
use winit::{dpi::PhysicalPosition, event::Event};

pub struct Camera {
    //TODO: Make a quat out of this
    pos: Vec3,
    velocity_dir: Vec3,
    input_dir: Vec3,
    speed: f32,
    yaw: f64,
    pitch: f64,
    looking: bool,
    last_mouse_pos: PhysicalPosition<f64>,
}

//This is not very fun... should find some better way for this in the future.
pub fn setup_camera_bindings(camera: Rc<RefCell<Camera>>, input_controller: &mut InputController) {
    let cam = camera.clone();
    input_controller.assign_axis_callback(
        "SteerVert".into(),
        Box::new(move |value| cam.borrow_mut().handle_steer_vert(value)),
    );
    let cam = camera.clone();
    input_controller.assign_axis_callback(
        "SteerHori".into(),
        Box::new(move |value| cam.borrow_mut().handle_steer_horiz(value)),
    );
    input_controller.assign_axis_callback(
        "SteerFwd".into(),
        Box::new(move |value| camera.borrow_mut().handle_steer_fwd(value)),
    );
}

impl Camera {
    pub fn handle_steer_horiz(&mut self, value: f32) {
        self.input_dir[0] = value;
    }
    pub fn handle_steer_vert(&mut self, value: f32) {
        self.input_dir[1] = value;
    }
    pub fn handle_steer_fwd(&mut self, value: f32) {
        self.input_dir[2] = value;
    }

    pub fn new() -> Self {
        let camera = Self {
            pos: Vec3::new(0.0, 0.0, -1.0),
            velocity_dir: Vec3::new(0.0, 0.0, 0.0),
            input_dir: Vec3::new(0.0, 0.0, 0.0),
            speed: 100.0,
            yaw: 0.0,
            pitch: 0.0,
            looking: false,
            last_mouse_pos: PhysicalPosition::new(0.0, 0.0),
        };

        camera
    }

    pub fn handle_event(&mut self, event: &Event<()>) {
        match event {
            Event::WindowEvent {
                window_id: _,
                event,
            } => match event {
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
                _ => {}
            },
            Event::DeviceEvent {
                device_id: _,
                event,
            } => match event {
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

    fn lerp_vec3(old_velocity_dir: Vec3, to_velocity_dir: Vec3, ratio: f32) -> Vec3 {
        let new_velocity_dir = old_velocity_dir + (to_velocity_dir - old_velocity_dir).mul(ratio);
        new_velocity_dir
    }

    pub fn update(&mut self, dt: f32) {
        let velocity_dir = mikpe_math::mat4_mul_vec3(&self.get_view_rotation(), &self.input_dir);

        self.velocity_dir = Self::lerp_vec3(self.velocity_dir, velocity_dir, 10.0 * dt);

        self.pos = self.pos + self.velocity_dir.mul(self.speed * dt);
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
