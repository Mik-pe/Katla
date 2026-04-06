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
                if let LoadResult::ImageThumbnailLoaded { ref path, .. } = result {
                    self.thumbnail_cache
                        .insert(path.clone(), ThumbnailEntry { uploaded: false });
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
}
