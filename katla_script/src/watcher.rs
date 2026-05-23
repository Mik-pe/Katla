use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use log::{debug, info};
use notify::Watcher;

/// watches a directory for `.luau` file changes and reports which scripts need reloading.
pub struct ScriptWatcher {
    _watcher: notify::RecommendedWatcher,
    rx: mpsc::Receiver<notify::Event>,
    scripts_dir: PathBuf,
}

impl ScriptWatcher {
    /// Start watching `scripts_dir` recursively for `.luau` changes.
    pub fn new(scripts_dir: impl Into<PathBuf>) -> Result<Self, notify::Error> {
        let scripts_dir = scripts_dir.into();
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
            scripts_dir,
        };

        w._watcher
            .watch(&w.scripts_dir, notify::RecursiveMode::Recursive)?;

        info!("Script watcher watching: {}", w.scripts_dir.display());
        Ok(w)
    }

    /// Poll for changed scripts, returning script names (relative paths without extension)
    /// that need hot-reloading. Call once per frame.
    pub fn poll_changes(&mut self) -> Vec<String> {
        let mut changed = HashSet::new();

        while let Ok(event) = self.rx.try_recv() {
            let is_relevant = matches!(
                event.kind,
                notify::EventKind::Create(_) | notify::EventKind::Modify(_)
            );

            if !is_relevant {
                continue;
            }

            for path in &event.paths {
                if path.extension().and_then(|e| e.to_str()) == Some("luau")
                    && let Some(name) = self.path_to_script_name(path)
                {
                    debug!("Detected script change: {name}");
                    changed.insert(name);
                }
            }
        }

        changed.into_iter().collect()
    }

    /// Convert an absolute file path to a script name (relative path without .luau extension).
    fn path_to_script_name(&self, path: &Path) -> Option<String> {
        let relative = path.strip_prefix(&self.scripts_dir).ok()?;
        let name = relative.to_str()?;
        Some(name.trim_end_matches(".luau").to_string())
    }
}
