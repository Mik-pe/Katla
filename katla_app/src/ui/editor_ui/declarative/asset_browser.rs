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
        let search_filter: String = ctx
            .get_state(search_id)
            .unwrap_or_else(|| draw_ctx.search_filter.clone());
        let search_lower = search_filter.to_lowercase();

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
        let cell_size = Vec2::new(item_size + 16.0, item_size + 32.0);
        let col_count = if draw_ctx.bounds.width() > 0.0 {
            ((draw_ctx.bounds.width() - 8.0) / (item_size + 16.0)).max(1.0) as usize
        } else {
            8
        };

        let mut grid_children = Vec::new();
        for (i, asset) in draw_ctx.assets.iter().enumerate().filter(|(_, asset)| {
            search_lower.is_empty() || asset.name.to_lowercase().contains(&search_lower)
        }) {
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
                    .font_size(FontSize::Small)
                    .boxed(),
            ])
            .spacing(8.0)
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
            let empty_text = if search_filter.is_empty() {
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
                .grid_spacing(16.0)
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
            other => pending_actions.push(other.into()),
        }
    }

    state.process_thumbnail_results(thumbnail_texture_handles);
    pending_actions
}
