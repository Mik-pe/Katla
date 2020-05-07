// use gltf::buffer::Data as BufferData;
// use gltf::image::Data as ImageData;
// use gltf::Document;

// use std::collections::HashMap;
// use std::path::{Path, PathBuf};

// #[derive(Clone, Debug)]
// pub struct CachedGLTFModel {
//     pub document: Document,
//     pub buffers: Vec<BufferData>,
//     pub images: Vec<ImageData>,
// }

// impl CachedGLTFModel {
//     fn new<P>(path: P) -> Self
//     where
//         P: AsRef<Path>,
//     {
//         let (document, buffers, images) = gltf::import(path).unwrap();

//         Self {
//             document,
//             buffers,
//             images,
//         }
//     }
// }

// pub struct ModelCache {
//     models: HashMap<PathBuf, CachedGLTFModel>,
// }

// impl ModelCache {
//     pub fn new() -> Self {
//         Self {
//             models: HashMap::new(),
//         }
//     }

//     pub fn read_gltf(&mut self, path: PathBuf) -> CachedGLTFModel {
//         match self.models.get(&path) {
//             Some(model) => model.clone(),
//             None => {
//                 let cached_model = CachedGLTFModel::new(path.as_path());
//                 self.models.insert(path, cached_model.clone());
//                 cached_model
//             }
//         }
//     }
// }
