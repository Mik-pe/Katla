use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

use katla_gfx::TextureHandle;
use katla_math::{Rect2D, Vec3};
use katla_ui::UiContext;
use katla_ui::declarative::{Build, BuildContext, ViewDescriptor};

use crate::ui::ColorScheme;
use crate::ui::editor_ui::asset_browser::{
    AssetAction, AssetBrowserState, AssetType, build_asset_browser,
};
use crate::ui::editor_ui::{EditorAction, SpawnableModel};

thread_local! {
    static ASSET_BROWSER_CTX: RefCell<Option<AssetBrowserDrawCtx>> = const { RefCell::new(None) };
}

struct AssetBrowserDrawCtx {
    bounds: Rect2D,
    theme: ColorScheme,
    is_focused: bool,
    viewport_bounds: Rect2D,
}

pub(crate) fn set_asset_browser_ctx(
    bounds: Rect2D,
    theme: ColorScheme,
    is_focused: bool,
    viewport_bounds: Rect2D,
) {
    ASSET_BROWSER_CTX.with(|c| {
        *c.borrow_mut() = Some(AssetBrowserDrawCtx {
            bounds,
            theme,
            is_focused,
            viewport_bounds,
        })
    });
}

pub(crate) struct AssetBrowserView;

impl Build for AssetBrowserView {
    fn build(&self, _ctx: &mut BuildContext) -> ViewDescriptor {
        ViewDescriptor::Custom(draw_asset_browser_placeholder)
    }
}

fn draw_asset_browser_placeholder(_ui: &mut UiContext, _bounds: Rect2D) {
    // The actual asset browser drawing happens in layout.rs via build_asset_browser_from_ctx()
    // because it needs &mut BackgroundLoader and thumbnail handles that can't go through
    // thread-local storage. This Custom node ensures the asset browser participates in the
    // declarative ViewTree.
}

/// Build the asset browser using the context set before the view tree frame.
/// Called from layout.rs after view_tree.frame() with access to the loader and thumbnails.
pub(crate) fn build_asset_browser_from_ctx(
    state: &mut AssetBrowserState,
    ui: &mut UiContext,
    loader: &mut crate::util::BackgroundLoader,
    thumbnail_texture_handles: &HashMap<PathBuf, TextureHandle>,
) -> Vec<EditorAction> {
    let ctx = ASSET_BROWSER_CTX.with(|c| c.borrow_mut().take());
    let Some(ctx) = ctx else {
        return Vec::new();
    };

    build_asset_browser(
        state,
        ui,
        &ctx.theme,
        ctx.bounds,
        ctx.is_focused,
        loader,
        thumbnail_texture_handles,
    );

    let viewport_bounds = ctx.viewport_bounds;
    let mut pending_actions = Vec::new();

    for action in state.take_actions() {
        match action {
            AssetAction::DragToViewport {
                path,
                asset_type,
                screen_pos,
            } => {
                if viewport_bounds.contains(screen_pos) {
                    match asset_type {
                        AssetType::Model => {
                            pending_actions
                                .push(EditorAction::SpawnModelAtPath { path, screen_pos });
                        }
                        AssetType::Audio => {
                            pending_actions
                                .push(EditorAction::SpawnAudioEmitter { path, screen_pos });
                        }
                        _ => {
                            pending_actions.push(EditorAction::SpawnModel(
                                SpawnableModel::Cube,
                                Vec3::new(0.0, 0.0, 0.0),
                            ));
                        }
                    }
                }
            }
            AssetAction::ModelPreviewRequested(_path) => {
                log::debug!("Model preview requested but feature is disabled");
            }
            AssetAction::AudioPreviewToggle { path } => {
                pending_actions.push(EditorAction::AudioPreviewToggle { path });
            }
            AssetAction::CreateFolder(parent_path) => {
                let mut new_folder = parent_path.join("New Folder");
                let mut counter = 1;
                while new_folder.exists() {
                    new_folder = parent_path.join(format!("New Folder {}", counter));
                    counter += 1;
                }
                if let Err(e) = std::fs::create_dir(&new_folder) {
                    log::warn!("Failed to create folder: {}", e);
                } else {
                    log::info!("Created folder: {:?}", new_folder);
                    state.scan_directory(thumbnail_texture_handles);
                }
            }
            AssetAction::Delete(path) => {
                if path.is_dir() {
                    if let Err(e) = std::fs::remove_dir_all(&path) {
                        log::warn!("Failed to delete folder: {}", e);
                    } else {
                        log::info!("Deleted folder: {:?}", path);
                        state.scan_directory(thumbnail_texture_handles);
                    }
                } else if let Err(e) = std::fs::remove_file(&path) {
                    log::warn!("Failed to delete file: {}", e);
                } else {
                    log::info!("Deleted file: {:?}", path);
                    state.scan_directory(thumbnail_texture_handles);
                }
            }
            AssetAction::Rename { old_path, new_path } => {
                if old_path != new_path {
                    if let Err(e) = std::fs::rename(&old_path, &new_path) {
                        log::warn!("Failed to rename {:?} to {:?}: {}", old_path, new_path, e);
                    } else {
                        log::info!("Renamed {:?} to {:?}", old_path, new_path);
                        state.scan_directory(thumbnail_texture_handles);
                    }
                }
            }
            AssetAction::Open(path) => {
                if path.is_dir() {
                    state.navigate_to(&path, thumbnail_texture_handles);
                } else {
                    log::info!("Open file: {:?}", path);
                }
            }
            AssetAction::CopyPath(path) => {
                log::info!("Copy path: {:?}", path);
            }
            AssetAction::ShowInExplorer(path) => {
                #[cfg(target_os = "windows")]
                {
                    if let Err(e) = std::process::Command::new("explorer")
                        .args(["/select,", &path.to_string_lossy()])
                        .spawn()
                    {
                        log::warn!("Failed to open explorer: {}", e);
                    }
                }
                #[cfg(target_os = "macos")]
                {
                    if let Err(e) = std::process::Command::new("open")
                        .args(["-R", &path.to_string_lossy()])
                        .spawn()
                    {
                        log::warn!("Failed to open finder: {}", e);
                    }
                }
                #[cfg(target_os = "linux")]
                {
                    if let Err(e) = std::process::Command::new("xdg-open")
                        .arg(path.parent().unwrap_or(&path))
                        .spawn()
                    {
                        log::warn!("Failed to open file manager: {}", e);
                    }
                }
            }
            AssetAction::MoveToFolder {
                asset_path,
                folder_path,
            } => {
                let file_name = asset_path.file_name().unwrap_or_default();
                let dest_path = folder_path.join(file_name);
                if asset_path != dest_path {
                    if let Err(e) = std::fs::rename(&asset_path, &dest_path) {
                        log::warn!("Failed to move {:?} to {:?}: {}", asset_path, dest_path, e);
                    } else {
                        log::info!("Moved {:?} to {:?}", asset_path, dest_path);
                        state.scan_directory(thumbnail_texture_handles);
                    }
                }
            }
        }
    }

    pending_actions
}
