use std::path::PathBuf;

pub mod asset_watcher;
pub mod background_loader;
pub mod cache;
pub mod config;
pub mod gltf_material;
pub mod gltf_parser;
pub mod modelcache;
pub mod stl_parser;
pub mod timer;

#[cfg(feature = "editor")]
pub use asset_watcher::*;
#[cfg(feature = "editor")]
pub use background_loader::*;
pub use cache::*;
pub use config::*;
pub use modelcache::*;
pub use stl_parser::*;
pub use timer::*;

/// Cache for loaded glTF models with a boxed loader function.
pub type GltfCache = FileCache<GLTFModel, Box<dyn Fn(&PathBuf) -> GLTFModel>>;
