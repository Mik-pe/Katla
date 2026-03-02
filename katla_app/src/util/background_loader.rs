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
use std::thread::{self, JoinHandle};

use log::{debug, warn};

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

    /// Load full image (for textures, skyboxes, etc.).
    Image { id: LoadId, path: PathBuf },

    /// Load GLTF/GLB model (CPU parsing only, GPU upload on main thread).
    Model { id: LoadId, path: PathBuf },

    /// Load shader source (for hot reload, validation).
    ShaderSource { id: LoadId, path: PathBuf },
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

    /// Full image ready for GPU upload.
    ImageLoaded {
        id: LoadId,
        path: PathBuf,
        width: u32,
        height: u32,
        pixels: Vec<u8>, // RGBA8
    },

    /// Model parsed, ready for GPU buffer upload and entity creation.
    ModelLoaded {
        id: LoadId,
        path: PathBuf,
        /// PBR vertex data (position, normal, uv, tangent)
        vertices: Vec<katla_gfx::VertexPBR>,
        /// Index buffer
        indices: Vec<u32>,
    },

    /// Shader source loaded.
    ShaderSourceLoaded {
        id: LoadId,
        path: PathBuf,
        source: String,
    },

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
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// RGBA8 pixel data.
    pub pixels: Vec<u8>,
    /// Whether the thumbnail has been uploaded to GPU.
    pub uploaded: bool,
}

/// Background asset loader.
///
/// Spawns a worker thread that processes load requests and returns
/// results via a channel. Call `poll()` each frame to check for completed loads.
pub struct BackgroundLoader {
    /// Sender for load requests to worker thread.
    request_sender: Sender<LoadRequest>,
    /// Receiver for completed load results.
    result_receiver: Receiver<LoadResult>,
    /// Handle to worker thread (joined on drop).
    _thread_handle: JoinHandle<()>,
    /// Paths currently being loaded.
    pending_loads: HashMap<LoadId, PathBuf>,
    /// Cache of loaded thumbnails by path.
    thumbnail_cache: HashMap<PathBuf, ThumbnailEntry>,
    /// Next unique load ID.
    next_load_id: u64,
}

impl BackgroundLoader {
    /// Create a new background loader with a worker thread.
    pub fn new() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<LoadRequest>();
        let (result_tx, result_rx) = mpsc::channel::<LoadResult>();

        // Spawn worker thread
        let thread_handle = thread::spawn(move || {
            Self::worker_thread(request_rx, result_tx);
        });

        Self {
            request_sender: request_tx,
            result_receiver: result_rx,
            _thread_handle: thread_handle,
            pending_loads: HashMap::new(),
            thumbnail_cache: HashMap::new(),
            next_load_id: 1,
        }
    }

    /// Worker thread that processes load requests.
    fn worker_thread(request_rx: Receiver<LoadRequest>, result_tx: Sender<LoadResult>) {
        debug!("Background loader thread started");

        for request in request_rx {
            let result = match request {
                LoadRequest::ImageThumbnail { id, path, max_size } => {
                    Self::load_image_thumbnail(id, &path, max_size)
                }
                LoadRequest::Image { id, path } => Self::load_image(id, &path),
                LoadRequest::Model { id, path } => Self::load_model(id, &path),
                LoadRequest::ShaderSource { id, path } => Self::load_shader(id, &path),
            };

            if result_tx.send(result).is_err() {
                debug!("Background loader thread: result channel closed, exiting");
                break;
            }
        }

        debug!("Background loader thread exiting");
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

    /// Load a full image (no resizing).
    fn load_image(id: LoadId, path: &PathBuf) -> LoadResult {
        debug!("Loading image: {:?}", path);

        match image::open(path) {
            Ok(img) => {
                let rgba = img.to_rgba8();
                let (width, height) = rgba.dimensions();
                let pixels = rgba.into_raw();

                debug!("Loaded image: {:?} ({}x{})", path, width, height);

                LoadResult::ImageLoaded {
                    id,
                    path: path.clone(),
                    width,
                    height,
                    pixels,
                }
            }
            Err(e) => {
                warn!("Failed to load image {:?}: {}", path, e);
                LoadResult::Failed {
                    id,
                    path: path.clone(),
                    error: e.to_string(),
                }
            }
        }
    }

    /// Load a GLTF/GLB model (CPU-side parsing only).
    fn load_model(id: LoadId, path: &PathBuf) -> LoadResult {
        debug!("Loading model: {:?}", path);

        // Use the existing GLTF parser
        use crate::util::GLTFModel;

        match GLTFModel::new(path) {
            Ok(_model) => {
                // For now, just mark as successful. The actual model data
                // will be used when spawning via drag-drop (uses FileCache).
                // TODO: In the future, parse vertices/indices here for background processing.
                debug!("Model parsed successfully: {:?}", path);

                // Return a minimal result for now
                LoadResult::ModelLoaded {
                    id,
                    path: path.clone(),
                    vertices: Vec::new(), // TODO: Extract from model
                    indices: Vec::new(),  // TODO: Extract from model
                }
            }
            Err(e) => {
                warn!("Failed to load model {:?}: {}", path, e);
                LoadResult::Failed {
                    id,
                    path: path.clone(),
                    error: e.to_string(),
                }
            }
        }
    }

    /// Load shader source code.
    fn load_shader(id: LoadId, path: &PathBuf) -> LoadResult {
        debug!("Loading shader: {:?}", path);

        match std::fs::read_to_string(path) {
            Ok(source) => {
                debug!("Loaded shader: {:?} ({} bytes)", path, source.len());
                LoadResult::ShaderSourceLoaded {
                    id,
                    path: path.clone(),
                    source,
                }
            }
            Err(e) => {
                warn!("Failed to load shader {:?}: {}", path, e);
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

        if let Err(e) = self.request_sender.send(request) {
            warn!("Failed to send thumbnail request: {}", e);
        }

        id
    }

    /// Request a full image to be loaded.
    pub fn request_image(&mut self, path: PathBuf) -> LoadId {
        let id = LoadId(self.next_load_id);
        self.next_load_id += 1;

        self.pending_loads.insert(id, path.clone());

        let request = LoadRequest::Image { id, path };

        if let Err(e) = self.request_sender.send(request) {
            warn!("Failed to send image request: {}", e);
        }

        id
    }

    /// Request a model to be loaded.
    pub fn request_model(&mut self, path: PathBuf) -> LoadId {
        let id = LoadId(self.next_load_id);
        self.next_load_id += 1;

        self.pending_loads.insert(id, path.clone());

        let request = LoadRequest::Model { id, path };

        if let Err(e) = self.request_sender.send(request) {
            warn!("Failed to send model request: {}", e);
        }

        id
    }

    /// Request shader source to be loaded.
    pub fn request_shader(&mut self, path: PathBuf) -> LoadId {
        let id = LoadId(self.next_load_id);
        self.next_load_id += 1;

        self.pending_loads.insert(id, path.clone());

        let request = LoadRequest::ShaderSource { id, path };

        if let Err(e) = self.request_sender.send(request) {
            warn!("Failed to send shader request: {}", e);
        }

        id
    }

    /// Poll for completed load results (non-blocking).
    ///
    /// Call this each frame to process completed loads.
    /// Returns a list of completed load results.
    pub fn poll(&mut self) -> Vec<LoadResult> {
        let mut results = Vec::new();

        // Use try_recv to avoid blocking
        while let Ok(result) = self.result_receiver.try_recv() {
            // Remove from pending
            if let Some(path) = self.pending_loads.remove(&result.id()) {
                // Cache thumbnails
                if let LoadResult::ImageThumbnailLoaded {
                    ref path,
                    width,
                    height,
                    ref pixels,
                    ..
                } = result
                {
                    self.thumbnail_cache.insert(
                        path.clone(),
                        ThumbnailEntry {
                            width,
                            height,
                            pixels: pixels.clone(),
                            uploaded: false,
                        },
                    );
                }
                // Don't remove from pending_loads here - do it for all results
                let _ = path; // Path was used above
            }
            self.pending_loads.remove(&result.id());
            results.push(result);
        }

        results
    }

    /// Get a cached thumbnail entry by path.
    pub fn get_thumbnail(&self, path: &PathBuf) -> Option<&ThumbnailEntry> {
        self.thumbnail_cache.get(path)
    }

    /// Get a mutable cached thumbnail entry by path.
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

    /// Get the number of pending loads.
    pub fn pending_count(&self) -> usize {
        self.pending_loads.len()
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
            LoadResult::ImageLoaded { id, .. } => *id,
            LoadResult::ModelLoaded { id, .. } => *id,
            LoadResult::ShaderSourceLoaded { id, .. } => *id,
            LoadResult::Failed { id, .. } => *id,
        }
    }

    /// Get the path for this result.
    pub fn path(&self) -> &PathBuf {
        match self {
            LoadResult::ImageThumbnailLoaded { path, .. } => path,
            LoadResult::ImageLoaded { path, .. } => path,
            LoadResult::ModelLoaded { path, .. } => path,
            LoadResult::ShaderSourceLoaded { path, .. } => path,
            LoadResult::Failed { path, .. } => path,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_id_uniqueness() {
        let mut loader = BackgroundLoader::new();
        let id1 = loader.request_image(PathBuf::from("test1.png"));
        let id2 = loader.request_image(PathBuf::from("test2.png"));
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_pending_tracking() {
        let mut loader = BackgroundLoader::new();
        let path = PathBuf::from("test.png");
        loader.request_image(path.clone());
        assert!(loader.is_loading(&path));
        assert_eq!(loader.pending_count(), 1);
    }
}
