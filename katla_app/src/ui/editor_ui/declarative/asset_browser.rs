use std::boxed::Box;
use std::collections::HashMap;
use std::path::PathBuf;

use katla_gfx::TextureHandle;
use katla_math::{Rect2D, Vec2};
use katla_ui::declarative::{
    Build, BuildContext, Padding, StateId, Widget, WidgetBox, button, context_entry, context_menu,
    empty, grid, hstack, icon, image, image_button, modal, panel, scroll, selectable,
    separator_horizontal, text, textfield, vstack,
};
use katla_ui::{FontSize, ForkAwesome, TextureId};

use crate::ui::ColorScheme;
use crate::ui::editor_ui::EditorAction;
use crate::ui::editor_ui::ThumbnailState;
use crate::ui::editor_ui::asset_browser::{AssetAction, AssetBrowserState, AssetType};

/// Environment data injected before each frame for the asset browser panel.
#[derive(Clone)]
pub(crate) struct AssetBrowserDrawCtx {
    pub bounds: Rect2D,
    pub theme: ColorScheme,
    pub assets: Vec<AssetRenderData>,
    pub selected_index: Option<usize>,
    pub path_segments: Vec<String>,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub search_filter: String,
    pub context_menu_open: bool,
    pub context_menu_is_asset: bool,
    pub confirm_dialog_open: bool,
    pub confirm_dialog_message: String,
    pub collapsed: bool,
}

/// Render data for a single asset entry.
#[derive(Clone)]
pub(crate) struct AssetRenderData {
    pub name: String,
    pub path: PathBuf,
    pub asset_type: AssetType,
    pub thumbnail_state: ThumbnailState,
}

/// Actions emitted by the declarative asset browser panel.
#[derive(Clone, Debug)]
pub(crate) enum AssetBrowserAction {
    NavigateToSegment(usize),
    NavigateBack,
    NavigateForward,
    Refresh,
    AssetClicked(usize),
    AssetDoubleClicked {
        path: PathBuf,
        asset_type: AssetType,
    },
    ContextMenuAction {
        action: String,
        asset_index: Option<usize>,
    },
    ConfirmDelete(PathBuf),
    CancelDelete,
}

pub(crate) struct AssetBrowserView;

impl Build for AssetBrowserView {
    fn build(&self, ctx: &mut BuildContext) -> Box<dyn Widget> {
        let draw_ctx = ctx.env::<AssetBrowserDrawCtx>().cloned();
        let Some(draw_ctx) = draw_ctx else {
            return empty().boxed();
        };

        if draw_ctx.collapsed {
            return empty().boxed();
        }

        let search_id: StateId = ctx.state(draw_ctx.search_filter.clone());
        let scroll_id: StateId = ctx.state(0.0f32);
        let context_open_id: StateId = ctx.state(draw_ctx.context_menu_open);
        let confirm_open_id: StateId = ctx.state(draw_ctx.confirm_dialog_open);
        ctx.set_state(confirm_open_id, draw_ctx.confirm_dialog_open);

        // Breadcrumbs
        let mut breadcrumb_items: Vec<Box<dyn Widget>> = Vec::new();
        for (i, segment) in draw_ctx.path_segments.iter().enumerate() {
            if i > 0 {
                breadcrumb_items.push(
                    text(" / ")
                        .color(draw_ctx.theme.text_muted)
                        .font_size(FontSize::Small)
                        .boxed(),
                );
            }
            let is_last = i == draw_ctx.path_segments.len() - 1;
            if is_last {
                breadcrumb_items.push(
                    text(segment)
                        .color(draw_ctx.theme.text_primary)
                        .font_size(FontSize::Small)
                        .boxed(),
                );
            } else {
                let seg_index = i;
                breadcrumb_items.push(
                    button(segment)
                        .fill(katla_math::Color::TRANSPARENT)
                        .border(katla_math::Color::TRANSPARENT)
                        .on_click(ctx.on_click(move |actions| {
                            actions.emit(AssetBrowserAction::NavigateToSegment(seg_index));
                        }))
                        .boxed(),
                );
            }
        }

        // Navigation buttons
        let back_cb = ctx.on_click(|actions| {
            actions.emit(AssetBrowserAction::NavigateBack);
        });
        let forward_cb = ctx.on_click(|actions| {
            actions.emit(AssetBrowserAction::NavigateForward);
        });
        let refresh_cb = ctx.on_click(|actions| {
            actions.emit(AssetBrowserAction::Refresh);
        });

        let toolbar = hstack([
            hstack(breadcrumb_items).spacing(2.0).boxed(),
            textfield("Filter...", search_id).boxed(),
            image_button(ForkAwesome::ARROW_LEFT)
                .enabled(draw_ctx.can_go_back)
                .on_click(back_cb)
                .boxed(),
            image_button(ForkAwesome::ARROW_RIGHT)
                .enabled(draw_ctx.can_go_forward)
                .on_click(forward_cb)
                .boxed(),
            image_button(ForkAwesome::REFRESH)
                .on_click(refresh_cb)
                .boxed(),
        ])
        .spacing(4.0)
        .padding(Padding::all(4.0))
        .boxed();

        // Grid of assets
        let item_size = 80.0;
        let cell_size = Vec2::new(item_size + 4.0, item_size + 24.0);
        let col_count = if draw_ctx.bounds.width() > 0.0 {
            ((draw_ctx.bounds.width() - 8.0) / (item_size + 4.0)).max(1.0) as usize
        } else {
            8
        };

        let mut grid_children = Vec::new();
        for (i, asset) in draw_ctx.assets.iter().enumerate() {
            let is_selected = draw_ctx.selected_index == Some(i);

            let icon_content = match &asset.thumbnail_state {
                ThumbnailState::Loaded { texture_handle } => image(
                    TextureId::from_handle_index(texture_handle.index()),
                    katla_math::Color::WHITE,
                )
                .boxed(),
                ThumbnailState::Loading => icon(ForkAwesome::CIRCLE_OUTLINE)
                    .icon_size(FontSize::Huge)
                    .color(draw_ctx.theme.text_secondary)
                    .boxed(),
                ThumbnailState::Failed => icon(ForkAwesome::TIMES_CIRCLE)
                    .icon_size(FontSize::Huge)
                    .color(draw_ctx.theme.error)
                    .boxed(),
                ThumbnailState::Pending => icon(asset.asset_type.icon())
                    .icon_size(FontSize::Huge)
                    .color(asset.asset_type.color(&draw_ctx.theme))
                    .boxed(),
            };

            let display_name = truncate_name(&asset.name, 12);

            let cell = vstack([
                icon_content,
                text(display_name)
                    .color(draw_ctx.theme.text_secondary)
                    .font_size(FontSize::XSmall)
                    .boxed(),
            ])
            .spacing(2.0)
            .padding_all(2.0)
            .align(katla_ui::declarative::Alignment::Center);

            let click_index = i;
            let click_path = asset.path.clone();
            let click_type = asset.asset_type;

            grid_children.push(
                selectable(cell.boxed())
                    .selected(is_selected)
                    .on_click(ctx.on_click(move |actions| {
                        actions.emit(AssetBrowserAction::AssetClicked(click_index));
                        actions.emit(AssetBrowserAction::AssetDoubleClicked {
                            path: click_path.clone(),
                            asset_type: click_type,
                        });
                    }))
                    .boxed(),
            );
        }

        let grid_content = if grid_children.is_empty() {
            let empty_text = if draw_ctx.search_filter.is_empty() {
                "No assets found"
            } else {
                "No matching assets"
            };
            text(empty_text)
                .color(draw_ctx.theme.text_muted)
                .font_size(FontSize::Small)
                .boxed()
        } else {
            grid(col_count, cell_size, grid_children)
                .grid_spacing(4.0)
                .boxed()
        };

        let content = vstack([
            toolbar,
            separator_horizontal().boxed(),
            scroll(grid_content, scroll_id).boxed(),
        ])
        .boxed();

        // Context menu
        let context_items: Vec<katla_ui::declarative::ContextMenuEntry> =
            if draw_ctx.context_menu_is_asset {
                vec![
                    context_entry("Open").on_click(ctx.on_click(|actions| {
                        actions.emit(AssetBrowserAction::ContextMenuAction {
                            action: "Open".to_string(),
                            asset_index: None, // resolved from state in processor
                        });
                    })),
                    context_entry("Rename").on_click(ctx.on_click(|actions| {
                        actions.emit(AssetBrowserAction::ContextMenuAction {
                            action: "Rename".to_string(),
                            asset_index: None,
                        });
                    })),
                    context_entry("Copy Path").on_click(ctx.on_click(|actions| {
                        actions.emit(AssetBrowserAction::ContextMenuAction {
                            action: "Copy Path".to_string(),
                            asset_index: None,
                        });
                    })),
                    context_entry("Show in Explorer").on_click(ctx.on_click(|actions| {
                        actions.emit(AssetBrowserAction::ContextMenuAction {
                            action: "Show in Explorer".to_string(),
                            asset_index: None,
                        });
                    })),
                    context_entry("Delete").on_click(ctx.on_click(|actions| {
                        actions.emit(AssetBrowserAction::ContextMenuAction {
                            action: "Delete".to_string(),
                            asset_index: None,
                        });
                    })),
                ]
            } else {
                vec![
                    context_entry("New Folder").on_click(ctx.on_click(|actions| {
                        actions.emit(AssetBrowserAction::ContextMenuAction {
                            action: "New Folder".to_string(),
                            asset_index: None,
                        })
                    })),
                    context_entry("Refresh").on_click(ctx.on_click(|actions| {
                        actions.emit(AssetBrowserAction::ContextMenuAction {
                            action: "Refresh".to_string(),
                            asset_index: None,
                        });
                    })),
                    context_entry("Show in Explorer").on_click(ctx.on_click(|actions| {
                        actions.emit(AssetBrowserAction::ContextMenuAction {
                            action: "Show in Explorer".to_string(),
                            asset_index: None,
                        });
                    })),
                ]
            };

        let ctx_menu = context_menu(context_items, context_open_id).boxed();

        // Confirmation modal
        let no_btn = button("No")
            .fill(katla_math::Color::TRANSPARENT)
            .border(katla_math::Color::TRANSPARENT)
            .on_click(ctx.on_click(|actions| {
                actions.emit(AssetBrowserAction::CancelDelete);
            }))
            .boxed();
        let yes_btn = button("Yes")
            .fill(draw_ctx.theme.error.with_alpha(0.3))
            .border(katla_math::Color::TRANSPARENT)
            .on_click(ctx.on_click(|actions| {
                actions.emit(AssetBrowserAction::ConfirmDelete(PathBuf::new()));
            }))
            .boxed();

        let modal_content = vstack([
            text("Confirm Delete")
                .color(draw_ctx.theme.text_primary)
                .boxed(),
            text(&draw_ctx.confirm_dialog_message)
                .color(draw_ctx.theme.text_secondary)
                .boxed(),
            hstack([no_btn, yes_btn]).spacing(8.0).boxed(),
        ])
        .spacing(8.0)
        .padding_all(8.0);

        let confirm_modal = modal(320.0, 120.0, confirm_open_id, modal_content.boxed()).on_close(
            ctx.on_click(|actions| {
                actions.emit(AssetBrowserAction::CancelDelete);
            }),
        );

        panel(
            "Asset Browser",
            vstack([content, ctx_menu, confirm_modal.boxed()]).boxed(),
        )
        .flex_width(draw_ctx.bounds.width())
        .flex_height(draw_ctx.bounds.height())
        .boxed()
    }
}

fn truncate_name(name: &str, max_chars: usize) -> String {
    if name.chars().count() > max_chars {
        format!("{}...", name.chars().take(max_chars).collect::<String>())
    } else {
        name.to_string()
    }
}

/// Process pending asset browser actions into editor actions.
/// Called from layout.rs after view_tree.frame().
pub(crate) fn process_asset_actions(
    state: &mut AssetBrowserState,
    thumbnail_texture_handles: &HashMap<PathBuf, TextureHandle>,
    _viewport_bounds: Rect2D,
) -> Vec<EditorAction> {
    let mut pending_actions = Vec::new();

    for action in state.take_actions() {
        match action {
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
        }
    }

    pending_actions
}

/// Process declarative asset browser actions emitted from the view tree.
/// Maps high-level UI actions to state mutations and editor actions.
pub(crate) fn process_declarative_actions(
    state: &mut AssetBrowserState,
    thumbnail_texture_handles: &HashMap<PathBuf, TextureHandle>,
    viewport_bounds: Rect2D,
    actions: Vec<AssetBrowserAction>,
) -> Vec<EditorAction> {
    let mut pending = Vec::new();

    for action in actions {
        match action {
            AssetBrowserAction::NavigateToSegment(idx) => {
                state.navigate_to_segment(idx, thumbnail_texture_handles);
            }
            AssetBrowserAction::NavigateBack => {
                state.navigate_back(thumbnail_texture_handles);
            }
            AssetBrowserAction::NavigateForward => {
                state.navigate_forward(thumbnail_texture_handles);
            }
            AssetBrowserAction::Refresh => {
                state.refresh(thumbnail_texture_handles);
            }
            AssetBrowserAction::AssetClicked(idx) => {
                state.selected_index = Some(idx);
                state.selected_indices.clear();
            }
            AssetBrowserAction::AssetDoubleClicked {
                asset_type, path, ..
            } => {
                if asset_type == AssetType::Folder {
                    if path.ends_with("..") {
                        state.navigate_up(thumbnail_texture_handles);
                    } else {
                        state.navigate_to(&path, thumbnail_texture_handles);
                    }
                } else if asset_type == AssetType::Model {
                    state
                        .pending_actions
                        .push(AssetAction::ModelPreviewRequested(path));
                } else if asset_type == AssetType::Audio {
                    state
                        .pending_actions
                        .push(AssetAction::AudioPreviewToggle { path });
                }
            }
            AssetBrowserAction::ContextMenuAction {
                action,
                asset_index,
            } => {
                let asset_path = asset_index
                    .and_then(|idx| state.assets.get(idx))
                    .map(|a| a.path.clone())
                    .unwrap_or_else(|| state.current_path.clone());
                let asset_type = asset_index
                    .and_then(|idx| state.assets.get(idx))
                    .map(|a| a.asset_type);

                state.context_menu_open = false;
                state.context_menu_asset = None;

                match action.as_str() {
                    "Open" => {
                        if asset_type == Some(AssetType::Folder) {
                            if asset_path.file_name().map(|n| n == "..").unwrap_or(false) {
                                state.navigate_up(thumbnail_texture_handles);
                            } else {
                                state.navigate_to(&asset_path, thumbnail_texture_handles);
                            }
                        } else if asset_type.is_some() {
                            state.pending_actions.push(AssetAction::Open(asset_path));
                        }
                    }
                    "Rename" => {
                        if let Some(idx) = asset_index {
                            state.start_rename(idx);
                        }
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
            AssetBrowserAction::ConfirmDelete(path) => {
                state.confirm_dialog_open = false;
                state.pending_actions.push(AssetAction::Delete(path));
            }
            AssetBrowserAction::CancelDelete => {
                state.confirm_dialog_open = false;
                state.confirm_pending_action = None;
            }
        }
    }

    // Process any pending AssetActions that were queued by the action handlers
    pending.extend(process_asset_actions(
        state,
        thumbnail_texture_handles,
        viewport_bounds,
    ));

    pending
}
