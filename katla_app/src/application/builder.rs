use std::{cell::RefCell, rc::Rc};

use katla_ecs::{System, SystemExecutionOrder, World};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::ModifiersState;

use crate::{
    application::{Application, ApplicationInfo},
    entities::Camera,
    input::InputMapper,
    rendering::MaterialManager,
    systems::SkeletonUploadSystem,
    util::{FileCache, Timer},
};

#[derive(Default)]
pub struct ApplicationBuilder {
    app_name: String,
    validation_layer_enabled: bool,
    max_frames: Option<usize>,
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

    pub fn single_frame(mut self, on: bool) -> Self {
        // When single_frame is enabled, render some frames for better validation
        self.max_frames = if on { Some(25) } else { None };
        self
    }

    pub fn max_frames(mut self, count: usize) -> Self {
        self.max_frames = Some(count);
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

        let info = ApplicationInfo {
            name: self.app_name,
            validation_layer_enabled: self.validation_layer_enabled,
            max_frames: self.max_frames,
        };
        let mut world = self.world;
        let camera = Rc::new(RefCell::new(Camera::new(&mut world)));

        let app = Application {
            window: None,
            renderer: None,
            camera,
            gltf_cache: FileCache::new(),
            material_manager: MaterialManager::new(),
            stage_upload: false,
            timer: Timer::new(100),
            info,
            world,
            input_mapper: InputMapper::new(),
            current_modifiers: ModifiersState::empty(),
            frame_count: 0,
            skeleton_upload_system: SkeletonUploadSystem::new(),
            fox_entity: None,
            skeleton_registered: false,
        };

        (app, event_loop)
    }
}
