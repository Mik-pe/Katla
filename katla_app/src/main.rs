use katla::application::ApplicationBuilder;

use katla::input::InputMapping;
use winit::keyboard::KeyCode;

fn main() {
    let (mut application, event_loop) = ApplicationBuilder::new()
        .with_name("Katla")
        .validation_layer(true)
        //TODO: This seems to be typical to reside in configuration files:
        .with_axis_input(KeyCode::KeyA, InputMapping::MoveHorizontal, -1.0)
        .with_axis_input(KeyCode::KeyD, InputMapping::MoveHorizontal, 1.0)
        .with_axis_input(KeyCode::KeyS, InputMapping::MoveForward, -1.0)
        .with_axis_input(KeyCode::KeyW, InputMapping::MoveForward, 1.0)
        .with_axis_input(KeyCode::KeyQ, InputMapping::MoveVertical, -1.0)
                    .with_axis_input(KeyCode::KeyE, InputMapping::MoveVertical, 1.0)
        .build();

    //TODO: add some sort of system, so that we can run stuff in-loop?
    application.init();
    event_loop.run_app(&mut application).unwrap();
}
