use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use katla_ui::ScrollAreaState;

use super::types::{AssetAction, AssetEntry, AssetType, ThumbnailState};

/// Asset browser panel state.
pub struct AssetBrowserState {
    /// Current directory being browsed
    pub current_path: PathBuf,
    /// Assets in current directory
    pub assets: Vec<AssetEntry>,
    /// Currently selected asset index (single selection)
    pub selected_index: Option<usize>,
    /// Multi-selected asset indices
    pub selected_indices: std::collections::HashSet<usize>,
    /// Selection rectangle start position (for marquee selection)
    pub(crate) selection_rect_start: Option<katla_math::Vec2>,
    /// Selection rectangle current position
    pub(crate) selection_rect_current: Option<katla_math::Vec2>,
    /// Whether marquee selection is active
    pub(crate) is_marquee_selecting: bool,
    /// Scroll state for the content area
    pub scroll_state: ScrollAreaState,
    /// Panel height in pixels (when not collapsed)
    pub panel_height: f32,
    /// Whether panel is collapsed
    pub collapsed: bool,
    /// Last time directory was scanned
    pub last_scan: Option<Instant>,
    /// Index of last clicked item (for double-click same-item check)
    pub(crate) last_click_index: Option<usize>,
    /// Search/filter text
    pub search_filter: String,
    /// Whether search input is focused
    pub search_focused: bool,
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
    /// Drag state - asset being dragged
    pub drag_asset: Option<usize>,
    /// Drag state - start position
    pub(crate) drag_start_pos: Option<katla_math::Vec2>,
    /// Drag state - is actively dragging (moved past threshold)
    pub is_dragging: bool,
    /// Drag threshold in pixels
    pub(crate) drag_threshold: f32,
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
}

impl AssetBrowserState {
    /// Create a new asset browser state starting at the Resources folder.
    pub fn new() -> Self {
        let current_path = PathBuf::from("resources");
        let nav_history = vec![current_path.clone()];

        // Initial scan will happen in build_asset_browser when needs_rescan() returns true
        Self {
            current_path,
            assets: Vec::new(),
            selected_index: None,
            selected_indices: std::collections::HashSet::new(),
            selection_rect_start: None,
            selection_rect_current: None,
            is_marquee_selecting: false,
            scroll_state: ScrollAreaState::default(),
            panel_height: 150.0,
            collapsed: false,
            last_scan: None,
            last_click_index: None,
            search_filter: String::new(),
            search_focused: false,
            context_menu_open: false,
            context_menu_asset: None,
            nav_history,
            nav_history_pos: 0,
            pending_actions: Vec::new(),
            drag_asset: None,
            drag_start_pos: None,
            is_dragging: false,
            drag_threshold: 5.0,
            rename_mode: false,
            rename_asset: None,
            rename_buffer: String::new(),
            confirm_dialog_open: false,
            confirm_dialog_message: String::new(),
            confirm_pending_action: None,
            last_col_count: 8,
        }
    }

    /// Scan the current directory for assets.
    pub fn scan_directory(
        &mut self,
        thumbnail_texture_handles: &HashMap<PathBuf, katla_gfx::TextureHandle>,
    ) {
        // Preserve thumbnail states before clearing
        let old_thumbnails: HashMap<PathBuf, ThumbnailState> = self
            .assets
            .iter()
            .map(|a| (a.path.clone(), a.thumbnail_state.clone()))
            .collect();

        self.assets.clear();

        // Add parent directory entry if not at root (parent must be different from current)
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

        // Read directory entries
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

            // Sort directories and files alphabetically
            dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

            // Directories first, then files
            self.assets.extend(dirs);
            self.assets.extend(files);
        }

        self.last_scan = Some(Instant::now());
        // Don't clear selection on rescan - only clear on navigation
        // Reset scroll on rescan
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
        if path.is_dir() && path != &self.current_path {
            // Clear forward history
            self.nav_history.truncate(self.nav_history_pos + 1);
            // Add to history
            self.nav_history.push(path.clone());
            self.nav_history_pos = self.nav_history.len() - 1;

            self.current_path = path.clone();
            // Clear selection when navigating to a new directory
            self.selected_index = None;
            self.selected_indices.clear();
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
            if parent_path != self.current_path {
                self.navigate_to(&parent_path, thumbnail_texture_handles);
            }
        }
    }

    /// Navigate back in history.
    pub fn navigate_back(
        &mut self,
        thumbnail_texture_handles: &HashMap<PathBuf, katla_gfx::TextureHandle>,
    ) {
        if self.nav_history_pos > 0 {
            self.nav_history_pos -= 1;
            self.current_path = self.nav_history[self.nav_history_pos].clone();
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

    /// Start dragging an asset.
    pub fn start_drag(&mut self, asset_index: usize, pos: katla_math::Vec2) {
        self.drag_asset = Some(asset_index);
        self.drag_start_pos = Some(pos);
        self.is_dragging = false;
    }

    /// Update drag position and check threshold.
    pub fn update_drag(&mut self, current_pos: katla_math::Vec2) {
        if let Some(start_pos) = self.drag_start_pos {
            let dist = (current_pos - start_pos).length();
            if dist > self.drag_threshold {
                self.is_dragging = true;
            }
        }
    }

    /// End drag operation.
    pub fn end_drag(&mut self) -> Option<(usize, katla_math::Vec2)> {
        let result = if self.is_dragging {
            self.drag_asset.zip(self.drag_start_pos)
        } else {
            None
        };
        self.drag_asset = None;
        self.drag_start_pos = None;
        self.is_dragging = false;
        result
    }

    /// Cancel drag operation.
    pub fn cancel_drag(&mut self) {
        self.drag_asset = None;
        self.drag_start_pos = None;
        self.is_dragging = false;
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

    /// Cancel rename mode.
    pub fn cancel_rename(&mut self) {
        self.rename_mode = false;
        self.rename_asset = None;
        self.rename_buffer.clear();
    }

    /// Take pending actions, clearing the list.
    pub fn take_actions(&mut self) -> Vec<AssetAction> {
        std::mem::take(&mut self.pending_actions)
    }

    /// Handle keyboard navigation.
    pub fn handle_keyboard(
        &mut self,
        key: katla_ui::input::KeyCode,
        thumbnail_texture_handles: &HashMap<PathBuf, katla_gfx::TextureHandle>,
    ) -> Option<AssetAction> {
        if self.search_focused || self.assets.is_empty() {
            return None;
        }

        let col_count = self.last_col_count.max(1);

        match key {
            katla_ui::input::KeyCode::ArrowUp => {
                if let Some(idx) = self.selected_index {
                    if idx >= col_count {
                        self.selected_index = Some(idx - col_count);
                        self.scroll_to_selected();
                    }
                } else {
                    self.selected_index = Some(0);
                }
            }
            katla_ui::input::KeyCode::ArrowDown => {
                if let Some(idx) = self.selected_index {
                    if idx + col_count < self.assets.len() {
                        self.selected_index = Some(idx + col_count);
                        self.scroll_to_selected();
                    }
                } else {
                    self.selected_index = Some(0);
                }
            }
            katla_ui::input::KeyCode::ArrowLeft => {
                if let Some(idx) = self.selected_index {
                    if idx > 0 {
                        self.selected_index = Some(idx - 1);
                        self.scroll_to_selected();
                    }
                } else {
                    self.selected_index = Some(0);
                }
            }
            katla_ui::input::KeyCode::ArrowRight => {
                if let Some(idx) = self.selected_index {
                    if idx + 1 < self.assets.len() {
                        self.selected_index = Some(idx + 1);
                        self.scroll_to_selected();
                    }
                } else {
                    self.selected_index = Some(0);
                }
            }
            katla_ui::input::KeyCode::Enter => {
                if let Some(idx) = self.selected_index {
                    let asset_type = self.assets[idx].asset_type;
                    let is_parent = self.assets[idx].name == "..";
                    let path = self.assets[idx].path.clone();

                    if asset_type == AssetType::Folder {
                        if is_parent {
                            self.navigate_up(thumbnail_texture_handles);
                        } else {
                            self.navigate_to(&path, thumbnail_texture_handles);
                        }
                    } else {
                        return Some(AssetAction::Open(path));
                    }
                }
            }
            katla_ui::input::KeyCode::Backspace => {
                self.navigate_up(thumbnail_texture_handles);
            }
            _ => {}
        }

        None
    }

    /// Scroll to ensure selected item is visible.
    fn scroll_to_selected(&mut self) {
        let item_size = 64.0;
        let row_height = item_size + 24.0;
        let col_count = self.last_col_count.max(1);

        if let Some(idx) = self.selected_index {
            let row = idx / col_count;
            let item_y = row as f32 * row_height;

            // Scroll to make item visible (with some padding)
            let visible_top = self.scroll_state.scroll_offset;
            let visible_bottom = self.scroll_state.scroll_offset + 100.0; // Approximate visible height

            if item_y < visible_top {
                self.scroll_state.scroll_offset = item_y;
            } else if item_y + row_height > visible_bottom {
                self.scroll_state.scroll_offset = item_y + row_height - 100.0;
            }
        }
    }
}

impl Default for AssetBrowserState {
    fn default() -> Self {
        Self::new()
    }
}
