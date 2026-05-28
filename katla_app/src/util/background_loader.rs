#![cfg_attr(not(feature = "editor"), allow(dead_code))]
//! Background asset loader system.
//!
//! Loads assets on a background thread (CPU work) and returns results
//! for GPU upload on the main thread (Vulkan requirement).
//!
//! # Architecture
//!
//! ```text
//! Main Thread                         Background Thread
//!     |                                    |
//!     |  LoadRequest (path, type)          |
//!     |----------------------------------->|  switch on asset_type:
//!     |                                    |    Image: image::open(), resize()
//!     |                                    |    Model: gltf::import(), parse()
//!     |  LoadResult (data)                 |    Shader: fs::read(), validate()
//!     |<-----------------------------------|
//!     |                                    |
//!     |  GPU upload (Vulkan-safe)          |
//!     |  - Texture::create_image()         |
//!     |  - Mesh::upload_buffers()          |
//!     |  - Update UI with results          |
//!     v
//! ```

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

use log::{debug, warn};
use rayon::ThreadPool;

/// Unique identifier for load requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LoadId(pub u64);

/// Types of assets the loader can handle.
#[derive(Debug, Clone)]
pub enum LoadRequest {
    /// Load image as thumbnail (resized, for asset browser).
    ImageThumbnail {
        id: LoadId,
        path: PathBuf,
        max_size: u32,
    },

    /// Load a full-size texture with optional mipmap generation.
    FullTexture {
        id: LoadId,
        path: PathBuf,
        generate_mipmaps: bool,
    },

    /// Load a glTF model file.
    GltfModel { id: LoadId, path: PathBuf },
}

/// Results from background loading.
#[derive(Debug)]
pub enum LoadResult {
    /// Image thumbnail ready for GPU upload.
    ImageThumbnailLoaded {
        id: LoadId,
        path: PathBuf,
        width: u32,
        height: u32,
        pixels: Vec<u8>, // RGBA8
    },

    /// Full-size texture loaded.
    FullTextureLoaded {
        id: LoadId,
        width: u32,
        height: u32,
        mip_levels: u32,
        pixels: Vec<u8>, // RGBA8
    },

    /// glTF model loaded (CPU-side data only, GPU upload happens on main thread).
    GltfModelLoaded { id: LoadId, path: PathBuf },

    /// Load failed.
    Failed {
        id: LoadId,
        path: PathBuf,
        error: String,
    },
}

/// Entry for a loaded thumbnail in the cache.
#[derive(Debug, Clone)]
pub struct ThumbnailEntry {
    /// Whether the thumbnail has been uploaded to GPU.
    pub uploaded: bool,
}

/// Background asset loader.
///
/// Uses a rayon thread pool to process load requests and returns
/// results via a channel. Call `poll()` each frame to check for completed loads.
pub struct BackgroundLoader {
    /// Rayon thread pool for background loading.
    pool: ThreadPool,
    /// Receiver for completed load results.
    result_receiver: Receiver<LoadResult>,
    /// Sender for load results (cloned into each pool task).
    result_sender: Sender<LoadResult>,
    /// Paths currently being loaded.
    pending_loads: HashMap<LoadId, PathBuf>,
    /// Cache of loaded thumbnails by path.
    thumbnail_cache: HashMap<PathBuf, ThumbnailEntry>,
    /// Next unique load ID.
    next_load_id: u64,
}

impl BackgroundLoader {
    /// Create a new background loader with a rayon thread pool.
    pub fn new() -> Self {
        let (result_tx, result_rx) = mpsc::channel::<LoadResult>();

        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .thread_name(|idx| format!("bg-loader-{idx}"))
            .build()
            .expect("Failed to create background loader thread pool");

        Self {
            pool,
            result_receiver: result_rx,
            result_sender: result_tx,
            pending_loads: HashMap::new(),
            thumbnail_cache: HashMap::new(),
            next_load_id: 1,
        }
    }

    /// Submit a single load request to the thread pool.
    pub fn submit(&self, request: LoadRequest) {
        let result_tx = self.result_sender.clone();
        self.pool.spawn(move || {
            let result = match request {
                LoadRequest::ImageThumbnail { id, path, max_size } => {
                    Self::load_image_thumbnail(id, &path, max_size)
                }
                LoadRequest::FullTexture {
                    id,
                    path,
                    generate_mipmaps,
                } => Self::load_full_texture(id, &path, generate_mipmaps),
                LoadRequest::GltfModel { id, path } => Self::load_gltf_model(id, &path),
            };

            if result_tx.send(result).is_err() {
                debug!("Background loader: result channel closed, dropping result");
            }
        });
    }

    /// Load an image and resize it for thumbnail display.
    fn load_image_thumbnail(id: LoadId, path: &PathBuf, max_size: u32) -> LoadResult {
        debug!("Loading thumbnail: {:?}", path);

        match image::open(path) {
            Ok(img) => {
                // Resize if larger than max_size
                let (orig_width, orig_height) = (img.width(), img.height());
                let (new_width, new_height) = if orig_width > max_size || orig_height > max_size {
                    let ratio = (max_size as f32 / orig_width.max(orig_height) as f32).min(1.0);
                    (
                        (orig_width as f32 * ratio) as u32,
                        (orig_height as f32 * ratio) as u32,
                    )
                } else {
                    (orig_width, orig_height)
                };

                // Resize and convert to RGBA8
                let resized =
                    img.resize(new_width, new_height, image::imageops::FilterType::Lanczos3);
                let rgba = resized.to_rgba8();
                let (width, height) = rgba.dimensions();
                let pixels = rgba.into_raw();

                debug!(
                    "Loaded thumbnail: {:?} ({}x{} -> {}x{})",
                    path, orig_width, orig_height, width, height
                );

                LoadResult::ImageThumbnailLoaded {
                    id,
                    path: path.clone(),
                    width,
                    height,
                    pixels,
                }
            }
            Err(e) => {
                warn!("Failed to load thumbnail {:?}: {}", path, e);
                LoadResult::Failed {
                    id,
                    path: path.clone(),
                    error: e.to_string(),
                }
            }
        }
    }

    /// Load a full-size texture with optional mipmap generation.
    fn load_full_texture(id: LoadId, path: &PathBuf, generate_mipmaps: bool) -> LoadResult {
        debug!("Loading full texture: {:?}", path);

        match image::open(path) {
            Ok(img) => {
                let rgba = img.to_rgba8();
                let (width, height) = rgba.dimensions();
                let pixels = rgba.into_raw();

                let mip_levels = if generate_mipmaps {
                    (width.max(height) as f32).log2().floor() as u32 + 1
                } else {
                    1
                };

                debug!(
                    "Loaded full texture: {:?} ({}x{}, mip_levels={})",
                    path, width, height, mip_levels
                );

                LoadResult::FullTextureLoaded {
                    id,
                    width,
                    height,
                    mip_levels,
                    pixels,
                }
            }
            Err(e) => {
                warn!("Failed to load texture {:?}: {}", path, e);
                LoadResult::Failed {
                    id,
                    path: path.clone(),
                    error: e.to_string(),
                }
            }
        }
    }

    /// Load a glTF model (stub - validates path exists, actual parsing in 171c).
    fn load_gltf_model(id: LoadId, path: &PathBuf) -> LoadResult {
        debug!("Loading glTF model: {:?}", path);

        if !path.exists() {
            warn!("glTF model not found: {:?}", path);
            return LoadResult::Failed {
                id,
                path: path.clone(),
                error: format!("File not found: {}", path.display()),
            };
        }

        match gltf::import(path) {
            Ok(_import) => {
                debug!("glTF model parsed: {:?}", path);
                LoadResult::GltfModelLoaded {
                    id,
                    path: path.clone(),
                }
            }
            Err(e) => {
                warn!("Failed to load glTF model {:?}: {}", path, e);
                LoadResult::Failed {
                    id,
                    path: path.clone(),
                    error: e.to_string(),
                }
            }
        }
    }

    /// Request an image thumbnail to be loaded.
    ///
    /// Returns the LoadId for tracking. Check `poll()` for the result.
    pub fn request_thumbnail(&mut self, path: PathBuf, max_size: u32) -> LoadId {
        // Check if already cached
        if self.thumbnail_cache.contains_key(&path) {
            debug!("Thumbnail already cached: {:?}", path);
            return LoadId(0); // No need to load
        }

        let id = LoadId(self.next_load_id);
        self.next_load_id += 1;

        self.pending_loads.insert(id, path.clone());

        let request = LoadRequest::ImageThumbnail { id, path, max_size };
        self.submit(request);

        id
    }

    /// Request a full-size texture to be loaded.
    ///
    /// Returns the LoadId for tracking. Check `poll()` for the result.
    pub fn request_full_texture(&mut self, path: PathBuf, generate_mipmaps: bool) -> LoadId {
        let id = LoadId(self.next_load_id);
        self.next_load_id += 1;

        self.pending_loads.insert(id, path.clone());

        let request = LoadRequest::FullTexture {
            id,
            path,
            generate_mipmaps,
        };
        self.submit(request);

        id
    }

    /// Request a glTF model to be loaded.
    ///
    /// Returns the LoadId for tracking. Check `poll()` for the result.
    pub fn request_gltf_model(&mut self, path: PathBuf) -> LoadId {
        let id = LoadId(self.next_load_id);
        self.next_load_id += 1;

        self.pending_loads.insert(id, path.clone());

        let request = LoadRequest::GltfModel { id, path };
        self.submit(request);

        id
    }

    /// Poll for completed load results (non-blocking).
    ///
    /// Call this each frame to process completed loads.
    /// Returns a list of completed load results.
    pub fn poll(&mut self) -> Vec<LoadResult> {
        let mut results = Vec::new();

        while let Ok(result) = self.result_receiver.try_recv() {
            if let Some(path) = self.pending_loads.remove(&result.id()) {
                if let LoadResult::ImageThumbnailLoaded { ref path, .. } = result {
                    self.thumbnail_cache
                        .insert(path.clone(), ThumbnailEntry { uploaded: false });
                }
                let _ = path;
            }
            results.push(result);
        }

        results
    }

    /// Get a cached thumbnail entry by path.
    pub fn get_thumbnail_mut(&mut self, path: &PathBuf) -> Option<&mut ThumbnailEntry> {
        self.thumbnail_cache.get_mut(path)
    }

    /// Check if a thumbnail is cached (may not be uploaded to GPU yet).
    pub fn has_thumbnail(&self, path: &PathBuf) -> bool {
        self.thumbnail_cache.contains_key(path)
    }

    /// Check if a load is pending for the given path.
    pub fn is_loading(&self, path: &PathBuf) -> bool {
        self.pending_loads.values().any(|p| p == path)
    }
}

impl Default for BackgroundLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadResult {
    /// Get the LoadId for this result.
    pub fn id(&self) -> LoadId {
        match self {
            LoadResult::ImageThumbnailLoaded { id, .. } => *id,
            LoadResult::FullTextureLoaded { id, .. } => *id,
            LoadResult::GltfModelLoaded { id, .. } => *id,
            LoadResult::Failed { id, .. } => *id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pending_tracking() {
        let mut loader = BackgroundLoader::new();
        let path = PathBuf::from("test.png");
        loader.request_thumbnail(path.clone(), 64);
        assert!(loader.is_loading(&path));
    }

    #[test]
    fn test_full_texture_request() {
        let mut loader = BackgroundLoader::new();
        let path = PathBuf::from("test_texture.png");
        let id = loader.request_full_texture(path.clone(), true);
        assert!(id.0 > 0);
        assert!(loader.is_loading(&path));
    }

    #[test]
    fn test_gltf_model_request() {
        let mut loader = BackgroundLoader::new();
        let path = PathBuf::from("test_model.glb");
        let id = loader.request_gltf_model(path.clone());
        assert!(id.0 > 0);
        assert!(loader.is_loading(&path));
    }

    #[test]
    fn test_load_ids_increment() {
        let mut loader = BackgroundLoader::new();
        let id1 = loader.request_thumbnail(PathBuf::from("a.png"), 64);
        let id2 = loader.request_full_texture(PathBuf::from("b.png"), false);
        let id3 = loader.request_gltf_model(PathBuf::from("c.glb"));
        assert!(id1.0 < id2.0);
        assert!(id2.0 < id3.0);
    }
}
