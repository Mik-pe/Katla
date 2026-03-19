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

use katla_gfx::TextureHandle;
use katla_math::{Color, Rect2D, Vec2};
use katla_ui::widgets::ImageButton;
use katla_ui::{
    mouse_button, ForkAwesome, KeyCode, ScrollArea, ScrollAreaState, TextureId, UiContext,
};

use super::FocusedPanel;
use crate::ui::theme::Theme;

/// Build a rectangle from two corner points (handles any ordering).
#[inline]
fn rect_from_points(a: Vec2, b: Vec2) -> Rect2D {
    Rect2D::new(
        Vec2::new(a.x().min(b.x()), a.y().min(b.y())),
        Vec2::new(a.x().max(b.x()), a.y().max(b.y())),
    )
}

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
#[derive(Debug, Clone, Default)]
pub enum ThumbnailState {
    /// Thumbnail not yet loaded or requested.
    #[default]
    Pending,
    /// Currently loading in background thread.
    Loading,
    /// Loaded and uploaded to GPU.
    Loaded { texture_handle: TextureHandle },
    /// Failed to load.
    Failed,
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
            Self::Font => ForkAwesome::FILE_TEXT,
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
    /// Scroll state for the content area
    pub scroll_state: ScrollAreaState,
    /// Panel height in pixels (when not collapsed)
    pub panel_height: f32,
    /// Whether panel is collapsed
    pub collapsed: bool,
    /// Last time directory was scanned
    last_scan: Option<Instant>,
    /// Index of last clicked item (for double-click same-item check)
    last_click_index: Option<usize>,
    /// Search/filter text
    pub search_filter: String,
    /// Whether search input is focused
    pub search_focused: bool,
    /// Context menu is open
    pub context_menu_open: bool,
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
    /// Request model preview (double-click on model file)
    ModelPreviewRequested(PathBuf),
    /// Copy path to clipboard
    CopyPath(PathBuf),
    /// Show in Explorer/Finder
    ShowInExplorer(PathBuf),
    /// Delete asset
    Delete(PathBuf),
    /// Rename asset (old_path, new_path)
    Rename {
        old_path: PathBuf,
        new_path: PathBuf,
    },
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
        }
    }

    /// Scan the current directory for assets.
    pub fn scan_directory(&mut self, thumbnail_texture_handles: &HashMap<PathBuf, TextureHandle>) {
        // Preserve thumbnail states before clearing
        let old_thumbnails: HashMap<PathBuf, ThumbnailState> = self
            .assets
            .iter()
            .map(|a| (a.path.clone(), a.thumbnail_state.clone()))
            .collect();

        self.assets.clear();

        // Add parent directory entry if not at root (parent must be different from current)
        if let Some(parent) = self.current_path.parent() {
            if parent != self.current_path && !parent.as_os_str().is_empty() {
                self.assets.push(AssetEntry {
                    name: "..".to_string(),
                    path: parent.to_path_buf(),
                    asset_type: AssetType::Folder,
                    thumbnail_state: ThumbnailState::Pending,
                });
            }
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
    pub fn refresh(&mut self, thumbnail_texture_handles: &HashMap<PathBuf, TextureHandle>) {
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
        thumbnail_texture_handles: &HashMap<PathBuf, TextureHandle>,
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
    pub fn navigate_up(&mut self, thumbnail_texture_handles: &HashMap<PathBuf, TextureHandle>) {
        if let Some(parent) = self.current_path.parent() {
            let parent_path = parent.to_path_buf();
            if parent_path != self.current_path {
                self.navigate_to(&parent_path, thumbnail_texture_handles);
            }
        }
    }

    /// Navigate back in history.
    pub fn navigate_back(&mut self, thumbnail_texture_handles: &HashMap<PathBuf, TextureHandle>) {
        if self.nav_history_pos > 0 {
            self.nav_history_pos -= 1;
            self.current_path = self.nav_history[self.nav_history_pos].clone();
            self.scan_directory(thumbnail_texture_handles);
        }
    }

    /// Navigate forward in history.
    pub fn navigate_forward(
        &mut self,
        thumbnail_texture_handles: &HashMap<PathBuf, TextureHandle>,
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
        thumbnail_texture_handles: &HashMap<PathBuf, TextureHandle>,
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

    /// Get max scroll offset based on content.
    pub fn max_scroll(&self, content_height: f32, visible_height: f32) -> f32 {
        (content_height - visible_height).max(0.0)
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
    pub fn handle_keyboard(
        &mut self,
        key: katla_ui::input::KeyCode,
        thumbnail_texture_handles: &HashMap<PathBuf, TextureHandle>,
    ) -> Option<AssetAction> {
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
        let col_count = 8;

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

/// Build the asset browser panel.
pub fn build_asset_browser(
    state: &mut AssetBrowserState,
    ui: &mut UiContext,
    theme: &Theme,
    bounds: Rect2D,
    focused_panel: &mut FocusedPanel,
    loader: &mut crate::util::BackgroundLoader,
    thumbnail_texture_handles: &HashMap<PathBuf, TextureHandle>,
) {
    let is_focused = *focused_panel == FocusedPanel::AssetBrowser;
    // Auto-rescan if needed
    if state.needs_rescan() {
        state.scan_directory(thumbnail_texture_handles);
    }

    // Panel background
    ui.draw_rect(bounds, theme.panel_bg);

    // Focus this panel when clicked
    if ui.is_hovered(bounds) && ui.mouse_clicked(mouse_button::LEFT) {
        *focused_panel = FocusedPanel::AssetBrowser;
    }

    // Draw focus indicator border if focused
    if is_focused {
        ui.draw_rect_border(bounds, theme.panel_bg, theme.highlight, 2.0);
    }

    // Header with breadcrumbs and controls
    let header_height = ui.style.button_height_small;
    let toolbar_height = ui.style.toolbar_height;
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

    if ui
        .add(katla_ui::widgets::ImageButton::new(toggle_icon).bounds(toggle_bounds))
        .clicked
    {
        state.collapsed = !state.collapsed;
    }

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
    let title_width = ui
        .measure_text(
            "Asset Browser",
            ui.scaled_font_size(katla_ui::FontSize::Medium),
        )
        .x();
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

    let toolbar_top = bounds.min.y() + header_height;
    let toolbar_bounds = Rect2D::from_origin_size(
        Vec2::new(bounds.min.x(), toolbar_top),
        Vec2::new(bounds.width(), toolbar_height),
    );
    ui.draw_rect(toolbar_bounds, theme.background_dark);

    // Breadcrumb navigation
    let mut breadcrumb_x = bounds.min.x() + padding;
    let breadcrumb_y =
        toolbar_bounds.center().y() - ui.scaled_font_size(katla_ui::FontSize::Small) * 0.5;
    let breadcrumb_height = ui.scaled_font_size(katla_ui::FontSize::Small) + 4.0;
    let segments = state.path_segments();

    // Track breadcrumb clicks
    let mut clicked_segment: Option<usize> = None;

    for (i, segment) in segments.iter().enumerate() {
        // Draw separator (except for first)
        if i > 0 {
            let sep_text = " / ";
            let sep_size =
                ui.measure_text(sep_text, ui.scaled_font_size(katla_ui::FontSize::Small));
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
        if is_hovered && !is_last && ui.mouse_clicked(mouse_button::LEFT) {
            clicked_segment = Some(i);
        }

        breadcrumb_x += seg_size.x() + 2.0;
    }

    // Process breadcrumb click after iteration
    if let Some(idx) = clicked_segment {
        state.navigate_to_segment(idx, thumbnail_texture_handles);
    }

    let nav_btn_size = ui.style.button_height_small;
    let mut nav_x = bounds.max.x() - nav_btn_size - 4.0;

    // Refresh button
    let refresh_bounds = Rect2D::from_origin_size(
        Vec2::new(nav_x, toolbar_top + 2.0),
        Vec2::new(nav_btn_size, nav_btn_size),
    );

    if ui
        .add(katla_ui::widgets::ImageButton::new(ForkAwesome::REFRESH).bounds(refresh_bounds))
        .clicked
    {
        state.refresh(thumbnail_texture_handles);
    }
    nav_x -= nav_btn_size + 2.0;

    // Forward button
    let forward_bounds = Rect2D::from_origin_size(
        Vec2::new(nav_x, toolbar_top + 2.0),
        Vec2::new(nav_btn_size, nav_btn_size),
    );
    let can_forward = state.can_go_forward();

    if ui
        .add(
            ImageButton::new(ForkAwesome::ARROW_RIGHT)
                .bounds(forward_bounds)
                .enabled(can_forward),
        )
        .clicked
    {
        state.navigate_forward(thumbnail_texture_handles);
    }
    nav_x -= nav_btn_size + 2.0;

    // Back button
    let back_bounds = Rect2D::from_origin_size(
        Vec2::new(nav_x, toolbar_top + 2.0),
        Vec2::new(nav_btn_size, nav_btn_size),
    );
    let can_back = state.can_go_back();

    if ui
        .add(
            ImageButton::new(ForkAwesome::ARROW_LEFT)
                .bounds(back_bounds)
                .enabled(can_back),
        )
        .clicked
    {
        state.navigate_back(thumbnail_texture_handles);
    }

    // Search box (left of navigation buttons)
    let search_width = 100.0;
    let search_height = 20.0;
    let search_bounds = Rect2D::from_origin_size(
        Vec2::new(back_bounds.min.x() - search_width - 8.0, toolbar_top + 4.0),
        Vec2::new(search_width, search_height),
    );

    let search_response = ui.add(
        katla_ui::widgets::TextInput::new(&mut state.search_filter)
            .bounds(search_bounds)
            .placeholder("Filter...")
            .show_clear(true),
    );

    state.search_focused = search_response.active;
    if search_response.changed {
        state.refresh(thumbnail_texture_handles);
    }

    // Toolbar bottom border
    ui.draw_line(
        Vec2::new(bounds.min.x(), toolbar_top + toolbar_height),
        Vec2::new(bounds.max.x(), toolbar_top + toolbar_height),
        theme.separator,
        1.0,
    );

    let content_top = toolbar_top + toolbar_height;
    let content_bounds = Rect2D::new(Vec2::new(bounds.min.x(), content_top), bounds.max);

    state.scroll_state = ui.scroll_area(
        ScrollArea::new("asset_scroll").max_height(content_bounds.height()),
        state.scroll_state,
        content_bounds,
        |ui| {
            let scroll_offset = ui.scroll_offset();

            let item_size = ui.style.thumbnail_size;
            let item_padding = 8.0;
            let col_count =
                ((bounds.width() - item_padding) / (item_size + item_padding)).max(1.0) as usize;
            let row_height = item_size + 24.0;

            let mut clicked_index: Option<usize> = None;
            let mut right_clicked_index: Option<usize> = None;
            let mut drag_start_index: Option<usize> = None;
            let mut should_navigate: Option<PathBuf> = None;
            let mut should_preview_model: Option<PathBuf> = None;

            for (i, asset) in state.assets.iter().enumerate() {
                let col = i % col_count;
                let row = i / col_count;

                let item_x =
                    bounds.min.x() + item_padding + col as f32 * (item_size + item_padding);
                let item_y = content_top + row as f32 * row_height - scroll_offset;

                if item_y + row_height < content_top || item_y > bounds.max.y() {
                    continue;
                }

                let item_pos = Vec2::new(item_x, item_y);
                let item_bounds =
                    Rect2D::from_origin_size(item_pos, Vec2::new(item_size, item_size));

                let is_selected =
                    state.selected_index == Some(i) || state.selected_indices.contains(&i);
                let is_hovered = ui.is_hovered(item_bounds);

                if is_selected {
                    ui.draw_rect(item_bounds, theme.selection);
                } else if is_hovered {
                    ui.draw_rect(item_bounds, theme.selection_hover);
                }

                let icon_pos = Vec2::new(
                    item_bounds.center().x() - ui.style.icon_size_large * 0.5,
                    item_bounds.center().y() - ui.style.icon_size_large * 0.5,
                );

                match &asset.thumbnail_state {
                    ThumbnailState::Loaded { texture_handle } => {
                        let inset = 3.0;
                        let thumb_bounds = Rect2D::from_origin_size(
                            Vec2::new(item_bounds.min.x() + inset, item_bounds.min.y() + inset),
                            Vec2::new(
                                item_bounds.width() - inset * 2.0,
                                item_bounds.height() - inset * 2.0,
                            ),
                        );
                        ui.image(
                            TextureId::from_handle_index(texture_handle.index()),
                            thumb_bounds,
                            None,
                            Some(Color::WHITE),
                        );
                    }
                    ThumbnailState::Loading => {
                        let icon_size = ui.style.icon_size_large;
                        let rotation = (std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis()
                            % 1000) as f32
                            / 1000.0
                            * std::f32::consts::TAU;
                        let spinner_chars = ['|', '/', '—', '\\'];
                        let spinner_idx = ((rotation / std::f32::consts::FRAC_PI_2) as usize) % 4;
                        ui.draw_icon(
                            spinner_chars[spinner_idx],
                            icon_pos,
                            icon_size,
                            theme.text_secondary,
                        );
                    }
                    ThumbnailState::Failed => {
                        ui.draw_icon(
                            ForkAwesome::TIMES_CIRCLE,
                            icon_pos,
                            ui.style.icon_size_large,
                            theme.error,
                        );
                    }
                    ThumbnailState::Pending => {
                        ui.draw_icon(
                            asset.asset_type.icon(),
                            icon_pos,
                            ui.style.icon_size_large,
                            asset.asset_type.color(theme),
                        );
                    }
                }

                if is_selected {
                    ui.draw_selection_border(item_bounds, theme.highlight, 2.0);
                }

                let label_y = item_y + item_size + 2.0;
                let max_label_width = item_size + item_padding;
                let display_name = truncate_text(asset.name.as_str(), max_label_width, ui);

                let label_size = ui.measure_text(
                    &display_name,
                    ui.scaled_font_size(katla_ui::FontSize::XSmall),
                );
                let label_pos = Vec2::new(item_bounds.center().x() - label_size.x() * 0.5, label_y);
                ui.draw_text(
                    &display_name,
                    label_pos,
                    theme.text_secondary,
                    ui.scaled_font_size(katla_ui::FontSize::XSmall),
                );

                if is_hovered && !ui.has_open_popup() && ui.mouse_clicked(mouse_button::LEFT) {
                    clicked_index = Some(i);
                    if asset.asset_type == AssetType::Folder {
                        should_navigate = Some(asset.path.clone());
                    } else if asset.asset_type == AssetType::Model {
                        should_preview_model = Some(asset.path.clone());
                    }
                    drag_start_index = Some(i);
                }

                if is_hovered && !ui.has_open_popup() && ui.mouse_clicked(mouse_button::RIGHT) {
                    right_clicked_index = Some(i);
                    state.selected_index = Some(i);
                }

                if is_hovered && !state.context_menu_open && !state.is_dragging {
                    let tooltip_text = format!(
                        "{}\nType: {}\nPath: {}",
                        asset.name,
                        match asset.asset_type {
                            AssetType::Folder => "Folder",
                            AssetType::Model => "Model",
                            AssetType::Image => "Image",
                            AssetType::Shader => "Shader",
                            AssetType::Material => "Material",
                            AssetType::Font => "Font",
                            AssetType::Unknown => "File",
                        },
                        asset.path.display()
                    );
                    ui.with_z_index(katla_ui::z_index::POPUP, |ui| {
                        ui.tooltip(&tooltip_text);
                    });
                }
            }

            // Marquee selection
            {
                let mouse_in_content = content_bounds.contains(ui.mouse_pos());
                let mouse_down = ui.mouse_down(mouse_button::LEFT);

                if mouse_in_content
                    && !ui.has_open_popup()
                    && ui.mouse_clicked(mouse_button::LEFT)
                    && clicked_index.is_none()
                {
                    state.selection_rect_start = Some(ui.mouse_pos());
                    state.selection_rect_current = Some(ui.mouse_pos());
                    state.is_marquee_selecting = false;
                }

                if let Some(start) = state.selection_rect_start {
                    if mouse_down {
                        state.selection_rect_current = Some(ui.mouse_pos());
                        let current = ui.mouse_pos();
                        let dist = (current - start).length();
                        if dist > state.drag_threshold {
                            state.is_marquee_selecting = true;
                        }
                    }
                }

                if state.is_marquee_selecting {
                    if let (Some(start), Some(current)) =
                        (state.selection_rect_start, state.selection_rect_current)
                    {
                        let sel_rect = rect_from_points(start, current);

                        for (i, _asset) in state.assets.iter().enumerate() {
                            let col = i % col_count;
                            let row = i / col_count;
                            let item_x = bounds.min.x()
                                + item_padding
                                + col as f32 * (item_size + item_padding);
                            let item_y = content_top + row as f32 * row_height - scroll_offset;
                            let item_bounds = Rect2D::from_origin_size(
                                Vec2::new(item_x, item_y),
                                Vec2::new(item_size, item_size),
                            );

                            if item_bounds.min.x() <= sel_rect.max.x()
                                && item_bounds.max.x() >= sel_rect.min.x()
                                && item_bounds.min.y() <= sel_rect.max.y()
                                && item_bounds.max.y() >= sel_rect.min.y()
                            {
                                ui.draw_rect(item_bounds, Color::new(0.3, 0.5, 0.8, 0.4));
                            }
                        }

                        ui.draw_rect(sel_rect, Color::new(0.3, 0.5, 0.8, 0.3));
                        ui.draw_rect_border(
                            sel_rect,
                            Color::new(0.3, 0.5, 0.8, 0.3),
                            Color::new(0.4, 0.6, 0.9, 0.8),
                            1.0,
                        );
                    }
                }

                if state.selection_rect_start.is_some() && ui.mouse_released(mouse_button::LEFT) {
                    if state.is_marquee_selecting {
                        if let (Some(start), Some(current)) =
                            (state.selection_rect_start, state.selection_rect_current)
                        {
                            let sel_rect = rect_from_points(start, current);

                            state.selected_indices.clear();
                            state.selected_index = None;

                            for (i, _asset) in state.assets.iter().enumerate() {
                                let col = i % col_count;
                                let row = i / col_count;
                                let item_x = bounds.min.x()
                                    + item_padding
                                    + col as f32 * (item_size + item_padding);
                                let item_y = content_top + row as f32 * row_height - scroll_offset;
                                let item_bounds = Rect2D::from_origin_size(
                                    Vec2::new(item_x, item_y),
                                    Vec2::new(item_size, item_size),
                                );

                                if item_bounds.min.x() <= sel_rect.max.x()
                                    && item_bounds.max.x() >= sel_rect.min.x()
                                    && item_bounds.min.y() <= sel_rect.max.y()
                                    && item_bounds.max.y() >= sel_rect.min.y()
                                {
                                    state.selected_indices.insert(i);
                                    if state.selected_index.is_none() {
                                        state.selected_index = Some(i);
                                    }
                                }
                            }
                        }
                    } else {
                        state.selected_index = None;
                        state.selected_indices.clear();
                    }
                    state.selection_rect_start = None;
                    state.selection_rect_current = None;
                    state.is_marquee_selecting = false;
                }
            }

            {
                let mut thumbs_to_request: Vec<(usize, PathBuf)> = Vec::new();

                for (i, asset) in state.assets.iter().enumerate() {
                    if asset.asset_type != AssetType::Image {
                        continue;
                    }

                    let row = i / col_count;
                    let item_y = content_top + row as f32 * row_height - scroll_offset;

                    if item_y + row_height < content_top || item_y > bounds.max.y() {
                        continue;
                    }

                    if matches!(asset.thumbnail_state, ThumbnailState::Pending) {
                        if loader.has_thumbnail(&asset.path) {
                        } else if !loader.is_loading(&asset.path) {
                            thumbs_to_request.push((i, asset.path.clone()));
                        }
                    }
                }

                for (idx, path) in thumbs_to_request.into_iter().take(4) {
                    loader.request_thumbnail(path, item_size as u32);
                    state.assets[idx].thumbnail_state = ThumbnailState::Loading;
                }
            }

            if let Some(idx) = drag_start_index {
                state.start_drag(idx, ui.mouse_pos());
            }

            if state.drag_asset.is_some() && ui.mouse_down(mouse_button::LEFT) {
                state.update_drag(ui.mouse_pos());
            }

            if state.drag_asset.is_some() && ui.mouse_released(mouse_button::LEFT) {
                let drag_idx = state.drag_asset.unwrap();
                let mouse_pos = ui.mouse_pos();
                let mouse_in_browser = bounds.contains(mouse_pos);

                if state.is_dragging {
                    let mut assets_to_drag: Vec<(usize, PathBuf, AssetType)> = Vec::new();

                    if !state.selected_indices.is_empty()
                        && state.selected_indices.contains(&drag_idx)
                    {
                        for &idx in &state.selected_indices {
                            if let Some(asset) = state.assets.get(idx) {
                                assets_to_drag.push((idx, asset.path.clone(), asset.asset_type));
                            }
                        }
                    } else {
                        if let Some(asset) = state.assets.get(drag_idx) {
                            assets_to_drag.push((drag_idx, asset.path.clone(), asset.asset_type));
                        }
                    }

                    let mut dropped_on_folder: Option<PathBuf> = None;

                    if mouse_in_browser {
                        for (i, asset) in state.assets.iter().enumerate() {
                            if assets_to_drag.iter().any(|(idx, _, _)| *idx == i) {
                                continue;
                            }

                            let col = i % col_count;
                            let row = i / col_count;
                            let item_x = bounds.min.x()
                                + item_padding
                                + col as f32 * (item_size + item_padding);
                            let item_y = content_top + row as f32 * row_height - scroll_offset;
                            let item_bounds = Rect2D::from_origin_size(
                                Vec2::new(item_x, item_y),
                                Vec2::new(item_size, item_size),
                            );

                            if item_bounds.contains(mouse_pos)
                                && asset.asset_type == AssetType::Folder
                            {
                                dropped_on_folder = Some(asset.path.clone());
                                break;
                            }
                        }
                    }

                    if let Some(folder_path) = dropped_on_folder {
                        for (_, asset_path, _) in &assets_to_drag {
                            state.pending_actions.push(AssetAction::MoveToFolder {
                                asset_path: asset_path.clone(),
                                folder_path: folder_path.clone(),
                            });
                        }
                    } else if !mouse_in_browser {
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

            if state.drag_asset.is_some() && ui.key_pressed(KeyCode::Escape) {
                state.cancel_drag();
            }

            let rename_data = if state.rename_mode {
                if let Some(rename_idx) = state.rename_asset {
                    state
                        .assets
                        .get(rename_idx)
                        .map(|asset| (rename_idx, asset.name.clone(), asset.path.clone()))
                } else {
                    None
                }
            } else {
                None
            };

            if let Some((rename_idx, original_name, original_path)) = rename_data {
                let col = rename_idx % col_count;
                let row = rename_idx / col_count;
                let item_x =
                    bounds.min.x() + item_padding + col as f32 * (item_size + item_padding);
                let item_y = content_top + row as f32 * row_height - scroll_offset;

                let input_bounds = Rect2D::from_origin_size(
                    Vec2::new(item_x, item_y + item_size + 2.0),
                    Vec2::new(item_size, 18.0),
                );

                ui.with_z_index(katla_ui::z_index::POPUP, |ui| {
                    let rename_response = ui.add(
                        katla_ui::widgets::TextInput::new(&mut state.rename_buffer)
                            .bounds(input_bounds),
                    );

                    if is_focused {
                        ui.capture_keyboard();
                    }

                    if rename_response.active && is_focused && ui.key_pressed(KeyCode::Escape) {
                        state.cancel_rename();
                    } else if (is_focused && ui.key_pressed(KeyCode::Enter))
                        || (ui.mouse_clicked(mouse_button::LEFT) && !ui.is_hovered(input_bounds))
                    {
                        let new_name = state.rename_buffer.clone();
                        if new_name != original_name && !new_name.is_empty() {
                            let new_path = original_path.parent().unwrap().join(&new_name);
                            state.pending_actions.push(AssetAction::Rename {
                                old_path: original_path.clone(),
                                new_path,
                            });
                        }
                        state.cancel_rename();
                    }
                });
            }

            if let Some(index) = clicked_index {
                let is_double = ui.mouse_double_clicked(mouse_button::LEFT)
                    && state.last_click_index == Some(index);
                state.last_click_index = Some(index);

                let ctrl_held = ui.key_down(KeyCode::Control);
                let shift_held = ui.key_down(KeyCode::Shift);

                if ctrl_held {
                    if state.selected_indices.contains(&index)
                        || state.selected_index == Some(index)
                    {
                        state.selected_indices.remove(&index);
                        if state.selected_index == Some(index) {
                            state.selected_index = state.selected_indices.iter().next().copied();
                        }
                    } else {
                        if let Some(prev) = state.selected_index {
                            if !state.selected_indices.contains(&prev) {
                                state.selected_indices.insert(prev);
                            }
                        }
                        state.selected_indices.insert(index);
                        state.selected_index = Some(index);
                    }
                } else if shift_held && state.selected_index.is_some() {
                    let start = state.selected_index.unwrap();
                    let end = index;
                    state.selected_indices.clear();
                    for i in start.min(end)..=start.max(end) {
                        if i < state.assets.len() {
                            state.selected_indices.insert(i);
                        }
                    }
                } else {
                    state.selected_index = Some(index);
                    state.selected_indices.clear();
                }

                if is_double && !ctrl_held && !shift_held {
                    if let Some(path) = should_navigate {
                        if path.ends_with("..") {
                            state.navigate_up(thumbnail_texture_handles);
                        } else {
                            state.navigate_to(&path, thumbnail_texture_handles);
                        }
                    } else if let Some(path) = should_preview_model {
                        state
                            .pending_actions
                            .push(AssetAction::ModelPreviewRequested(path));
                    }
                }
            }

            if let Some(index) = right_clicked_index {
                state.context_menu_asset = Some(index);
                state.context_menu_open = true;
            }

            if state.assets.is_empty() {
                let empty_text = if state.search_filter.is_empty() {
                    "No assets found"
                } else {
                    "No matching assets"
                };
                let empty_size =
                    ui.measure_text(empty_text, ui.scaled_font_size(katla_ui::FontSize::Medium));
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

            let total_rows = state.assets.len().div_ceil(col_count);
            total_rows as f32 * row_height
        },
    );

    // Get scroll offset for context menu (from state since scroll_area cleared it)
    let scroll_offset = state.scroll_state.scroll_offset;
    let item_size = ui.style.thumbnail_size;
    let item_padding = 8.0;
    let col_count =
        ((bounds.width() - item_padding) / (item_size + item_padding)).max(1.0) as usize;
    let row_height = item_size + 24.0;

    if !ui.has_open_popup() && ui.is_hovered(content_bounds) {
        let mut clicked_on_asset = false;
        for (i, _asset) in state.assets.iter().enumerate() {
            let col = i % col_count;
            let row = i / col_count;
            let item_x = bounds.min.x() + item_padding + col as f32 * (item_size + item_padding);
            let item_y = content_top + row as f32 * row_height - scroll_offset;
            let item_bounds = Rect2D::from_origin_size(
                Vec2::new(item_x, item_y),
                Vec2::new(item_size, item_size + 16.0),
            );
            if ui.is_hovered(item_bounds) {
                clicked_on_asset = true;
                break;
            }
        }

        if !clicked_on_asset && ui.mouse_clicked(mouse_button::RIGHT) {
            state.context_menu_asset = None;
            state.context_menu_open = true;
        }
    }

    let (asset_type, asset_name, asset_path, asset_idx) =
        if let Some(asset_idx) = state.context_menu_asset {
            if let Some(asset) = state.assets.get(asset_idx) {
                (
                    Some(asset.asset_type),
                    asset.name.clone(),
                    asset.path.clone(),
                    asset_idx,
                )
            } else {
                (None, String::new(), state.current_path.clone(), 0)
            }
        } else {
            (None, String::new(), state.current_path.clone(), 0)
        };

    let open_icon = if asset_type == Some(AssetType::Folder) {
        ForkAwesome::FOLDER_OPEN
    } else {
        ForkAwesome::FILE
    };

    let mut clicked_action: Option<&str> = None;
    ui.context_menu(
        "asset_context",
        &mut state.context_menu_open,
        |ui, open| match asset_type {
            Some(_) => {
                if ui.menu_item_clicked_with_icon_and_shortcut("Open", open_icon, true, "Enter") {
                    clicked_action = Some("Open");
                    *open = false;
                    return;
                }
                if ui.menu_item_clicked_with_icon_and_shortcut(
                    "Rename",
                    ForkAwesome::PENCIL,
                    true,
                    "F2",
                ) {
                    clicked_action = Some("Rename");
                    *open = false;
                    return;
                }
                ui.menu_separator();
                if ui.menu_item_clicked_with_icon_and_shortcut(
                    "Copy Path",
                    ForkAwesome::COPY,
                    true,
                    "",
                ) {
                    clicked_action = Some("Copy Path");
                    *open = false;
                    return;
                }
                if ui.menu_item_clicked_with_icon_and_shortcut(
                    "Show in Explorer",
                    ForkAwesome::EXTERNAL_LINK,
                    true,
                    "",
                ) {
                    clicked_action = Some("Show in Explorer");
                    *open = false;
                    return;
                }
                ui.menu_separator();
                if ui.menu_item_clicked_with_icon_and_shortcut(
                    "Delete",
                    ForkAwesome::TRASH,
                    true,
                    "Del",
                ) {
                    clicked_action = Some("Delete");
                    *open = false;
                }
            }
            None => {
                if ui.menu_item_clicked_with_icon_and_shortcut(
                    "New Folder",
                    ForkAwesome::FOLDER,
                    true,
                    "",
                ) {
                    clicked_action = Some("New Folder");
                    *open = false;
                    return;
                }
                ui.menu_separator();
                if ui.menu_item_clicked_with_icon_and_shortcut(
                    "Refresh",
                    ForkAwesome::REFRESH,
                    true,
                    "F5",
                ) {
                    clicked_action = Some("Refresh");
                    *open = false;
                    return;
                }
                ui.menu_separator();
                if ui.menu_item_clicked_with_icon_and_shortcut(
                    "Show in Explorer",
                    ForkAwesome::EXTERNAL_LINK,
                    true,
                    "",
                ) {
                    clicked_action = Some("Show in Explorer");
                    *open = false;
                }
            }
        },
    );

    if !state.context_menu_open {
        state.context_menu_asset = None;
    }

    if let Some(action) = clicked_action {
        match action {
            "Open" => {
                if asset_type == Some(AssetType::Folder) {
                    if asset_name == ".." {
                        state.navigate_up(thumbnail_texture_handles);
                    } else {
                        state.navigate_to(&asset_path, thumbnail_texture_handles);
                    }
                } else if asset_type.is_some() {
                    state.pending_actions.push(AssetAction::Open(asset_path));
                }
            }
            "Rename" => {
                state.start_rename(asset_idx);
            }
            "Copy Path" => {
                state
                    .pending_actions
                    .push(AssetAction::CopyPath(asset_path));
            }
            "Show in Explorer" => {
                state
                    .pending_actions
                    .push(AssetAction::ShowInExplorer(asset_path));
            }
            "Delete" => {
                let is_folder = asset_path.is_dir();
                let name = asset_path
                    .file_name()
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
                state
                    .pending_actions
                    .push(AssetAction::CreateFolder(asset_path));
            }
            "Refresh" => {
                state.refresh(thumbnail_texture_handles);
            }
            _ => {}
        }
    }

    ui.modal(
        "confirm_dialog",
        320.0,
        120.0,
        &mut state.confirm_dialog_open,
        |ui, open| {
            let dialog_bounds = ui.get_popup_bounds();
            let dialog_pos = dialog_bounds.min;
            let dialog_width = dialog_bounds.width();

            let title_bounds = Rect2D::from_origin_size(dialog_pos, Vec2::new(dialog_width, 28.0));
            ui.draw_rect(title_bounds, theme.panel_header);
            ui.draw_text(
                "Confirm Delete",
                Vec2::new(dialog_pos.x() + 10.0, dialog_pos.y() + 7.0),
                theme.text_primary,
                ui.scaled_font_size(katla_ui::FontSize::Small),
            );

            ui.draw_text(
                &state.confirm_dialog_message,
                Vec2::new(dialog_pos.x() + 10.0, dialog_pos.y() + 40.0),
                theme.text_secondary,
                ui.scaled_font_size(katla_ui::FontSize::Small),
            );

            let btn_width = 80.0;
            let btn_height = 28.0;
            let btn_y = dialog_pos.y() + 120.0 - btn_height - 12.0;

            let no_btn_bounds = Rect2D::from_origin_size(
                Vec2::new(
                    dialog_pos.x() + dialog_width - btn_width * 2.0 - 20.0,
                    btn_y,
                ),
                Vec2::new(btn_width, btn_height),
            );

            if ui
                .add(
                    katla_ui::widgets::Button::new("No")
                        .bounds(no_btn_bounds)
                        .fill_color(katla_math::Color::TRANSPARENT)
                        .hover_color(theme.button_hover)
                        .border(theme.border),
                )
                .clicked
            {
                state.confirm_pending_action = None;
                *open = false;
            }

            let yes_btn_bounds = Rect2D::from_origin_size(
                Vec2::new(dialog_pos.x() + dialog_width - btn_width - 10.0, btn_y),
                Vec2::new(btn_width, btn_height),
            );

            if ui
                .add(
                    katla_ui::widgets::Button::new("Yes")
                        .bounds(yes_btn_bounds)
                        .fill_color(theme.error)
                        .hover_color(theme.error * 1.3)
                        .border(theme.border),
                )
                .clicked
            {
                if let Some(action) = state.confirm_pending_action.take() {
                    state.pending_actions.push(action);
                }
                *open = false;
            }
        },
    );

    if !state.search_focused && !state.context_menu_open && !state.rename_mode {
        use katla_ui::input::KeyCode;

        if ui.key_pressed(KeyCode::ArrowUp) {
            state.handle_keyboard(KeyCode::ArrowUp, thumbnail_texture_handles);
        }
        if ui.key_pressed(KeyCode::ArrowDown) {
            state.handle_keyboard(KeyCode::ArrowDown, thumbnail_texture_handles);
        }
        if ui.key_pressed(KeyCode::ArrowLeft) {
            state.handle_keyboard(KeyCode::ArrowLeft, thumbnail_texture_handles);
        }
        if ui.key_pressed(KeyCode::ArrowRight) {
            state.handle_keyboard(KeyCode::ArrowRight, thumbnail_texture_handles);
        }
        if ui.key_pressed(KeyCode::Enter) {
            if let Some(action) = state.handle_keyboard(KeyCode::Enter, thumbnail_texture_handles) {
                state.pending_actions.push(action);
            }
        }
        if ui.key_pressed(KeyCode::Backspace) {
            state.handle_keyboard(KeyCode::Backspace, thumbnail_texture_handles);
        }
    }
}

/// Truncate text to fit within a maximum width, adding ellipsis if needed.
fn truncate_text(text: &str, max_width: f32, ui: &UiContext) -> String {
    let font_size = ui.scaled_font_size(katla_ui::FontSize::XSmall);
    let full_width = ui.measure_text(text, font_size).x();

    if full_width <= max_width {
        return text.to_string();
    }

    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let truncated = format!("{}...", chars.iter().collect::<String>());
        if ui.measure_text(&truncated, font_size).x() <= max_width {
            return truncated;
        }
    }

    "...".to_string()
}
