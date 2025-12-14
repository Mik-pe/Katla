use katla_math::Quat;
pub(crate) use katla_math::Vec3;
use std::{cell::RefCell, rc::Rc};
use winit::event::{DeviceEvent, ElementState, MouseButton, WindowEvent};

use crate::input::{InputController, InputMapping};

use super::CameraController;

#[derive(Debug)]
pub struct FpsControl {
    velocity_dir: Vec3,
    input_dir: Vec3,
    speed: f32,
    yaw: f64,
    pitch: f64,
    looking: bool,
}

impl FpsControl {
    pub fn handle_steer_horiz(&mut self, value: f32) {
        self.input_dir[0] = value;
    }
    pub fn handle_steer_vert(&mut self, value: f32) {
        self.input_dir[1] = value;
    }
    pub fn handle_steer_fwd(&mut self, value: f32) {
        self.input_dir[2] = value;
    }

    pub fn handle_window_event(&mut self, event: &WindowEvent) {
        if let WindowEvent::MouseInput {
            device_id: _,
            state,
            button,
        } = event
        {
            if button == &MouseButton::Right && state == &ElementState::Pressed {
                self.looking = true;
            } else if button == &MouseButton::Right && state == &ElementState::Released {
                self.looking = false;
            }
        }
    }

    pub fn handle_device_event(&mut self, event: &DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event {
            if self.looking {
                //Since -y is up for now, this is valid:
                self.yaw += 0.005 * delta.0;
                self.pitch += 0.005 * delta.1;
                self.pitch = self.pitch.clamp(
                    -std::f64::consts::FRAC_PI_2 + 0.01,
                    std::f64::consts::FRAC_PI_2 - 0.01,
                );
            }
        }
    }
}

impl CameraController for FpsControl {
    fn tick_camera(&mut self, camera: &mut super::Camera, dt: f32) {
        let rotation = Quat::new_from_yaw_pitch(self.yaw as f32, self.pitch as f32);

        let velocity_dir = katla_math::mat4_mul_vec3(&camera.get_view_rotation(), &self.input_dir);

        self.velocity_dir = Vec3::lerp(self.velocity_dir, velocity_dir, 7.0 * dt);

        camera.pos = camera.pos + self.velocity_dir.mul(self.speed * dt);
        camera.quat = rotation;
    }
}

impl Default for FpsControl {
    fn default() -> Self {
        Self {
            velocity_dir: Default::default(),
            input_dir: Default::default(),
            speed: 100.0,
            yaw: 0.0,
            pitch: 0.0,
            looking: false,
        }
    }
}

// This is not very fun... should find some better way for this in the future.
// Probably through internal mutability or something else..?
pub fn setup_camera_bindings(
    controls: Rc<RefCell<FpsControl>>,
    input_controller: &mut InputController,
) {
    let control = controls.clone();
    input_controller.assign_axis_callback(
        InputMapping::MoveVertical,
        Box::new(move |value| control.borrow_mut().handle_steer_vert(value)),
    );
    let control = controls.clone();
    input_controller.assign_axis_callback(
        InputMapping::MoveHorizontal,
        Box::new(move |value| control.borrow_mut().handle_steer_horiz(value)),
    );
    let control = controls.clone();
    input_controller.assign_axis_callback(
        InputMapping::MoveForward,
        Box::new(move |value| control.borrow_mut().handle_steer_fwd(value)),
    );
}
