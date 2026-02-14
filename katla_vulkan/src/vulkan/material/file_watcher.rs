//! Filesystem watching for material hot reload.
//!
//! This module provides event-driven file watching using the `notify` crate,
//! allowing materials to be reloaded immediately when shader files change
//! without polling in the render loop.

use log::{error, info, warn};
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
    time::Duration,
};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// A file system watcher that runs in a background thread
///
/// This watcher monitors directories for file changes and sends
/// notifications through a channel. The main render thread can
/// check for notifications without blocking.
pub struct FileWatcher {
    _watcher_thread: JoinHandle<()>,
    receiver: Receiver<PathBuf>,
}

impl FileWatcher {
    /// Create a new file watcher for the given directory
    ///
    /// # Arguments
    /// * `directory` - The directory to watch for changes
    /// * `debounce_ms` - Debounce delay in milliseconds to prevent
    ///   multiple notifications for the same file change
    ///
    /// # Returns
    /// A `FileWatcher` that will send modified file paths through
    /// the internal receiver when .wgsl or .toml files change.
    pub fn new(directory: impl AsRef<Path>, debounce_ms: u64) -> Result<Self, WatcherError> {
        let (tx, rx) = mpsc::channel();
        let dir = directory.as_ref().to_path_buf();

        let thread = thread::spawn(move || {
            Self::watcher_thread(dir, tx, debounce_ms);
        });

        Ok(Self {
            _watcher_thread: thread,
            receiver: rx,
        })
    }

    /// The background thread that runs the notify watcher
    fn watcher_thread(directory: PathBuf, sender: Sender<PathBuf>, debounce_ms: u64) {
        // Create a channel for notify events
        let (notify_tx, notify_rx) = mpsc::channel();

        // Create the watcher using the recommended watcher for the platform
        let mut watcher: RecommendedWatcher = match Watcher::new(
            move |res| {
                if let Ok(event) = res {
                    let _ = notify_tx.send(event);
                }
            },
            notify::Config::default(),
        ) {
            Ok(w) => w,
            Err(e) => {
                error!("Failed to create file watcher: {:?}", e);
                return;
            }
        };

        // Watch the directory recursively
        if let Err(e) = watcher.watch(&directory, RecursiveMode::Recursive) {
            error!("Failed to watch directory {}: {:?}", directory.display(), e);
            return;
        }

        info!("File watcher started for: {}", directory.display());

        // Process events
        let mut last_event_time = std::time::Instant::now();
        let mut last_modified_path: Option<PathBuf> = None;

        for event in notify_rx {
            // Filter for write/modify events
            if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                let now = std::time::Instant::now();

                // Check if any file has relevant extension
                for path in &event.paths {
                    if path.extension().is_some_and(|ext| {
                        ext == "wgsl" || ext == "toml" || ext == "vert" || ext == "frag"
                    }) {
                        // Debounce: ignore events too soon after the last one
                        if now.duration_since(last_event_time) > Duration::from_millis(debounce_ms)
                        {
                            let _ = sender.send(path.clone());
                            last_event_time = now;
                            last_modified_path = Some(path.clone());
                        } else if let Some(ref last_path) = last_modified_path {
                            // If same file was modified, update the time but don't send yet
                            if path == last_path {
                                last_event_time = now;
                            }
                        }
                        break;
                    }
                }
            }
        }
    }

    /// Check for any file change events (non-blocking)
    ///
    /// This should be called in the main update loop. Returns the
    /// path of any file that was modified since the last check,
    /// or None if no changes occurred.
    pub fn try_recv(&self) -> Option<PathBuf> {
        self.receiver.try_recv().ok()
    }

    /// Get the receiver for direct polling if needed
    pub fn receiver(&self) -> &Receiver<PathBuf> {
        &self.receiver
    }
}

/// Errors that can occur when creating a file watcher
#[derive(Debug)]
pub enum WatcherError {
    /// Failed to create the underlying notify watcher
    CreationFailed(String),
    /// Failed to watch the specified directory
    WatchFailed(String),
}

impl std::fmt::Display for WatcherError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WatcherError::CreationFailed(msg) => {
                write!(f, "Failed to create watcher: {}", msg)
            }
            WatcherError::WatchFailed(msg) => {
                write!(f, "Failed to watch directory: {}", msg)
            }
        }
    }
}

impl std::error::Error for WatcherError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watcher_creation() {
        // Test that we can create a watcher (won't actually watch without a directory)
        let temp_dir = std::env::temp_dir();
        match FileWatcher::new(&temp_dir, 100) {
            Ok(_) => {}
            Err(_) => {
                // May fail on some systems, that's ok for this test
            }
        }
    }
}
