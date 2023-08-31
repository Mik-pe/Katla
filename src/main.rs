mod application;
mod cameracontroller;
mod input;
mod rendering;
mod util;

use application::ApplicationBuilder;

use input::InputMapping;
use winit::event::VirtualKeyCode;

fn main() {
    let application = ApplicationBuilder::new()
        .with_name("Katla")
        .validation_layer(true)
        //TODO: This seems to be typical to reside in configuration files:
        .with_axis_input(VirtualKeyCode::A, InputMapping::MoveHorizontal, -1.0)
        .with_axis_input(VirtualKeyCode::D, InputMapping::MoveHorizontal, 1.0)
        .with_axis_input(VirtualKeyCode::S, InputMapping::MoveForward, -1.0)
        .with_axis_input(VirtualKeyCode::W, InputMapping::MoveForward, 1.0)
        .with_axis_input(VirtualKeyCode::Q, InputMapping::MoveVertical, -1.0)
        .with_axis_input(VirtualKeyCode::E, InputMapping::MoveVertical, 1.0)
        .build();

    //TODO: add some sort of system, so that we can run stuff in-loop?
    application.run();
}
