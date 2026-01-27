use katla::application::ApplicationBuilder;

use katla::systems::{FlyCameraLookSystem, PhysicsSystem, VelocitySystem};
use katla_ecs::System;

fn main() {
    let systems: Vec<Box<dyn System>> = vec![
        Box::new(FlyCameraLookSystem),
        Box::new(VelocitySystem),
        Box::new(PhysicsSystem),
    ];

    let (mut application, event_loop) = ApplicationBuilder::new()
        .with_name("Katla")
        .validation_layer(true)
        .with_systems(systems)
        .build();

    application.init();
    event_loop.run_app(&mut application).unwrap();
}
