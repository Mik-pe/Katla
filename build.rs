use gl_generator::*;
use std::env;
use std::fs::File;
use std::path::*;

fn main() {
    let dest = env::var("OUT_DIR").unwrap();
    let mut file = File::create(&Path::new(&dest).join("generated_gl.rs")).unwrap();
    let extensions = ["GL_ARB_debug_output"];

    Registry::new(Api::Gl, (4, 5), Profile::Core, Fallbacks::All, extensions)
        .write_bindings(GlobalGenerator, &mut file)
        .unwrap();
}
