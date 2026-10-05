use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use katla_ui::{ScrollAreaState, input::DOUBLE_CLICK_TIME};

use super::types::{AssetAction, AssetEntry, AssetType, ThumbnailState};

/// Asset browser panel state.
pub struct AssetBrowserState {
    /// Show the built-in material palette beside project navigation.
    pub(crate) show_material_library: bool,
    pub(crate) project_folders: Vec<PathBuf>,
    pub(crate) selected_material: Option<usize>,
    /// Current directory being browsed
    pub current_path: PathBuf,
    /// Assets in current directory
    pub assets: Vec<AssetEntry>,
    /// Currently selected asset index (single selection)
    pub selected_index: Option<usize>,
    /// Multi-selected asset indices
    pub selected_indices: std::collections::HashSet<usize>,
    /// Scroll state for the content area
    pub scroll_state: ScrollAreaState,
    /// Panel height in pixels
    pub panel_height: f32,
    /// Last time directory was scanned
    pub last_scan: Option<Instant>,
    /// Search/filter text
    pub search_filter: String,
    /// Context menu is open
    pub context_menu_open: bool,
    /// Context menu for asset index (None = empty space context menu)
    pub context_menu_asset: Option<usize>,
    /// Navigation history (for back button)
    pub nav_history: Vec<PathBuf>,
    /// Current position in history
    pub nav_history_pos: usize,
    /// Pending actions to be processed
    pub pending_actions: Vec<AssetAction>,
    /// Rename mode active
    pub rename_mode: bool,
    /// Asset being renamed
    pub(crate) rename_asset: Option<usize>,
    /// New name buffer
    pub rename_buffer: String,
    /// Confirmation dialog is open
    pub confirm_dialog_open: bool,
    /// Message to show in confirmation dialog
    pub confirm_dialog_message: String,
    /// Pending action to confirm (stored until user responds)
    pub(crate) confirm_pending_action: Option<AssetAction>,
    /// Last computed column count from the rendering pass, used for keyboard navigation.
    pub(crate) last_col_count: usize,
    /// Asset index from the first click in a possible double-click sequence.
    last_click_index: Option<usize>,
    /// Time of the first click in a possible double-click sequence.
    last_click_time: Option<f64>,
}

impl AssetBrowserState {
    /// Create a new asset browser state starting at the Resources folder.
    pub fn new() -> Self {
        let current_path = PathBuf::from("resources");
        let nav_history = vec![current_path.clone()];

        Self {
            show_material_library: true,
            project_folders: Vec::new(),
            selected_material: None,
            current_path,
            assets: Vec::new(),
            selected_index: None,
            selected_indices: std::collections::HashSet::new(),
            scroll_state: ScrollAreaState::default(),
            panel_height: 150.0,
            last_scan: None,
            search_filter: String::new(),
            context_menu_open: false,
            context_menu_asset: None,
            nav_history,
            nav_history_pos: 0,
            pending_actions: Vec::new(),
            rename_mode: false,
            rename_asset: None,
            rename_buffer: String::new(),
            confirm_dialog_open: false,
            confirm_dialog_message: String::new(),
            confirm_pending_action: None,
            last_col_count: 8,
            last_click_index: None,
            last_click_time: None,
        }
    }

    /// Scan the current directory for assets.
    pub fn scan_directory(
        &mut self,
        thumbnail_texture_handles: &HashMap<PathBuf, katla_gfx::TextureHandle>,
    ) {
        self.project_folders = self
            .nav_history
            .first()
            .and_then(|root| fs::read_dir(root).ok())
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_dir()
                    && !path
                        .file_name()
                        .is_some_and(|name| name.to_string_lossy().starts_with('.'))
            })
            .collect();
        self.project_folders.sort_by_key(|path| {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy())
                .unwrap_or_default();
            (
                [
                    "models",
                    "prefabs",
                    "images",
                    "materials",
                    "meshes",
                    "scripts",
                    "fonts",
                ]
                .iter()
                .position(|item| *item == name)
                .unwrap_or(7),
                name.to_string(),
            )
        });
        let old_thumbnails: HashMap<PathBuf, ThumbnailState> = self
            .assets
            .iter()
            .map(|a| (a.path.clone(), a.thumbnail_state.clone()))
            .collect();

        let path_at = |index: Option<usize>| {
            index
                .and_then(|i| self.assets.get(i))
                .map(|a| a.path.clone())
        };
        let clicked_path = path_at(self.last_click_index);
        let selected_path = path_at(self.selected_index);
        let context_path = path_at(self.context_menu_asset);
        let rename_path = path_at(self.rename_asset);
        let selected_paths: Vec<_> = self
            .selected_indices
            .iter()
            .filter_map(|i| path_at(Some(*i)))
            .collect();
        self.assets.clear();

        if let Some(parent) = self.current_path.parent()
            && parent != self.current_path
            && !parent.as_os_str().is_empty()
        {
            self.assets.push(AssetEntry {
                name: "..".to_string(),
                path: parent.to_path_buf(),
                asset_type: AssetType::Folder,
                thumbnail_state: ThumbnailState::Pending,
            });
        }

        if let Ok(entries) = fs::read_dir(&self.current_path) {
            let mut dirs: Vec<AssetEntry> = Vec::new();
            let mut files: Vec<AssetEntry> = Vec::new();

            for entry in entries.flatten() {
                let path = entry.path();
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                if name.starts_with('.') {
                    continue;
                }

                if !self.search_filter.is_empty()
                    && !name
                        .to_lowercase()
                        .contains(&self.search_filter.to_lowercase())
                {
                    continue;
                }

                let asset_type = AssetType::from_path(&path);

                let thumbnail_state = if let Some(old_state) = old_thumbnails.get(&path) {
                    old_state.clone()
                } else if let Some(&texture_handle) = thumbnail_texture_handles.get(&path) {
                    ThumbnailState::Loaded { texture_handle }
                } else {
                    ThumbnailState::Pending
                };

                let entry = AssetEntry {
                    name,
                    path: path.clone(),
                    asset_type,
                    thumbnail_state,
                };

                if asset_type == AssetType::Folder {
                    dirs.push(entry);
                } else {
                    files.push(entry);
                }
            }

            dirs.sort_by_key(|a| a.name.to_lowercase());
            files.sort_by_key(|a| a.name.to_lowercase());

            self.assets.extend(dirs);
            self.assets.extend(files);
        }

        self.last_scan = Some(Instant::now());
        let index_of = |path: &PathBuf| self.assets.iter().position(|a| &a.path == path);
        self.last_click_index = clicked_path.as_ref().and_then(index_of);
        if self.last_click_index.is_none() {
            self.last_click_time = None;
        }
        self.selected_index = selected_path.as_ref().and_then(index_of);
        self.selected_indices = selected_paths.iter().filter_map(index_of).collect();
        self.context_menu_asset = context_path.as_ref().and_then(index_of);
        if context_path.is_some() && self.context_menu_asset.is_none() {
            self.context_menu_open = false;
        }
        self.rename_asset = rename_path.as_ref().and_then(index_of);
        if rename_path.is_some() && self.rename_asset.is_none() {
            self.rename_mode = false;
        }
    }

    fn reset_navigation_state(&mut self) {
        self.show_material_library = false;
        self.selected_index = None;
        self.selected_indices.clear();
        self.last_click_index = None;
        self.last_click_time = None;
        self.context_menu_asset = None;
        self.context_menu_open = false;
        self.rename_asset = None;
        self.rename_mode = false;
        self.scroll_state.scroll_offset = 0.0;
    }

    /// Force a rescan (e.g., when refresh button clicked).
    pub fn refresh(
        &mut self,
        thumbnail_texture_handles: &HashMap<PathBuf, katla_gfx::TextureHandle>,
    ) {
        self.last_scan = None;
        self.scan_directory(thumbnail_texture_handles);
    }

    /// Check if we should rescan the directory (every 500ms).
    pub fn needs_rescan(&self) -> bool {
        match self.last_scan {
            Some(last) => last.elapsed().as_millis() > 500,
            None => true,
        }
    }

    /// Navigate to a folder asset (with history).
    pub fn navigate_to(
        &mut self,
        path: &PathBuf,
        thumbnail_texture_handles: &HashMap<PathBuf, katla_gfx::TextureHandle>,
    ) {
        if path.is_dir() {
            self.show_material_library = false;
        }
        if path.is_dir() && path != &self.current_path {
            self.nav_history.truncate(self.nav_history_pos + 1);
            self.nav_history.push(path.clone());
            self.nav_history_pos = self.nav_history.len() - 1;

            self.current_path = path.clone();
            self.reset_navigation_state();
            self.scan_directory(thumbnail_texture_handles);
        }
    }

    /// Navigate to parent directory.
    pub fn navigate_up(
        &mut self,
        thumbnail_texture_handles: &HashMap<PathBuf, katla_gfx::TextureHandle>,
    ) {
        if let Some(parent) = self.current_path.parent() {
            let parent_path = parent.to_path_buf();
            if !parent_path.as_os_str().is_empty() && parent_path != self.current_path {
                self.navigate_to(&parent_path, thumbnail_texture_handles);
            }
        }
    }

    /// Whether the current directory has a parent above the assets root.
    pub fn can_go_up(&self) -> bool {
        self.current_path
            .parent()
            .map(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(false)
    }

    /// Navigate back in history.
    pub fn navigate_back(
        &mut self,
        thumbnail_texture_handles: &HashMap<PathBuf, katla_gfx::TextureHandle>,
    ) {
        if self.nav_history_pos > 0 {
            self.nav_history_pos -= 1;
            self.current_path = self.nav_history[self.nav_history_pos].clone();
            self.reset_navigation_state();
            self.scan_directory(thumbnail_texture_handles);
        }
    }

    /// Navigate forward in history.
    pub fn navigate_forward(
        &mut self,
        thumbnail_texture_handles: &HashMap<PathBuf, katla_gfx::TextureHandle>,
    ) {
        if self.nav_history_pos < self.nav_history.len() - 1 {
            self.nav_history_pos += 1;
            self.current_path = self.nav_history[self.nav_history_pos].clone();
            self.reset_navigation_state();
            self.scan_directory(thumbnail_texture_handles);
        }
    }

    /// Check if can navigate back.
    pub fn can_go_back(&self) -> bool {
        self.nav_history_pos > 0
    }

    /// Check if can navigate forward.
    pub fn can_go_forward(&self) -> bool {
        self.nav_history_pos < self.nav_history.len() - 1
    }

    /// Navigate to a specific path segment (for breadcrumbs).
    pub fn navigate_to_segment(
        &mut self,
        segment_index: usize,
        thumbnail_texture_handles: &HashMap<PathBuf, katla_gfx::TextureHandle>,
    ) {
        let segments: Vec<&std::ffi::OsStr> = self.current_path.iter().collect();
        if segment_index < segments.len() {
            let mut new_path = PathBuf::new();
            for (i, seg) in segments.iter().enumerate() {
                if i <= segment_index {
                    new_path.push(seg);
                }
            }
            if new_path.is_dir() && new_path != self.current_path {
                self.navigate_to(&new_path, thumbnail_texture_handles);
            }
        }
    }

    /// Get path segments for breadcrumb navigation.
    pub fn path_segments(&self) -> Vec<String> {
        self.current_path
            .iter()
            .map(|s| s.to_string_lossy().to_string())
            .collect()
    }

    /// Start rename mode for an asset.
    pub fn start_rename(&mut self, asset_index: usize) {
        if let Some(asset) = self.assets.get(asset_index) {
            self.rename_mode = true;
            self.rename_asset = Some(asset_index);
            self.rename_buffer = asset.name.clone();
            self.context_menu_open = false;
        }
    }

    /// Use the input event timestamp so delayed frames do not change click timing.
    pub(crate) fn register_click(&mut self, asset_index: usize, time: f64) -> bool {
        let is_double_click = time.is_finite()
            && self.last_click_index == Some(asset_index)
            && self.last_click_time.is_some_and(|last| {
                let elapsed = time - last;
                (0.0..=DOUBLE_CLICK_TIME).contains(&elapsed)
            });
        if is_double_click {
            self.last_click_index = None;
            self.last_click_time = None;
        } else {
            self.last_click_index = Some(asset_index);
            self.last_click_time = time.is_finite().then_some(time);
        }
        is_double_click
    }

    /// Take pending actions, clearing the list.
    pub fn take_actions(&mut self) -> Vec<AssetAction> {
        std::mem::take(&mut self.pending_actions)
    }
}

impl Default for AssetBrowserState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_double_click_requires_same_asset_within_time_window() {
        let mut state = AssetBrowserState::new();
        let start = 1.0;

        assert!(!state.register_click(1, start));
        assert!(!state.register_click(2, start + 0.1));
        assert!(state.register_click(2, start + 0.2));
    }

    #[test]
    fn test_click_after_timeout_starts_a_new_sequence() {
        let mut state = AssetBrowserState::new();
        let start = 1.0;

        assert!(!state.register_click(3, start));
        assert!(!state.register_click(3, start + 0.6));
        assert!(state.register_click(3, start + 0.7));
    }
    #[test]
    fn test_background_rescan_preserves_click_selection_and_scroll_by_path() {
        let root = std::env::temp_dir().join(format!("katla-asset-rescan-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("z.katprefab"), "").unwrap();
        let mut state = AssetBrowserState::new();
        state.current_path = root.clone();
        let handles = HashMap::new();
        state.scan_directory(&handles);
        let index = state
            .assets
            .iter()
            .position(|a| a.name == "z.katprefab")
            .unwrap();
        state.selected_index = Some(index);
        state.context_menu_asset = Some(index);
        state.context_menu_open = true;
        state.scroll_state.scroll_offset = 42.0;
        assert!(!state.register_click(index, 1.0));
        std::fs::write(root.join("a.katprefab"), "").unwrap();
        state.scan_directory(&handles);
        let fresh_index = state
            .assets
            .iter()
            .position(|a| a.name == "z.katprefab")
            .unwrap();
        assert_ne!(fresh_index, index);
        assert_eq!(state.selected_index, Some(fresh_index));
        assert_eq!(state.context_menu_asset, Some(fresh_index));
        assert_eq!(state.scroll_state.scroll_offset, 42.0);
        assert!(state.register_click(fresh_index, 1.2));
        std::fs::remove_file(root.join("z.katprefab")).unwrap();
        state.scan_directory(&handles);
        assert!(state.selected_index.is_none());
        assert!(!state.context_menu_open);
        std::fs::remove_dir_all(root).unwrap();
    }
}
