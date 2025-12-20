use std::{cell::RefCell, rc::Rc};

use katla_ecs::{System, SystemExecutionOrder, World};
use winit::{
    event_loop::{ControlFlow, EventLoop},
    keyboard::KeyCode,
};

use crate::{
    application::{Application, ApplicationInfo},
    cameracontroller::{self, fpscontrol::FpsControl, Camera},
    input::InputController,
    util::{FileCache, Timer},
};

#[derive(Default)]
pub struct ApplicationBuilder {
    app_name: String,
    validation_layer_enabled: bool,
    controller: Rc<RefCell<FpsControl>>,
    input_controller: InputController,
    world: World,
}

impl ApplicationBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.app_name = name.into();
        self
    }

    pub fn validation_layer(mut self, on: bool) -> Self {
        self.validation_layer_enabled = on;
        self
    }

    pub fn with_axis_input<S>(mut self, key_event: KeyCode, input: S, value: f32) -> Self
    where
        S: Into<u32>,
    {
        self.input_controller
            .assign_axis_input(key_event, input.into(), value);
        self
    }

    pub fn with_system(mut self, system: Box<dyn System>, order: SystemExecutionOrder) -> Self {
        self.world.register_system(system, order);
        self
    }

    pub fn with_systems(mut self, systems: Vec<Box<dyn System>>) -> Self {
        for system in systems {
            self.world
                .register_system(system, SystemExecutionOrder::default());
        }
        self
    }

    fn build_event_loop() -> EventLoop<()> {
        let event_loop = EventLoop::new().unwrap();
        event_loop.set_control_flow(ControlFlow::Poll);
        event_loop
    }

    pub fn build(self) -> (Application, EventLoop<()>) {
        let event_loop = Self::build_event_loop();

        let mut input_controller = self.input_controller;

        cameracontroller::fpscontrol::setup_camera_bindings(
            self.controller.clone(),
            &mut input_controller,
        );
        let info = ApplicationInfo {
            name: self.app_name,
            validation_layer_enabled: self.validation_layer_enabled,
        };
        let mut world = self.world;
        let camera = Rc::new(RefCell::new(Camera::new(&mut world)));

        let app = Application {
            window: None,
            renderer: None,
            camera,
            controller: self.controller,
            input_controller,
            gltf_cache: FileCache::new(),
            stage_upload: false,
            timer: Timer::new(100),
            info,
            world,
        };

        (app, event_loop)
    }
}
