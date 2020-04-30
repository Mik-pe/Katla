pub mod drawable;
pub mod material;
pub mod mesh;
pub mod meshbuffer;
pub mod pipeline;
pub mod shaders;
pub mod shared_resources;
pub mod texture;
pub mod vertextypes;

pub use drawable::Drawable;
pub use material::*;
pub use mesh::*;
pub use meshbuffer::*;
pub use pipeline::*;
pub use shaders::*;
pub use shared_resources::*;
pub use texture::*;
pub use vertextypes::*;

#[macro_export]
macro_rules! glchk {
    ($($s:stmt;)*) => {
        use gl;
        $(
            $s
            if cfg!(debug_assertions) {
                let err = gl::GetError();
                if err != gl::NO_ERROR {
                    let err_str = match err {
                        gl::INVALID_ENUM => "GL_INVALID_ENUM",
                        gl::INVALID_VALUE => "GL_INVALID_VALUE",
                        gl::INVALID_OPERATION => "GL_INVALID_OPERATION",
                        gl::INVALID_FRAMEBUFFER_OPERATION => "GL_INVALID_FRAMEBUFFER_OPERATION",
                        gl::OUT_OF_MEMORY => "GL_OUT_OF_MEMORY",
                        gl::STACK_UNDERFLOW => "GL_STACK_UNDERFLOW",
                        gl::STACK_OVERFLOW => "GL_STACK_OVERFLOW",
                        _ => "unknown error"
                    };
                    println!("{}:{} - {} caused {}",
                             file!(),
                             line!(),
                             stringify!($s),
                             err_str);
                }
            }
        )*
    }
}
