use std::path::PathBuf;

pub mod background_loader;
pub mod cache;
pub mod config;
pub mod gltf_material;
pub mod gltf_parser;
pub mod modelcache;
#[cfg(feature = "vulkan")]
pub mod stl_parser;
pub mod timer;

#[cfg(feature = "editor")]
pub use background_loader::*;
pub use cache::*;
pub use config::*;
pub use modelcache::*;
#[cfg(feature = "vulkan")]
pub use stl_parser::*;
pub use timer::*;

/// Cache for loaded glTF models with a boxed loader function.
pub type GltfCache = FileCache<GLTFModel, Box<dyn Fn(&PathBuf) -> GLTFModel>>;
