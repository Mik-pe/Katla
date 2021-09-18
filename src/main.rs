mod application;
mod cameracontroller;
mod inputcontroller;
mod renderer;
mod rendering;
mod util;

use application::ApplicationBuilder;

use winit::event::VirtualKeyCode;

fn main() {
    let application = ApplicationBuilder::new()
        .with_name("Katla")
        .validation_layer(true)
        //TODO: This seems to be typical to reside in configuration files:
        .with_axis_input(VirtualKeyCode::A, "SteerHori", -1.0)
        .with_axis_input(VirtualKeyCode::D, "SteerHori", 1.0)
        .with_axis_input(VirtualKeyCode::S, "SteerFwd", -1.0)
        .with_axis_input(VirtualKeyCode::W, "SteerFwd", 1.0)
        .with_axis_input(VirtualKeyCode::Q, "SteerVert", -1.0)
        .with_axis_input(VirtualKeyCode::E, "SteerVert", 1.0)
        .build();

    //TODO: add some sort of system, so that we can run stuff in-loop?
    application.run();
}
