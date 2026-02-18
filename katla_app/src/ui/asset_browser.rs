//! Asset Browser panel for browsing Resources folder.
//!
//! Provides a scrollable view of assets with:
//! - Grid layout with type icons
//! - Folder navigation
//! - PNG image thumbnail support (loaded in background)
//! - Auto-refresh on file changes

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use katla_math::{Color, Rect2D, Vec2};
use katla_ui::{ForkAwesome, TextureId, UiContext};

use super::editor_ui::FocusedPanel;
use super::theme::Theme;

/// Asset type classification for icons and filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetType {
    /// 3D model files (.glb, .gltf)
    Model,
    /// Material definitions (.toml)
    Material,
    /// Shader source (.wgsl)
    Shader,
    /// Image files (.png, .jpg)
    Image,
    /// Font files (.ttf, .otf)
    Font,
    /// Directory
    Folder,
    /// Unknown/other file type
    Unknown,
}

/// Thumbnail loading state for an asset.
#[derive(Debug, Clone)]
pub enum ThumbnailState {
    /// Not yet requested to load.
    NotRequested,
    /// Currently loading in background thread.
    Loading,
    /// Loaded and uploaded to GPU.
    Loaded { texture_id: TextureId },
    /// Failed to load.
    Failed,
}

impl Default for ThumbnailState {
    fn default() -> Self {
        Self::NotRequested
    }
}

impl AssetType {
    /// Determine asset type from file extension.
    pub fn from_path(path: &std::path::Path) -> Self {
        if path.is_dir() {
            return Self::Folder;
        }

        match path.extension().and_then(|e| e.to_str()) {
            Some("glb") | Some("gltf") => Self::Model,
            Some("toml") => Self::Material,
            Some("wgsl") => Self::Shader,
            Some("png") | Some("jpg") | Some("jpeg") => Self::Image,
            Some("ttf") | Some("otf") => Self::Font,
            _ => Self::Unknown,
        }
    }

    /// Get the ForkAwesome icon for this asset type.
    pub fn icon(&self) -> char {
        match self {
            Self::Model => ForkAwesome::CUBE,
            Self::Material => ForkAwesome::PAINT_BRUSH,
            Self::Shader => ForkAwesome::FILE_CODE,
            Self::Image => ForkAwesome::IMAGE,
            Self::Font => ForkAwesome::FILE_TEXT, // No font icon, use file text
            Self::Folder => ForkAwesome::FOLDER,
            Self::Unknown => ForkAwesome::FILE,
        }
    }

    /// Get icon color for this asset type.
    pub fn color(&self, theme: &Theme) -> Color {
        match self {
            Self::Model => theme.entity_mesh,
            Self::Material => theme.text_accent,
            Self::Shader => theme.info,
            Self::Image => theme.warning,
            Self::Font => theme.text_secondary,
            Self::Folder => theme.highlight,
            Self::Unknown => theme.text_muted,
        }
    }
}

/// Single asset entry in the browser.
#[derive(Debug, Clone)]
pub struct AssetEntry {
    /// Display name (filename without path)
    pub name: String,
    /// Full filesystem path
    pub path: PathBuf,
    /// Asset type classification
    pub asset_type: AssetType,
    /// Thumbnail loading state (for images)
    pub thumbnail_state: ThumbnailState,
}

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
    selection_rect_start: Option<Vec2>,
    /// Selection rectangle current position
    selection_rect_current: Option<Vec2>,
    /// Whether marquee selection is active
    is_marquee_selecting: bool,
    /// Scroll offset in pixels
    pub scroll_offset: f32,
    /// Panel height in pixels (when not collapsed)
    pub panel_height: f32,
    /// Whether panel is collapsed
    pub collapsed: bool,
    /// Last time directory was scanned
    last_scan: Option<Instant>,
    /// Time of last click for double-click detection
    last_click_time: Option<Instant>,
    /// Index of last clicked item
    last_click_index: Option<usize>,
    /// Search/filter text
    pub search_filter: String,
    /// Whether search input is focused
    pub search_focused: bool,
    /// Context menu is open
    pub context_menu_open: bool,
    /// Context menu position
    context_menu_pos: Vec2,
    /// Context menu for asset index (None = empty space context menu)
    context_menu_asset: Option<usize>,
    /// Navigation history (for back button)
    pub nav_history: Vec<PathBuf>,
    /// Current position in history
    pub nav_history_pos: usize,
    /// Pending actions to be processed
    pub pending_actions: Vec<AssetAction>,
    /// Drag state - asset being dragged
    pub drag_asset: Option<usize>,
    /// Drag state - start position
    drag_start_pos: Option<Vec2>,
    /// Drag state - is actively dragging (moved past threshold)
    pub is_dragging: bool,
    /// Drag threshold in pixels
    drag_threshold: f32,
    /// Rename mode active
    pub rename_mode: bool,
    /// Asset being renamed
    rename_asset: Option<usize>,
    /// New name buffer
    pub rename_buffer: String,
    /// Confirmation dialog is open
    pub confirm_dialog_open: bool,
    /// Message to show in confirmation dialog
    confirm_dialog_message: String,
    /// Pending action to confirm (stored until user responds)
    confirm_pending_action: Option<AssetAction>,
}

/// Action from the asset browser context menu.
#[derive(Debug, Clone)]
pub enum AssetAction {
    /// Open asset (double-click equivalent)
    Open(PathBuf),
    /// Copy path to clipboard
    CopyPath(PathBuf),
    /// Show in Explorer/Finder
    ShowInExplorer(PathBuf),
    /// Delete asset
    Delete(PathBuf),
    /// Rename asset (old_path, new_path)
    Rename { old_path: PathBuf, new_path: PathBuf },
    /// Create new folder
    CreateFolder(PathBuf),
    /// Drag asset to viewport (spawn entity)
    DragToViewport {
        path: PathBuf,
        asset_type: AssetType,
        screen_pos: Vec2,
    },
    /// Move asset to folder
    MoveToFolder {
        asset_path: PathBuf,
        folder_path: PathBuf,
    },
}

impl AssetBrowserState {
    /// Create a new asset browser state starting at the Resources folder.
    pub fn new() -> Self {
        let current_path = PathBuf::from("resources");
        let nav_history = vec![current_path.clone()];

        let mut state = Self {
            current_path,
            assets: Vec::new(),
            selected_index: None,
            selected_indices: std::collections::HashSet::new(),
            selection_rect_start: None,
            selection_rect_current: None,
            is_marquee_selecting: false,
            scroll_offset: 0.0,
            panel_height: 150.0,
            collapsed: false,
            last_scan: None,
            last_click_time: None,
            last_click_index: None,
            search_filter: String::new(),
            search_focused: false,
            context_menu_open: false,
            context_menu_pos: Vec2::new(0.0, 0.0),
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
        };

        // Initial scan will happen in build_asset_browser when needs_rescan() returns true
        state
    }

    /// Scan the current directory for assets.
    pub fn scan_directory(&mut self, thumbnail_texture_ids: &HashMap<PathBuf, TextureId>) {
        // Preserve thumbnail states before clearing
        let old_thumbnails: HashMap<PathBuf, ThumbnailState> = self
            .assets
            .iter()
            .map(|a| (a.path.clone(), a.thumbnail_state.clone()))
            .collect();

        self.assets.clear();

        // Add parent directory entry if not at root
        if self.current_path.parent().is_some() {
            self.assets.push(AssetEntry {
                name: "..".to_string(),
                path: self.current_path.parent().unwrap().to_path_buf(),
                asset_type: AssetType::Folder,
                thumbnail_state: ThumbnailState::NotRequested,
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

                // Skip hidden files
                if name.starts_with('.') {
                    continue;
                }

                // Apply search filter
                if !self.search_filter.is_empty() {
                    if !name.to_lowercase().contains(&self.search_filter.to_lowercase()) {
                        continue;
                    }
                }

                let asset_type = AssetType::from_path(&path);

                // Determine thumbnail state:
                // 1. Check if we already have this asset in current view (old_thumbnails)
                // 2. Check if we have a cached GPU texture for this path (thumbnail_texture_ids)
                let thumbnail_state = if let Some(old_state) = old_thumbnails.get(&path) {
                    old_state.clone()
                } else if let Some(&texture_id) = thumbnail_texture_ids.get(&path) {
                    // Already uploaded to GPU on a previous visit
                    ThumbnailState::Loaded { texture_id }
                } else {
                    ThumbnailState::NotRequested
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
        // Preserve scroll_offset only if directory hasn't changed
        self.scroll_offset = 0.0;
    }

    /// Force a rescan (e.g., when refresh button clicked).
    pub fn refresh(&mut self, thumbnail_texture_ids: &HashMap<PathBuf, TextureId>) {
        self.last_scan = None;
        self.scan_directory(thumbnail_texture_ids);
    }

    /// Check if we should rescan the directory (every 500ms).
    pub fn needs_rescan(&self) -> bool {
        match self.last_scan {
            Some(last) => last.elapsed().as_millis() > 500,
            None => true,
        }
    }

    /// Navigate to a folder asset (with history).
    pub fn navigate_to(&mut self, path: &PathBuf, thumbnail_texture_ids: &HashMap<PathBuf, TextureId>) {
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
            self.scan_directory(thumbnail_texture_ids);
        }
    }

    /// Navigate to parent directory.
    pub fn navigate_up(&mut self, thumbnail_texture_ids: &HashMap<PathBuf, TextureId>) {
        if let Some(parent) = self.current_path.parent() {
            let parent_path = parent.to_path_buf();
            if parent_path != self.current_path {
                self.navigate_to(&parent_path, thumbnail_texture_ids);
            }
        }
    }

    /// Navigate back in history.
    pub fn navigate_back(&mut self, thumbnail_texture_ids: &HashMap<PathBuf, TextureId>) {
        if self.nav_history_pos > 0 {
            self.nav_history_pos -= 1;
            self.current_path = self.nav_history[self.nav_history_pos].clone();
            self.scan_directory(thumbnail_texture_ids);
        }
    }

    /// Navigate forward in history.
    pub fn navigate_forward(&mut self, thumbnail_texture_ids: &HashMap<PathBuf, TextureId>) {
        if self.nav_history_pos < self.nav_history.len() - 1 {
            self.nav_history_pos += 1;
            self.current_path = self.nav_history[self.nav_history_pos].clone();
            self.scan_directory(thumbnail_texture_ids);
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
    pub fn navigate_to_segment(&mut self, segment_index: usize, thumbnail_texture_ids: &HashMap<PathBuf, TextureId>) {
        let segments: Vec<&std::ffi::OsStr> = self.current_path.iter().collect();
        if segment_index < segments.len() {
            let mut new_path = PathBuf::new();
            for (i, seg) in segments.iter().enumerate() {
                if i <= segment_index {
                    new_path.push(seg);
                }
            }
            if new_path.is_dir() && new_path != self.current_path {
                self.navigate_to(&new_path, thumbnail_texture_ids);
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

    /// Check if a click is a double-click on the same item.
    pub fn is_double_click(&mut self, index: usize) -> bool {
        let now = Instant::now();

        let is_double = self.last_click_index == Some(index)
            && self
                .last_click_time
                .map(|t| t.elapsed().as_millis() < 500)
                .unwrap_or(false);

        self.last_click_time = Some(now);
        self.last_click_index = Some(index);

        is_double
    }

    /// Get max scroll offset based on content.
    pub fn max_scroll(&self, content_height: f32, visible_height: f32) -> f32 {
        (content_height - visible_height).max(0.0)
    }

    /// Open context menu for an asset.
    pub fn open_context_menu(&mut self, asset_index: usize, pos: Vec2) {
        self.context_menu_open = true;
        self.context_menu_pos = pos;
        self.context_menu_asset = Some(asset_index);
    }

    /// Close context menu.
    pub fn close_context_menu(&mut self) {
        self.context_menu_open = false;
        self.context_menu_asset = None;
    }

    /// Start dragging an asset.
    pub fn start_drag(&mut self, asset_index: usize, pos: Vec2) {
        self.drag_asset = Some(asset_index);
        self.drag_start_pos = Some(pos);
        self.is_dragging = false;
    }

    /// Update drag position and check threshold.
    pub fn update_drag(&mut self, current_pos: Vec2) {
        if let Some(start_pos) = self.drag_start_pos {
            let dist = (current_pos - start_pos).length();
            if dist > self.drag_threshold {
                self.is_dragging = true;
            }
        }
    }

    /// End drag operation.
    pub fn end_drag(&mut self) -> Option<(usize, Vec2)> {
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

    /// Commit rename.
    pub fn commit_rename(&mut self) -> Option<(PathBuf, String)> {
        if let Some(idx) = self.rename_asset {
            if let Some(asset) = self.assets.get(idx) {
                let old_path = asset.path.clone();
                let new_name = self.rename_buffer.clone();
                self.cancel_rename();
                return Some((old_path, new_name));
            }
        }
        self.cancel_rename();
        None
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
    pub fn handle_keyboard(&mut self, key: katla_ui::input::KeyCode, thumbnail_texture_ids: &HashMap<PathBuf, TextureId>) -> Option<AssetAction> {
        if self.search_focused || self.assets.is_empty() {
            return None;
        }

        // Get grid dimensions (approximate - will be recalculated in draw)
        let col_count = 8; // Approximate

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
                            self.navigate_up(thumbnail_texture_ids);
                        } else {
                            self.navigate_to(&path, thumbnail_texture_ids);
                        }
                    } else {
                        return Some(AssetAction::Open(path));
                    }
                }
            }
            katla_ui::input::KeyCode::Backspace => {
                self.navigate_up(thumbnail_texture_ids);
            }
            _ => {}
        }

        None
    }

    /// Scroll to ensure selected item is visible.
    fn scroll_to_selected(&mut self) {
        // Approximate calculation - actual values depend on draw-time grid
        let item_size = 64.0;
        let row_height = item_size + 24.0;
        let col_count = 8;

        if let Some(idx) = self.selected_index {
            let row = idx / col_count;
            let item_y = row as f32 * row_height;

            // Scroll to make item visible (with some padding)
            let visible_top = self.scroll_offset;
            let visible_bottom = self.scroll_offset + 100.0; // Approximate visible height

            if item_y < visible_top {
                self.scroll_offset = item_y;
            } else if item_y + row_height > visible_bottom {
                self.scroll_offset = item_y + row_height - 100.0;
            }
        }
    }
}

impl Default for AssetBrowserState {
    fn default() -> Self {
        Self::new()
    }
}

/// Build the asset browser panel.
pub fn build_asset_browser(
    state: &mut AssetBrowserState,
    ui: &mut UiContext,
    theme: &Theme,
    bounds: Rect2D,
    focused_panel: &mut FocusedPanel,
    loader: &mut crate::util::BackgroundLoader,
    thumbnail_texture_ids: &HashMap<PathBuf, TextureId>,
) {
    let is_focused = *focused_panel == FocusedPanel::AssetBrowser;
    // Auto-rescan if needed
    if state.needs_rescan() {
        state.scan_directory(thumbnail_texture_ids);
    }

    // Panel background
    ui.draw_rect(bounds, theme.panel_bg);

    // Focus this panel when clicked
    if ui.is_hovered(bounds) && ui.input.mouse_clicked(katla_ui::input::mouse_button::LEFT) {
        *focused_panel = FocusedPanel::AssetBrowser;
    }

    // Draw focus indicator border if focused
    if is_focused {
        ui.draw_rect_border(bounds, theme.panel_bg, theme.highlight, 2.0);
    }

    // Header with breadcrumbs and controls
    let header_height = 24.0;
    let toolbar_height = 28.0;
    let header_bounds =
        Rect2D::from_origin_size(bounds.min, Vec2::new(bounds.width(), header_height));
    ui.draw_rect(header_bounds, theme.panel_header);

    // === HEADER: Title + Collapse Toggle ===
    let padding = 8.0;

    // Collapse toggle button (left side)
    let toggle_size = 20.0;
    let toggle_bounds = Rect2D::from_origin_size(
        Vec2::new(bounds.min.x() + 4.0, bounds.min.y() + 2.0),
        Vec2::new(toggle_size, toggle_size),
    );
    let toggle_icon = if state.collapsed {
        ForkAwesome::CHEVRON_UP
    } else {
        ForkAwesome::CHEVRON_DOWN
    };

    let toggle_hovered = ui.is_hovered(toggle_bounds);
    if toggle_hovered && ui.input.mouse_clicked(katla_ui::input::mouse_button::LEFT) {
        state.collapsed = !state.collapsed;
    }

    let toggle_color = if toggle_hovered {
        theme.text_primary
    } else {
        theme.text_secondary
    };
    ui.draw_icon_aligned(
        toggle_icon,
        Vec2::new(toggle_bounds.min.x(), toggle_bounds.center().y() - 8.0),
        14.0,
        toggle_color,
        katla_ui::FontId::DEFAULT,
    );

    // Title
    let title_pos = Vec2::new(
        bounds.min.x() + toggle_size + 8.0,
        header_bounds.center().y() - ui.scaled_font_size(katla_ui::FontSize::Medium) * 0.5,
    );
    ui.draw_text(
        "Asset Browser",
        title_pos,
        theme.text_primary,
        ui.scaled_font_size(katla_ui::FontSize::Medium),
    );

    // Asset count
    let count_text = format!("({})", state.assets.len());
    let count_size = ui.measure_text(&count_text, ui.scaled_font_size(katla_ui::FontSize::Small));
    let title_width = ui.measure_text("Asset Browser", ui.scaled_font_size(katla_ui::FontSize::Medium)).x();
    let count_pos = Vec2::new(
        title_pos.x() + title_width + 6.0,
        header_bounds.center().y() - count_size.y() * 0.5,
    );
    ui.draw_text(
        &count_text,
        count_pos,
        theme.text_muted,
        ui.scaled_font_size(katla_ui::FontSize::Small),
    );

    // Top border
    ui.draw_line(
        Vec2::new(bounds.min.x(), bounds.min.y()),
        Vec2::new(bounds.max.x(), bounds.min.y()),
        theme.panel_border,
        1.0,
    );

    // If collapsed, don't render content
    if state.collapsed {
        return;
    }

    // === TOOLBAR: Breadcrumbs + Search + Refresh ===
    let toolbar_top = bounds.min.y() + header_height;
    let toolbar_bounds = Rect2D::from_origin_size(
        Vec2::new(bounds.min.x(), toolbar_top),
        Vec2::new(bounds.width(), toolbar_height),
    );
    ui.draw_rect(toolbar_bounds, theme.background_dark);

    // Breadcrumb navigation
    let mut breadcrumb_x = bounds.min.x() + padding;
    let breadcrumb_y = toolbar_bounds.center().y() - ui.scaled_font_size(katla_ui::FontSize::Small) * 0.5;
    let breadcrumb_height = ui.scaled_font_size(katla_ui::FontSize::Small) + 4.0;
    let segments = state.path_segments();

    // Track breadcrumb clicks
    let mut clicked_segment: Option<usize> = None;

    for (i, segment) in segments.iter().enumerate() {
        // Draw separator (except for first)
        if i > 0 {
            let sep_text = " / ";
            let sep_size = ui.measure_text(sep_text, ui.scaled_font_size(katla_ui::FontSize::Small));
            ui.draw_text(
                sep_text,
                Vec2::new(breadcrumb_x, breadcrumb_y),
                theme.text_muted,
                ui.scaled_font_size(katla_ui::FontSize::Small),
            );
            breadcrumb_x += sep_size.x();
        }

        // Draw segment as clickable
        let seg_size = ui.measure_text(segment, ui.scaled_font_size(katla_ui::FontSize::Small));
        let seg_bounds = Rect2D::from_origin_size(
            Vec2::new(breadcrumb_x, breadcrumb_y - 2.0),
            Vec2::new(seg_size.x(), breadcrumb_height),
        );

        let is_last = i == segments.len() - 1;
        let is_hovered = ui.is_hovered(seg_bounds);
        let seg_color = if is_last {
            theme.text_primary
        } else if is_hovered {
            theme.text_accent
        } else {
            theme.text_secondary
        };

        ui.draw_text(
            segment,
            Vec2::new(breadcrumb_x, breadcrumb_y),
            seg_color,
            ui.scaled_font_size(katla_ui::FontSize::Small),
        );

        // Click to navigate
        if is_hovered && !is_last && ui.input.mouse_clicked(katla_ui::input::mouse_button::LEFT) {
            clicked_segment = Some(i);
        }

        breadcrumb_x += seg_size.x() + 2.0;
    }

    // Process breadcrumb click after iteration
    if let Some(idx) = clicked_segment {
        state.navigate_to_segment(idx, thumbnail_texture_ids);
    }

    // === Navigation Buttons (right side of toolbar) ===
    let nav_btn_size = 24.0;
    let nav_icon_size = 12.0;
    let mut nav_x = bounds.max.x() - nav_btn_size - 4.0;

    // Refresh button
    let refresh_bounds = Rect2D::from_origin_size(
        Vec2::new(nav_x, toolbar_top + 2.0),
        Vec2::new(nav_btn_size, nav_btn_size),
    );
    let refresh_hovered = ui.is_hovered(refresh_bounds);

    if refresh_hovered {
        ui.draw_rect(refresh_bounds, theme.button_hover);
    }

    if refresh_hovered && ui.input.mouse_clicked(katla_ui::input::mouse_button::LEFT) {
        state.refresh(thumbnail_texture_ids);
    }

    ui.draw_icon_aligned(
        ForkAwesome::REFRESH,
        Vec2::new(refresh_bounds.min.x() + 5.0, refresh_bounds.center().y() - 7.0),
        nav_icon_size,
        if refresh_hovered { theme.text_primary } else { theme.text_secondary },
        katla_ui::FontId::DEFAULT,
    );
    nav_x -= nav_btn_size + 2.0;

    // Forward button
    let forward_bounds = Rect2D::from_origin_size(
        Vec2::new(nav_x, toolbar_top + 2.0),
        Vec2::new(nav_btn_size, nav_btn_size),
    );
    let forward_hovered = ui.is_hovered(forward_bounds);
    let can_forward = state.can_go_forward();

    if forward_hovered && can_forward {
        ui.draw_rect(forward_bounds, theme.button_hover);
    }

    if forward_hovered && can_forward && ui.input.mouse_clicked(katla_ui::input::mouse_button::LEFT) {
        state.navigate_forward(thumbnail_texture_ids);
    }

    ui.draw_icon_aligned(
        ForkAwesome::ARROW_RIGHT,
        Vec2::new(forward_bounds.min.x() + 5.0, forward_bounds.center().y() - 7.0),
        nav_icon_size,
        if can_forward {
            if forward_hovered { theme.text_primary } else { theme.text_secondary }
        } else {
            theme.text_muted
        },
        katla_ui::FontId::DEFAULT,
    );
    nav_x -= nav_btn_size + 2.0;

    // Back button
    let back_bounds = Rect2D::from_origin_size(
        Vec2::new(nav_x, toolbar_top + 2.0),
        Vec2::new(nav_btn_size, nav_btn_size),
    );
    let back_hovered = ui.is_hovered(back_bounds);
    let can_back = state.can_go_back();

    if back_hovered && can_back {
        ui.draw_rect(back_bounds, theme.button_hover);
    }

    if back_hovered && can_back && ui.input.mouse_clicked(katla_ui::input::mouse_button::LEFT) {
        state.navigate_back(thumbnail_texture_ids);
    }

    ui.draw_icon_aligned(
        ForkAwesome::ARROW_LEFT,
        Vec2::new(back_bounds.min.x() + 5.0, back_bounds.center().y() - 7.0),
        nav_icon_size,
        if can_back {
            if back_hovered { theme.text_primary } else { theme.text_secondary }
        } else {
            theme.text_muted
        },
        katla_ui::FontId::DEFAULT,
    );

    // Search box (left of navigation buttons)
    let search_width = 100.0;
    let search_height = 20.0;
    let search_bounds = Rect2D::from_origin_size(
        Vec2::new(back_bounds.min.x() - search_width - 8.0, toolbar_top + 4.0),
        Vec2::new(search_width, search_height),
    );

    // Search input background
    ui.draw_rect(search_bounds, theme.background);
    ui.draw_rect_border(search_bounds, theme.background, theme.border, 1.0);

    // Search icon
    ui.draw_icon_aligned(
        ForkAwesome::SEARCH,
        Vec2::new(search_bounds.min.x() + 4.0, search_bounds.center().y() - 7.0),
        12.0,
        theme.text_muted,
        katla_ui::FontId::DEFAULT,
    );

    // Search text
    let search_text_x = search_bounds.min.x() + 18.0;
    if state.search_filter.is_empty() {
        ui.draw_text(
            "Filter...",
            Vec2::new(search_text_x, search_bounds.center().y() - ui.scaled_font_size(katla_ui::FontSize::XSmall) * 0.5),
            theme.text_muted,
            ui.scaled_font_size(katla_ui::FontSize::XSmall),
        );
    } else {
        ui.draw_text(
            &state.search_filter,
            Vec2::new(search_text_x, search_bounds.center().y() - ui.scaled_font_size(katla_ui::FontSize::XSmall) * 0.5),
            theme.text_primary,
            ui.scaled_font_size(katla_ui::FontSize::XSmall),
        );
    }

    // Handle search focus and input
    if ui.is_hovered(search_bounds) {
        // Show text cursor when hovering over search field
        ui.set_mouse_cursor(katla_ui::input::MouseCursor::Text);

        if ui.input.mouse_clicked(katla_ui::input::mouse_button::LEFT) {
            state.search_focused = true;
            state.rename_mode = false; // Close rename if open
        }
    } else if ui.input.mouse_clicked(katla_ui::input::mouse_button::LEFT) {
        state.search_focused = false;
    }

    // Check focus AFTER click handling (use focused_panel directly, not cached is_focused)
    if state.search_focused && *focused_panel == FocusedPanel::AssetBrowser {
        // Capture keyboard so game doesn't get input
        ui.input.want_capture_keyboard = true;

        // Handle text input
        let prev_filter = state.search_filter.clone();
        for c in &ui.input.characters.clone() {
            if *c == '\x08' {
                state.search_filter.pop();
            } else if *c >= ' ' && state.search_filter.len() < 32 {
                state.search_filter.push(*c);
            }
        }

        // Rescan if filter changed
        if prev_filter != state.search_filter {
            state.refresh(thumbnail_texture_ids);
        }
    }

    // Toolbar bottom border
    ui.draw_line(
        Vec2::new(bounds.min.x(), toolbar_top + toolbar_height),
        Vec2::new(bounds.max.x(), toolbar_top + toolbar_height),
        theme.separator,
        1.0,
    );

    // === CONTENT AREA ===
    let content_top = toolbar_top + toolbar_height;
    let content_bounds = Rect2D::new(
        Vec2::new(bounds.min.x(), content_top),
        bounds.max,
    );

    // Push clipping for content
    ui.push_clip(content_bounds);

    // Grid layout parameters
    let item_size = 64.0;
    let item_padding = 8.0;
    let col_count = ((bounds.width() - item_padding) / (item_size + item_padding)).max(1.0) as usize;
    let row_height = item_size + 24.0; // Item + label

    // Handle scrolling
    let total_rows = (state.assets.len() + col_count - 1) / col_count;
    let content_height = total_rows as f32 * row_height;
    let max_scroll = state.max_scroll(content_height, content_bounds.height());

    if ui.is_hovered(content_bounds) {
        let scroll_delta = ui.input.scroll_delta.y() * 30.0;
        state.scroll_offset = (state.scroll_offset - scroll_delta).clamp(0.0, max_scroll);
    }

    // Draw assets in grid
    // Track actions to perform after iteration (to avoid borrow conflicts)
    let mut clicked_index: Option<usize> = None;
    let mut right_clicked_index: Option<usize> = None;
    let mut drag_start_index: Option<usize> = None;
    let mut should_navigate: Option<PathBuf> = None;

    for (i, asset) in state.assets.iter().enumerate() {
        let col = i % col_count;
        let row = i / col_count;

        let item_x = bounds.min.x() + item_padding + col as f32 * (item_size + item_padding);
        let item_y = content_top + row as f32 * row_height - state.scroll_offset;

        // Skip items that are outside the visible area
        if item_y + row_height < content_top || item_y > bounds.max.y() {
            continue;
        }

        let item_pos = Vec2::new(item_x, item_y);
        let item_bounds = Rect2D::from_origin_size(item_pos, Vec2::new(item_size, item_size));

        // Background on hover/select (check both single and multi-select)
        let is_selected = state.selected_index == Some(i) || state.selected_indices.contains(&i);
        let is_hovered = ui.is_hovered(item_bounds);

        if is_selected {
            ui.draw_rect(item_bounds, theme.selection);
        } else if is_hovered {
            ui.draw_rect(item_bounds, theme.selection_hover);
        }

        // Draw thumbnail or icon centered in item
        match &asset.thumbnail_state {
            ThumbnailState::Loaded { texture_id } => {
                // Draw thumbnail image with inset so selection/hover background is visible
                // UV.x >= 1.0 signals the shader to sample from the dynamic texture (set 1)
                // Add 1.0 to UV.x to shift from 0-1 range to 1-2 range
                let uv_offset = Rect2D::new(
                    Vec2::new(1.0, 0.0),  // UV min (offset by 1.0 in x)
                    Vec2::new(2.0, 1.0),  // UV max (offset by 1.0 in x)
                );
                // Inset thumbnail by 3 pixels to show selection/hover background
                let inset = 3.0;
                let thumb_bounds = Rect2D::from_origin_size(
                    Vec2::new(item_bounds.min.x() + inset, item_bounds.min.y() + inset),
                    Vec2::new(item_bounds.width() - inset * 2.0, item_bounds.height() - inset * 2.0),
                );
                ui.image(
                    *texture_id,
                    thumb_bounds,
                    Some(uv_offset),
                    None,  // White tint
                );
            }
            ThumbnailState::Loading => {
                // Show dimmed icon while loading
                let icon = asset.asset_type.icon();
                let icon_color = asset.asset_type.color(theme).with_alpha(0.5);
                let icon_size = 28.0;
                let icon_pos = Vec2::new(
                    item_bounds.center().x() - icon_size * 0.5,
                    item_bounds.center().y() - icon_size * 0.5,
                );
                ui.draw_icon(icon, icon_pos, icon_size, icon_color);
            }
            ThumbnailState::Failed => {
                // Show error icon
                let icon = ForkAwesome::TIMES_CIRCLE;
                let icon_color = theme.error;
                let icon_size = 28.0;
                let icon_pos = Vec2::new(
                    item_bounds.center().x() - icon_size * 0.5,
                    item_bounds.center().y() - icon_size * 0.5,
                );
                ui.draw_icon(icon, icon_pos, icon_size, icon_color);
            }
            ThumbnailState::NotRequested => {
                // Show regular icon
                let icon = asset.asset_type.icon();
                let icon_color = asset.asset_type.color(theme);
                let icon_size = 28.0;
                let icon_pos = Vec2::new(
                    item_bounds.center().x() - icon_size * 0.5,
                    item_bounds.center().y() - icon_size * 0.5,
                );
                ui.draw_icon(icon, icon_pos, icon_size, icon_color);
            }
        }

        // Draw selection border if selected (just the border, don't fill - icon already drawn)
        if is_selected {
            let border_width = 2.0;
            // Top
            ui.draw_rect(
                Rect2D::from_origin_size(item_bounds.min, Vec2::new(item_bounds.width(), border_width)),
                theme.highlight,
            );
            // Bottom
            ui.draw_rect(
                Rect2D::from_origin_size(
                    Vec2::new(item_bounds.min.x(), item_bounds.max.y() - border_width),
                    Vec2::new(item_bounds.width(), border_width),
                ),
                theme.highlight,
            );
            // Left
            ui.draw_rect(
                Rect2D::from_origin_size(item_bounds.min, Vec2::new(border_width, item_bounds.height())),
                theme.highlight,
            );
            // Right
            ui.draw_rect(
                Rect2D::from_origin_size(
                    Vec2::new(item_bounds.max.x() - border_width, item_bounds.min.y()),
                    Vec2::new(border_width, item_bounds.height()),
                ),
                theme.highlight,
            );
        }

        // Draw name below icon (truncated)
        let label_y = item_y + item_size + 2.0;
        let max_label_width = item_size + item_padding;

        // Truncate name if too long
        let display_name = truncate_text(asset.name.as_str(), max_label_width, ui);

        let label_size = ui.measure_text(&display_name, ui.scaled_font_size(katla_ui::FontSize::XSmall));
        let label_pos = Vec2::new(
            item_bounds.center().x() - label_size.x() * 0.5,
            label_y,
        );
        ui.draw_text(
            &display_name,
            label_pos,
            theme.text_secondary,
            ui.scaled_font_size(katla_ui::FontSize::XSmall),
        );

        // Handle click - just record the click for now
        if is_hovered && ui.input.mouse_clicked(katla_ui::input::mouse_button::LEFT) {
            clicked_index = Some(i);
            if asset.asset_type == AssetType::Folder {
                should_navigate = Some(asset.path.clone());
            }
            // Track potential drag start
            drag_start_index = Some(i);
        }

        // Handle right-click for context menu
        if is_hovered && ui.input.mouse_clicked(katla_ui::input::mouse_button::RIGHT) {
            right_clicked_index = Some(i);
            state.selected_index = Some(i);
        }
    }

    // === MARQUEE SELECTION ===
    // Handle rectangle selection in content area
    {
        let mouse_in_content = content_bounds.contains(ui.input.mouse_pos);
        let mouse_down = ui.input.is_mouse_down(katla_ui::input::mouse_button::LEFT);

        // Start marquee on click in content area (but not on an asset)
        if mouse_in_content && ui.input.mouse_clicked(katla_ui::input::mouse_button::LEFT) && clicked_index.is_none() {
            state.selection_rect_start = Some(ui.input.mouse_pos);
            state.selection_rect_current = Some(ui.input.mouse_pos);
            state.is_marquee_selecting = false; // Will become true on drag
        }

        // Update marquee rectangle while dragging
        if state.selection_rect_start.is_some() && mouse_down {
            state.selection_rect_current = Some(ui.input.mouse_pos);
            let start = state.selection_rect_start.unwrap();
            let current = ui.input.mouse_pos;
            let dist = (current - start).length();
            if dist > state.drag_threshold {
                state.is_marquee_selecting = true;
            }
        }

        // Draw selection rectangle and select assets on release
        if state.is_marquee_selecting {
            if let (Some(start), Some(current)) = (state.selection_rect_start, state.selection_rect_current) {
                // Draw the selection rectangle
                let rect_min = Vec2::new(start.x().min(current.x()), start.y().min(current.y()));
                let rect_max = Vec2::new(start.x().max(current.x()), start.y().max(current.y()));
                let sel_rect = Rect2D::new(rect_min, rect_max);

                ui.draw_rect(sel_rect, Color::new(0.3, 0.5, 0.8, 0.3));
                ui.draw_rect_border(sel_rect, Color::new(0.3, 0.5, 0.8, 0.3), Color::new(0.4, 0.6, 0.9, 0.8), 1.0);
            }
        }

        // Finalize selection on mouse release
        if state.selection_rect_start.is_some() && ui.input.mouse_released[katla_ui::input::mouse_button::LEFT] {
            if state.is_marquee_selecting {
                if let (Some(start), Some(current)) = (state.selection_rect_start, state.selection_rect_current) {
                    // Build selection rectangle
                    let rect_min = Vec2::new(start.x().min(current.x()), start.y().min(current.y()));
                    let rect_max = Vec2::new(start.x().max(current.x()), start.y().max(current.y()));
                    let sel_rect = Rect2D::new(rect_min, rect_max);

                    // Clear previous selection
                    state.selected_indices.clear();
                    state.selected_index = None;

                    // Select all assets that intersect with the rectangle
                    for (i, _asset) in state.assets.iter().enumerate() {
                        let col = i % col_count;
                        let row = i / col_count;
                        let item_x = bounds.min.x() + item_padding + col as f32 * (item_size + item_padding);
                        let item_y = content_top + row as f32 * row_height - state.scroll_offset;
                        let item_bounds = Rect2D::from_origin_size(Vec2::new(item_x, item_y), Vec2::new(item_size, item_size));

                        // Check if item intersects with selection rectangle (AABB intersection)
                        if item_bounds.min.x() <= sel_rect.max.x()
                            && item_bounds.max.x() >= sel_rect.min.x()
                            && item_bounds.min.y() <= sel_rect.max.y()
                            && item_bounds.max.y() >= sel_rect.min.y()
                        {
                            state.selected_indices.insert(i);
                            if state.selected_index.is_none() {
                                state.selected_index = Some(i); // Set primary selection
                            }
                        }
                    }
                }
            } else {
                // Simple click in empty space (not marquee) - clear selection
                state.selected_index = None;
                state.selected_indices.clear();
            }
            // Reset marquee state
            state.selection_rect_start = None;
            state.selection_rect_current = None;
            state.is_marquee_selecting = false;
        }
    }

    // === THUMBNAIL REQUESTING ===
    // Request thumbnails for visible images that haven't been requested yet
    {
        // Collect paths that need thumbnails (to avoid borrow conflicts)
        let mut thumbs_to_request: Vec<(usize, PathBuf)> = Vec::new();

        for (i, asset) in state.assets.iter().enumerate() {
            // Only request thumbnails for images
            if asset.asset_type != AssetType::Image {
                continue;
            }

            // Check if visible in viewport
            let col = i % col_count;
            let row = i / col_count;
            let item_y = content_top + row as f32 * row_height - state.scroll_offset;

            // Skip if outside visible area
            if item_y + row_height < content_top || item_y > bounds.max.y() {
                continue;
            }

            // Check if thumbnail needs to be requested
            if matches!(asset.thumbnail_state, ThumbnailState::NotRequested) {
                // Check if already cached in loader
                if loader.has_thumbnail(&asset.path) {
                    // Already cached, will be handled in poll()
                } else if !loader.is_loading(&asset.path) {
                    thumbs_to_request.push((i, asset.path.clone()));
                }
            }
        }

        // Request thumbnails (limited batch per frame to avoid overload)
        for (idx, path) in thumbs_to_request.into_iter().take(4) {
            loader.request_thumbnail(path, item_size as u32);
            state.assets[idx].thumbnail_state = ThumbnailState::Loading;
        }
    }

    // Start drag if tracked
    if let Some(idx) = drag_start_index {
        state.start_drag(idx, ui.input.mouse_pos);
    }

    // === DRAG AND DROP HANDLING ===
    // Update drag state while mouse is held
    if state.drag_asset.is_some() && ui.input.is_mouse_down(katla_ui::input::mouse_button::LEFT) {
        state.update_drag(ui.input.mouse_pos);
    }

    // Handle drag end - check what we're dropping on
    if state.drag_asset.is_some() && ui.input.mouse_released[katla_ui::input::mouse_button::LEFT] {
        let drag_idx = state.drag_asset.unwrap();
        let mouse_pos = ui.input.mouse_pos;
        let mouse_in_browser = bounds.contains(mouse_pos);

        if state.is_dragging {
            // Collect all assets to drag (single or multi-select)
            let mut assets_to_drag: Vec<(usize, PathBuf, AssetType)> = Vec::new();

            if !state.selected_indices.is_empty() && state.selected_indices.contains(&drag_idx) {
                // Drag all selected items
                for &idx in &state.selected_indices {
                    if let Some(asset) = state.assets.get(idx) {
                        assets_to_drag.push((idx, asset.path.clone(), asset.asset_type));
                    }
                }
            } else {
                // Drag only the clicked item
                if let Some(asset) = state.assets.get(drag_idx) {
                    assets_to_drag.push((drag_idx, asset.path.clone(), asset.asset_type));
                }
            }

            // Check if dropped on a folder
            let mut dropped_on_folder: Option<PathBuf> = None;

            if mouse_in_browser {
                for (i, asset) in state.assets.iter().enumerate() {
                    // Skip if this asset is being dragged
                    if assets_to_drag.iter().any(|(idx, _, _)| *idx == i) {
                        continue;
                    }

                    // Calculate item bounds
                    let col = i % col_count;
                    let row = i / col_count;
                    let item_x = bounds.min.x() + item_padding + col as f32 * (item_size + item_padding);
                    let item_y = content_top + row as f32 * row_height - state.scroll_offset;
                    let item_bounds = Rect2D::from_origin_size(
                        Vec2::new(item_x, item_y),
                        Vec2::new(item_size, item_size),
                    );

                    if item_bounds.contains(mouse_pos) && asset.asset_type == AssetType::Folder {
                        dropped_on_folder = Some(asset.path.clone());
                        break;
                    }
                }
            }

            if let Some(folder_path) = dropped_on_folder {
                // Drop on folder - move all dragged assets
                for (_, asset_path, _) in &assets_to_drag {
                    state.pending_actions.push(AssetAction::MoveToFolder {
                        asset_path: asset_path.clone(),
                        folder_path: folder_path.clone(),
                    });
                }
            } else if !mouse_in_browser {
                // Drop outside browser - spawn all models in viewport
                for (_, asset_path, asset_type) in &assets_to_drag {
                    if matches!(asset_type, AssetType::Model | AssetType::Image) {
                        state.pending_actions.push(AssetAction::DragToViewport {
                            path: asset_path.clone(),
                            asset_type: *asset_type,
                            screen_pos: mouse_pos,
                        });
                    }
                }
            }
        }
        state.end_drag();
    }

    // Cancel drag on escape
    if state.drag_asset.is_some() && ui.input.key_pressed(katla_ui::input::KeyCode::Escape) {
        state.cancel_drag();
    }

    // Note: Drag preview is now rendered at the EditorUI level for visibility across panels

    // === RENAME MODE HANDLING ===
    // Collect rename data first to avoid borrow conflicts
    let rename_data = if state.rename_mode {
        if let Some(rename_idx) = state.rename_asset {
            state.assets.get(rename_idx).map(|asset| {
                (
                    rename_idx,
                    asset.name.clone(),
                    asset.path.clone(),
                )
            })
        } else {
            None
        }
    } else {
        None
    };

    if let Some((rename_idx, original_name, original_path)) = rename_data {
        // Draw rename input overlay
        let col = rename_idx % col_count;
        let row = rename_idx / col_count;
        let item_x = bounds.min.x() + item_padding + col as f32 * (item_size + item_padding);
        let item_y = content_top + row as f32 * row_height - state.scroll_offset;

        let input_bounds = Rect2D::from_origin_size(
            Vec2::new(item_x, item_y + item_size + 2.0),
            Vec2::new(item_size, 18.0),
        );

        ui.push_z_index(250);
        ui.draw_rect(input_bounds, theme.background);
        ui.draw_rect_border(input_bounds, theme.background, theme.highlight, 1.0);

        // Draw rename text with cursor
        let text = &state.rename_buffer;
        ui.draw_text(
            text,
            Vec2::new(input_bounds.min.x() + 4.0, input_bounds.min.y() + 3.0),
            theme.text_primary,
            ui.scaled_font_size(katla_ui::FontSize::XSmall),
        );

        // Cursor (blink effect could be added)
        let text_width = ui.measure_text(text, ui.scaled_font_size(katla_ui::FontSize::XSmall)).x();
        ui.draw_rect(
            Rect2D::from_origin_size(
                Vec2::new(input_bounds.min.x() + 4.0 + text_width, input_bounds.min.y() + 2.0),
                Vec2::new(1.0, 14.0),
            ),
            theme.text_primary,
        );
        ui.pop_z_index();

        // Handle text input
        for c in &ui.input.characters.clone() {
            if *c == '\x08' {
                state.rename_buffer.pop();
            } else if *c >= ' ' && state.rename_buffer.len() < 64 {
                // Filter out invalid filename characters
                if !matches!(*c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                    state.rename_buffer.push(*c);
                }
            }
        }

        // Capture keyboard so game doesn't get input (only when panel is focused)
        if is_focused {
            ui.input.want_capture_keyboard = true;
        }

        // Track commit/cancel actions
        let mut should_commit = false;
        let mut should_cancel = false;

        // Commit on Enter
        if is_focused && ui.input.key_pressed(katla_ui::input::KeyCode::Enter) {
            should_commit = true;
        }

        // Cancel on Escape
        if is_focused && ui.input.key_pressed(katla_ui::input::KeyCode::Escape) {
            should_cancel = true;
        }

        // Commit on click outside
        if ui.input.mouse_clicked(katla_ui::input::mouse_button::LEFT) {
            if !ui.is_hovered(input_bounds) {
                should_commit = true;
            }
        }

        // Process actions (only when focused)
        if is_focused {
            if should_commit {
                let new_name = state.rename_buffer.clone();
                if new_name != original_name && !new_name.is_empty() {
                    let new_path = original_path.parent().unwrap().join(&new_name);
                    state.pending_actions.push(AssetAction::Rename {
                        old_path: original_path.clone(),
                        new_path,
                    });
                }
                state.cancel_rename();
            } else if should_cancel {
                state.cancel_rename();
            }
        }
    }

    // Process click actions after iteration (to avoid borrow conflicts)
    if let Some(index) = clicked_index {
        let is_double = state.is_double_click(index);

        // Check for modifier keys (Ctrl for toggle, Shift for range)
        let ctrl_held = ui.input.is_key_down(katla_ui::input::KeyCode::Control);
        let shift_held = ui.input.is_key_down(katla_ui::input::KeyCode::Shift);

        if ctrl_held {
            // Ctrl+Click: Toggle selection
            if state.selected_indices.contains(&index) || state.selected_index == Some(index) {
                // Deselect
                state.selected_indices.remove(&index);
                if state.selected_index == Some(index) {
                    state.selected_index = state.selected_indices.iter().next().copied();
                }
            } else {
                // Add to selection
                state.selected_indices.insert(index);
                state.selected_index = Some(index);
            }
        } else if shift_held && state.selected_index.is_some() {
            // Shift+Click: Range selection from last selected to this
            let start = state.selected_index.unwrap();
            let end = index;
            state.selected_indices.clear();
            for i in start.min(end)..=start.max(end) {
                if i < state.assets.len() {
                    state.selected_indices.insert(i);
                }
            }
        } else {
            // Normal click: Single selection
            state.selected_index = Some(index);
            state.selected_indices.clear();
        }

        if is_double {
            if let Some(path) = should_navigate {
                if path.ends_with("..") {
                    state.navigate_up(thumbnail_texture_ids);
                } else {
                    state.navigate_to(&path, thumbnail_texture_ids);
                }
            }
        }
    }

    // Process right-click to open context menu
    if let Some(index) = right_clicked_index {
        state.open_context_menu(index, ui.input.mouse_pos);
    }

    // Close context menu on click outside
    if state.context_menu_open {
        if ui.input.mouse_clicked(katla_ui::input::mouse_button::LEFT) {
            let menu_bounds = Rect2D::from_origin_size(
                state.context_menu_pos,
                Vec2::new(160.0, 120.0),
            );
            if !ui.is_hovered(menu_bounds) {
                state.close_context_menu();
            }
        }
    }

    // Empty state
    if state.assets.is_empty() {
        let empty_text = if state.search_filter.is_empty() {
            "No assets found"
        } else {
            "No matching assets"
        };
        let empty_size = ui.measure_text(empty_text, ui.scaled_font_size(katla_ui::FontSize::Medium));
        let empty_pos = Vec2::new(
            content_bounds.center().x() - empty_size.x() * 0.5,
            content_bounds.center().y() - empty_size.y() * 0.5,
        );
        ui.draw_text(
            empty_text,
            empty_pos,
            theme.text_muted,
            ui.scaled_font_size(katla_ui::FontSize::Medium),
        );
    }

    ui.pop_clip();

    // === CONTEXT MENU ===
    // Also handle empty space context menu (for creating folders, etc.)
    let mut empty_space_context_menu = false;
    if !state.context_menu_open && ui.is_hovered(content_bounds) {
        // Check if right-click was on empty space (not on any asset)
        let mut clicked_on_asset = false;
        for (i, asset) in state.assets.iter().enumerate() {
            let col = i % col_count;
            let row = i / col_count;
            let item_x = bounds.min.x() + item_padding + col as f32 * (item_size + item_padding);
            let item_y = content_top + row as f32 * row_height - state.scroll_offset;
            let item_bounds = Rect2D::from_origin_size(Vec2::new(item_x, item_y), Vec2::new(item_size, item_size + 16.0));
            if ui.is_hovered(item_bounds) {
                clicked_on_asset = true;
                break;
            }
        }

        if !clicked_on_asset && ui.input.mouse_clicked(katla_ui::input::mouse_button::RIGHT) {
            empty_space_context_menu = true;
            state.context_menu_pos = ui.input.mouse_pos;
            state.context_menu_asset = None; // None = empty space menu
            state.context_menu_open = true;
        }
    }

    // === ASSET CONTEXT MENU (or empty space menu) ===
    // Collect context menu data first to avoid borrow conflicts
    let context_menu_data = if state.context_menu_open {
        if let Some(asset_idx) = state.context_menu_asset {
            // Asset context menu
            state.assets.get(asset_idx).map(|asset| {
                (
                    Some(asset.asset_type),
                    asset.name.clone(),
                    asset.path.clone(),
                    state.context_menu_pos,
                    asset_idx,
                )
            })
        } else {
            // Empty space context menu
            Some((None, String::new(), state.current_path.clone(), state.context_menu_pos, 0))
        }
    } else {
        None
    };

    if let Some((asset_type, asset_name, asset_path, menu_pos, asset_idx)) = context_menu_data {
        let menu_width = 180.0;
        let item_height = 24.0;

        // Different menu items based on whether it's an asset or empty space
        let menu_items = if let Some(at) = asset_type {
            // Asset context menu
            if at == AssetType::Folder {
                vec![
                    ("Open", ForkAwesome::FOLDER_OPEN, true),
                    ("Rename", ForkAwesome::PENCIL, true),
                    ("separator", '\0', false),
                    ("Copy Path", ForkAwesome::COPY, true),
                    ("Show in Explorer", ForkAwesome::EXTERNAL_LINK, true),
                    ("separator", '\0', false),
                    ("Delete", ForkAwesome::TRASH, true),
                ]
            } else {
                vec![
                    ("Open", ForkAwesome::FILE, true),
                    ("Rename", ForkAwesome::PENCIL, true),
                    ("separator", '\0', false),
                    ("Copy Path", ForkAwesome::COPY, true),
                    ("Show in Explorer", ForkAwesome::EXTERNAL_LINK, true),
                    ("separator", '\0', false),
                    ("Delete", ForkAwesome::TRASH, true),
                ]
            }
        } else {
            // Empty space context menu
            vec![
                ("New Folder", ForkAwesome::FOLDER, true),
                ("separator", '\0', false),
                ("Refresh", ForkAwesome::REFRESH, true),
                ("separator", '\0', false),
                ("Show in Explorer", ForkAwesome::EXTERNAL_LINK, true),
            ]
        };

        // Filter out separators for height calculation
        let visible_items = menu_items.iter().filter(|(l, _, _)| *l != "separator").count();
        let separator_count = menu_items.iter().filter(|(l, _, _)| *l == "separator").count();
        let menu_height = (visible_items as f32 * item_height) + 8.0 + (separator_count as f32 * 4.0);

        // Clamp menu position to screen
        let menu_pos = Vec2::new(
            menu_pos.x().min(bounds.max.x() - menu_width - 10.0).max(10.0),
            menu_pos.y().min(bounds.max.y() - menu_height - 10.0).max(10.0),
        );
        let menu_bounds = Rect2D::from_origin_size(menu_pos, Vec2::new(menu_width, menu_height));

        // Push higher Z-index for menu
        ui.push_z_index(200); // z_index::POPUP

        // Shadow
        let shadow_bounds = Rect2D::new(
            menu_bounds.min + Vec2::new(3.0, 3.0),
            menu_bounds.max + Vec2::new(3.0, 3.0),
        );
        ui.draw_rect(shadow_bounds, Color::new(0.0, 0.0, 0.0, 0.5));

        // Background
        ui.draw_rect(menu_bounds, theme.popup_bg);
        ui.draw_rect_border(menu_bounds, theme.popup_bg, theme.popup_border, 1.0);

        // Track which action was clicked
        let mut clicked_action: Option<&str> = None;

        // Menu items
        let mut current_y = menu_pos.y() + 4.0;
        for (label, icon, enabled) in menu_items.iter() {
            if *label == "separator" {
                // Draw separator line
                ui.draw_line(
                    Vec2::new(menu_pos.x() + 8.0, current_y + 2.0),
                    Vec2::new(menu_pos.x() + menu_width - 8.0, current_y + 2.0),
                    theme.separator,
                    1.0,
                );
                current_y += 8.0;
                continue;
            }

            let item_bounds = Rect2D::from_origin_size(
                Vec2::new(menu_pos.x(), current_y),
                Vec2::new(menu_width, item_height),
            );
            let item_hovered = ui.is_hovered(item_bounds);

            if *enabled && item_hovered {
                ui.draw_rect(item_bounds, theme.selection_hover);
            }

            // Text position (baseline)
            let text_y = current_y + 6.0;
            let text_size = ui.scaled_font_size(katla_ui::FontSize::Small);

            // Icon - use same Y as text so they align
            ui.draw_icon_aligned(
                *icon,
                Vec2::new(menu_pos.x() + 8.0, text_y),
                12.0,
                if *enabled {
                    if item_hovered { theme.text_primary } else { theme.text_secondary }
                } else {
                    theme.text_muted
                },
                katla_ui::FontId::DEFAULT,
            );

            // Label
            ui.draw_text(
                label,
                Vec2::new(menu_pos.x() + 28.0, text_y),
                if *enabled {
                    if item_hovered { theme.text_primary } else { theme.text_secondary }
                } else {
                    theme.text_muted
                },
                text_size,
            );

            // Track click
            if *enabled && item_hovered && ui.input.mouse_clicked(katla_ui::input::mouse_button::LEFT) {
                clicked_action = Some(*label);
            }

            current_y += item_height;
        }

        ui.pop_z_index();

        // Process action after rendering (to avoid borrow conflicts)
        if let Some(action) = clicked_action {
            match action {
                "Open" => {
                    if asset_type == Some(AssetType::Folder) {
                        if asset_name == ".." {
                            state.navigate_up(thumbnail_texture_ids);
                        } else {
                            state.navigate_to(&asset_path, thumbnail_texture_ids);
                        }
                    } else if asset_type.is_some() {
                        state.pending_actions.push(AssetAction::Open(asset_path));
                    }
                }
                "Rename" => {
                    state.start_rename(asset_idx);
                }
                "Copy Path" => {
                    state.pending_actions.push(AssetAction::CopyPath(asset_path));
                }
                "Show in Explorer" => {
                    state.pending_actions.push(AssetAction::ShowInExplorer(asset_path));
                }
                "Delete" => {
                    // Show confirmation dialog instead of deleting immediately
                    let is_folder = asset_path.is_dir();
                    let name = asset_path.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "this item".to_string());
                    state.confirm_dialog_message = if is_folder {
                        format!("Delete folder \"{}\" and all its contents?", name)
                    } else {
                        format!("Delete \"{}\"?", name)
                    };
                    state.confirm_pending_action = Some(AssetAction::Delete(asset_path));
                    state.confirm_dialog_open = true;
                }
                "New Folder" => {
                    state.pending_actions.push(AssetAction::CreateFolder(asset_path));
                }
                "Refresh" => {
                    state.refresh(thumbnail_texture_ids);
                }
                _ => {}
            }
            state.close_context_menu();
        }
    }

    // === CONFIRMATION DIALOG ===
    if state.confirm_dialog_open {
        ui.push_z_index(300); // Higher than context menu

        // Darken background
        let screen_size = ui.screen_size();
        let screen_bounds = Rect2D::new(Vec2::new(0.0, 0.0), screen_size);
        ui.draw_rect(screen_bounds, Color::new(0.0, 0.0, 0.0, 0.5));

        // Dialog box
        let dialog_width = 320.0;
        let dialog_height = 120.0;
        let dialog_pos = Vec2::new(
            (screen_size.x() - dialog_width) * 0.5,
            (screen_size.y() - dialog_height) * 0.5,
        );
        let dialog_bounds = Rect2D::from_origin_size(dialog_pos, Vec2::new(dialog_width, dialog_height));

        // Shadow
        let shadow_bounds = Rect2D::new(dialog_bounds.min + Vec2::new(4.0, 4.0), dialog_bounds.max + Vec2::new(4.0, 4.0));
        ui.draw_rect(shadow_bounds, Color::new(0.0, 0.0, 0.0, 0.5));

        // Background
        ui.draw_rect(dialog_bounds, theme.popup_bg);
        ui.draw_rect_border(dialog_bounds, theme.popup_bg, theme.popup_border, 1.0);

        // Title bar
        let title_bounds = Rect2D::from_origin_size(dialog_pos, Vec2::new(dialog_width, 28.0));
        ui.draw_rect(title_bounds, theme.panel_header);
        ui.draw_text(
            "Confirm Delete",
            Vec2::new(dialog_pos.x() + 10.0, dialog_pos.y() + 7.0),
            theme.text_primary,
            ui.scaled_font_size(katla_ui::FontSize::Small),
        );

        // Message
        ui.draw_text(
            &state.confirm_dialog_message.clone(),
            Vec2::new(dialog_pos.x() + 10.0, dialog_pos.y() + 40.0),
            theme.text_secondary,
            ui.scaled_font_size(katla_ui::FontSize::Small),
        );

        // Buttons
        let btn_width = 80.0;
        let btn_height = 28.0;
        let btn_y = dialog_pos.y() + dialog_height - btn_height - 12.0;

        // No button
        let no_btn_bounds = Rect2D::from_origin_size(
            Vec2::new(dialog_pos.x() + dialog_width - btn_width * 2.0 - 20.0, btn_y),
            Vec2::new(btn_width, btn_height),
        );
        let no_hovered = ui.is_hovered(no_btn_bounds);
        if no_hovered {
            ui.draw_rect(no_btn_bounds, theme.button_hover);
        }
        ui.draw_rect_border(no_btn_bounds, theme.button_hover, theme.border, 1.0);
        let no_text_size = ui.measure_text("No", ui.scaled_font_size(katla_ui::FontSize::Small));
        ui.draw_text(
            "No",
            Vec2::new(no_btn_bounds.center().x() - no_text_size.x() * 0.5, no_btn_bounds.min.y() + 7.0),
            if no_hovered { theme.text_primary } else { theme.text_secondary },
            ui.scaled_font_size(katla_ui::FontSize::Small),
        );

        // Yes button
        let yes_btn_bounds = Rect2D::from_origin_size(
            Vec2::new(dialog_pos.x() + dialog_width - btn_width - 10.0, btn_y),
            Vec2::new(btn_width, btn_height),
        );
        let yes_hovered = ui.is_hovered(yes_btn_bounds);
        if yes_hovered {
            ui.draw_rect(yes_btn_bounds, theme.error);
        }
        ui.draw_rect_border(yes_btn_bounds, if yes_hovered { theme.error } else { theme.button_hover }, theme.border, 1.0);
        let yes_text_size = ui.measure_text("Yes", ui.scaled_font_size(katla_ui::FontSize::Small));
        ui.draw_text(
            "Yes",
            Vec2::new(yes_btn_bounds.center().x() - yes_text_size.x() * 0.5, yes_btn_bounds.min.y() + 7.0),
            theme.text_primary,
            ui.scaled_font_size(katla_ui::FontSize::Small),
        );

        ui.pop_z_index();

        // Handle button clicks
        if no_hovered && ui.input.mouse_clicked(katla_ui::input::mouse_button::LEFT) {
            state.confirm_dialog_open = false;
            state.confirm_pending_action = None;
        }
        if yes_hovered && ui.input.mouse_clicked(katla_ui::input::mouse_button::LEFT) {
            // Execute the pending action
            if let Some(action) = state.confirm_pending_action.take() {
                state.pending_actions.push(action);
            }
            state.confirm_dialog_open = false;
        }

        // Capture keyboard to prevent background actions
        ui.input.want_capture_keyboard = true;

        // Escape cancels
        if ui.input.key_pressed(katla_ui::input::KeyCode::Escape) {
            state.confirm_dialog_open = false;
            state.confirm_pending_action = None;
        }
    }

    // === KEYBOARD NAVIGATION ===
    if !state.search_focused && !state.context_menu_open {
        use katla_ui::input::KeyCode;

        if ui.input.key_pressed(KeyCode::ArrowUp) {
            state.handle_keyboard(KeyCode::ArrowUp, thumbnail_texture_ids);
        }
        if ui.input.key_pressed(KeyCode::ArrowDown) {
            state.handle_keyboard(KeyCode::ArrowDown, thumbnail_texture_ids);
        }
        if ui.input.key_pressed(KeyCode::ArrowLeft) {
            state.handle_keyboard(KeyCode::ArrowLeft, thumbnail_texture_ids);
        }
        if ui.input.key_pressed(KeyCode::ArrowRight) {
            state.handle_keyboard(KeyCode::ArrowRight, thumbnail_texture_ids);
        }
        if ui.input.key_pressed(KeyCode::Enter) {
            if let Some(action) = state.handle_keyboard(KeyCode::Enter, thumbnail_texture_ids) {
                state.pending_actions.push(action);
            }
        }
        if ui.input.key_pressed(KeyCode::Backspace) {
            state.handle_keyboard(KeyCode::Backspace, thumbnail_texture_ids);
        }
    }
}

/// Truncate text to fit within a maximum width, adding ellipsis if needed.
fn truncate_text(text: &str, max_width: f32, ui: &UiContext) -> String {
    let full_width = ui.measure_text(text, ui.scaled_font_size(katla_ui::FontSize::XSmall)).x();

    if full_width <= max_width {
        return text.to_string();
    }

    // Binary search for the right length
    let mut len = text.len();
    while len > 0 {
        let truncated = format!("{}...", &text[..len]);
        let width = ui.measure_text(&truncated, ui.scaled_font_size(katla_ui::FontSize::XSmall)).x();
        if width <= max_width {
            return truncated;
        }
        len -= 1;
    }

    "...".to_string()
}
