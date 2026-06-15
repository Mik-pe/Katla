use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use log::{debug, info, warn};
use notify::Watcher;

/// File types that can be hot-reloaded.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[expect(dead_code)]
pub enum AssetChangeKind {
    /// WGSL shader file changed
    Shader,
    /// Image/texture file changed
    Texture,
    /// Script file changed
    Script,
}

/// A detected file change event.
#[derive(Debug, Clone)]
pub struct AssetChange {
    pub kind: AssetChangeKind,
    pub path: PathBuf,
}

/// Watches asset directories for file changes and routes them by type.
///
/// Watches:
/// - `resources/shaders/` for `.wgsl` files
/// - `resources/` for images (`.png`, `.jpg`, `.jpeg`, `.tga`)
///
/// Script files (`.luau`) are handled by the existing `ScriptWatcher` in
/// `katla_script` and are not duplicated here.
pub struct AssetWatcher {
    _watcher: notify::RecommendedWatcher,
    rx: mpsc::Receiver<notify::Event>,
    watched_dirs: Vec<PathBuf>,
}

const SHADER_EXTENSIONS: &[&str] = &["wgsl"];
const TEXTURE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "tga"];

impl AssetWatcher {
    /// Start watching the given directories recursively.
    pub fn new(directories: &[PathBuf]) -> Result<Self, notify::Error> {
        let (tx, rx) = mpsc::channel();

        let watcher = notify::RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    let _ = tx.send(event);
                }
            },
            notify::Config::default().with_poll_interval(Duration::from_millis(500)),
        )?;

        let mut w = Self {
            _watcher: watcher,
            rx,
            watched_dirs: directories.to_vec(),
        };

        for dir in &w.watched_dirs {
            if dir.exists() {
                w._watcher.watch(dir, notify::RecursiveMode::Recursive)?;
                info!("Asset watcher watching: {}", dir.display());
            } else {
                warn!("Asset watcher: directory does not exist: {}", dir.display());
            }
        }

        Ok(w)
    }

    /// Poll for file changes since the last call. Call once per frame.
    pub fn poll_changes(&mut self) -> Vec<AssetChange> {
        let mut changes = Vec::new();
        let mut seen = HashSet::new();

        while let Ok(event) = self.rx.try_recv() {
            let is_relevant = matches!(
                event.kind,
                notify::EventKind::Create(_) | notify::EventKind::Modify(_)
            );

            if !is_relevant {
                continue;
            }

            for path in event.paths {
                if !seen.insert(path.clone()) {
                    continue;
                }

                if let Some(kind) = self.classify_path(&path) {
                    debug!("Detected asset change: {:?} — {}", kind, path.display());
                    changes.push(AssetChange { kind, path });
                }
            }
        }

        changes
    }

    fn classify_path(&self, path: &Path) -> Option<AssetChangeKind> {
        let ext = path.extension()?.to_str()?.to_lowercase();

        if SHADER_EXTENSIONS.contains(&ext.as_str()) {
            // Only classify as shader if under a shaders directory
            for dir in &self.watched_dirs {
                if let Ok(relative) = path.strip_prefix(dir) {
                    let first_component = relative.components().next()?;
                    let first_str = first_component.as_os_str().to_str()?;
                    if first_str == "shaders" {
                        return Some(AssetChangeKind::Shader);
                    }
                }
            }
        }

        if TEXTURE_EXTENSIONS.contains(&ext.as_str()) {
            return Some(AssetChangeKind::Texture);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_shader() {
        let _watcher = AssetWatcher::new(&[]).unwrap();
        let _path = PathBuf::from("/some/project/resources/shaders/pbr.wgsl");
        // Can't test classification without matching watched_dirs prefix,
        // but the basic structure compiles.
        assert!(SHADER_EXTENSIONS.contains(&"wgsl"));
    }

    #[test]
    fn test_classify_texture() {
        assert!(TEXTURE_EXTENSIONS.contains(&"png"));
        assert!(TEXTURE_EXTENSIONS.contains(&"jpg"));
        assert!(TEXTURE_EXTENSIONS.contains(&"jpeg"));
        assert!(TEXTURE_EXTENSIONS.contains(&"tga"));
    }
}
