use katla_ecs::EntityId;
use katla_math::{Rect2D, Vec2, Vec3};
use katla_ui::{
    FontSize, UiContext,
    widgets::{RadioButton, ResizeHandle},
};

use super::{
    EditorAction, EditorRenderParams, EditorUI, SpawnableModel,
    asset_browser::{AssetAction, AssetType, build_asset_browser},
    co_creator, hierarchy, inspector,
    preferences::PreferencesPanel,
    status_bar, toolbar, viewport_grid,
};
use crate::ui::ParticleInspector;

impl EditorUI {
    /// Build the editor UI.
    pub(super) fn build(&mut self, ui: &mut UiContext, params: &mut EditorRenderParams) {
        let entities = params.entities;
        let fps = params.fps;
        let frame_count = params.frame_count;
        let loader = &mut *params.loader;
        let thumbnail_texture_handles = params.thumbnail_texture_handles;
        let screen_size = ui.screen_size();
        let preferences = params.preferences;

        // Render floating panels first so they register hover layers and consume
        // scroll before background panels. Visual order is handled by z-index
        // sorting in the draw list, not render order.
        if self.preferences_panel_state.panel.is_visible() {
            // Sync LLM config snapshot into panel state each frame
            self.preferences_panel_state.llm_config = params.llm_config.clone();

            let theme_key = self.theme_key();
            let mut actions = Vec::new();
            ui.add(PreferencesPanel::new(
                screen_size,
                &mut self.preferences_panel_state,
                preferences,
                &self.editor_settings,
                &self.theme,
                theme_key,
                &mut actions,
            ));

            for action in actions {
                self.apply_preferences_action(action);
            }
        }

        if self.particle_inspector_state.panel.is_visible() {
            let mut actions = Vec::new();
            ui.add(ParticleInspector::new(
                &mut self.particle_inspector_state,
                &mut self.selected_particle_emitter,
                &self.theme,
                &self.particle_inspector_data,
                &mut actions,
            ));

            for action in actions {
                self.apply_particle_inspector_action(action);
            }
        }

        // Co-Creator chat panel
        if self.co_creator.is_open() {
            let style = co_creator::CoCreatorStyle::from_theme(&self.theme);
            let response = co_creator::draw_co_creator_panel(
                ui,
                &mut self.co_creator,
                &style,
                screen_size,
                params.agent_undo_count,
            );
            if let Some(text) = response.submitted_text {
                self.pending_actions
                    .push(EditorAction::CoCreatorRequest(text));
            }
            if response.undo_clicked {
                self.pending_actions.push(EditorAction::AgentUndo);
            }
        }

        let visible_entities: Vec<EntityId> = entities
            .iter()
            .filter(|e| {
                hierarchy::is_entity_visible(e, entities, &self.hierarchy_state.expanded_entities)
            })
            .map(|e| e.id)
            .collect();

        if ui.key_pressed(katla_ui::input::KeyCode::Delete)
            && let Some(entity_id) = self.selected_entity
            && entities.iter().any(|e| e.id == entity_id)
        {
            self.pending_actions
                .push(EditorAction::DeleteEntity(entity_id));
            self.selected_entity = None;
        }

        if ui.key_pressed(katla_ui::input::KeyCode::ArrowUp) {
            if let Some(current_id) = self.selected_entity {
                if let Some(pos) = visible_entities.iter().position(|id| *id == current_id)
                    && pos > 0
                {
                    self.selected_entity = Some(visible_entities[pos - 1]);
                }
            } else if !visible_entities.is_empty() {
                self.selected_entity = Some(*visible_entities.last().unwrap());
            }
        }

        if ui.key_pressed(katla_ui::input::KeyCode::ArrowDown) {
            if let Some(current_id) = self.selected_entity {
                if let Some(pos) = visible_entities.iter().position(|id| *id == current_id)
                    && pos < visible_entities.len() - 1
                {
                    self.selected_entity = Some(visible_entities[pos + 1]);
                }
            } else if !visible_entities.is_empty() {
                self.selected_entity = Some(visible_entities[0]);
            }
        }

        if ui.key_pressed(katla_ui::input::KeyCode::ArrowRight)
            && let Some(entity_id) = self.selected_entity
            && !self.hierarchy_state.expanded_entities.contains(&entity_id)
        {
            self.hierarchy_state.expanded_entities.insert(entity_id);
        }

        if ui.key_pressed(katla_ui::input::KeyCode::ArrowLeft)
            && let Some(entity_id) = self.selected_entity
        {
            if self.hierarchy_state.expanded_entities.contains(&entity_id) {
                self.hierarchy_state.expanded_entities.remove(&entity_id);
            } else if let Some(entity) = entities.iter().find(|e| e.id == entity_id)
                && let Some(parent_id) = entity.parent_id
            {
                self.selected_entity = Some(parent_id);
            }
        }

        if ui.key_pressed(katla_ui::input::KeyCode::Escape) {
            self.selected_entity = None;
        }

        let toolbar_height = 32.0;
        let status_bar_height = 24.0;

        let asset_browser_height = if self.asset_browser.collapsed {
            28.0
        } else {
            self.asset_browser.panel_height
        };

        self.toolbar_state.undo_count = params.undo_count;
        self.toolbar_state.redo_count = params.redo_count;

        ui.add(toolbar::Toolbar::new(
            screen_size,
            toolbar_height,
            &mut self.toolbar_state,
            &self.theme,
            preferences,
        ));
        self.pending_actions
            .append(&mut self.toolbar_state.pending_actions);

        let panel_top = toolbar_height;
        let panel_bottom = screen_size.y() - status_bar_height - asset_browser_height;
        let panel_height = panel_bottom - panel_top;

        let resize_handle_width = 5.0;
        let min_panel_width = 150.0;
        let min_viewport_width = 200.0;
        let min_asset_browser_height = 100.0;

        let left_resize_bounds = Rect2D::from_origin_size(
            Vec2::new(self.left_panel_width - resize_handle_width / 2.0, panel_top),
            Vec2::new(resize_handle_width, panel_height),
        );

        let right_panel_x = screen_size.x() - self.right_panel_width;
        let right_resize_bounds = Rect2D::from_origin_size(
            Vec2::new(right_panel_x - resize_handle_width / 2.0, panel_top),
            Vec2::new(resize_handle_width, panel_height),
        );

        let asset_resize_bounds = Rect2D::from_origin_size(
            Vec2::new(0.0, panel_bottom - resize_handle_width / 2.0),
            Vec2::new(screen_size.x(), resize_handle_width),
        );

        let max_left_width =
            (screen_size.x() - self.right_panel_width - min_viewport_width).max(min_panel_width);
        self.left_panel_width = ResizeHandle::horizontal(left_resize_bounds, self.left_panel_width)
            .min_value(min_panel_width)
            .max_value(max_left_width)
            .show(ui);

        let max_right_width =
            (screen_size.x() - self.left_panel_width - min_viewport_width).max(min_panel_width);
        self.right_panel_width =
            ResizeHandle::horizontal(right_resize_bounds, self.right_panel_width)
                .min_value(min_panel_width)
                .max_value(max_right_width)
                .show(ui);

        if !self.asset_browser.collapsed {
            let max_height =
                (screen_size.y() - status_bar_height - toolbar_height - min_viewport_width)
                    .max(min_asset_browser_height);
            self.asset_browser.panel_height =
                ResizeHandle::vertical(asset_resize_bounds, self.asset_browser.panel_height)
                    .min_value(min_asset_browser_height)
                    .max_value(max_height)
                    .show(ui);
        }

        let left_panel_bounds = Rect2D::from_origin_size(
            Vec2::new(0.0, panel_top),
            Vec2::new(self.left_panel_width, panel_height),
        );
        ui.register_panel(1, left_panel_bounds);
        ui.add(hierarchy::Hierarchy::new(
            left_panel_bounds,
            &mut self.hierarchy_state,
            &mut self.selected_entity,
            entities,
            &mut self.pending_actions,
            &self.theme,
        ));

        ui.draw_rect(
            Rect2D::from_origin_size(
                Vec2::new(self.left_panel_width, panel_top),
                Vec2::new(1.0, panel_height),
            ),
            ui.style().separator,
        );

        let right_panel_bounds = Rect2D::from_origin_size(
            Vec2::new(right_panel_x, panel_top),
            Vec2::new(self.right_panel_width, panel_height),
        );
        ui.register_panel(2, right_panel_bounds);
        ui.add(inspector::Inspector::new(
            right_panel_bounds,
            &mut self.selected_entity,
            entities,
            &mut self.pending_actions,
            &self.theme,
            &mut self.inspector_edit,
            &mut self.inspector_scroll_state,
        ));

        ui.draw_rect(
            Rect2D::from_origin_size(
                Vec2::new(right_panel_x - 1.0, panel_top),
                Vec2::new(1.0, panel_height),
            ),
            ui.style().separator,
        );

        let viewport_bounds = Rect2D::new(
            Vec2::new(self.left_panel_width + 1.0, panel_top),
            Vec2::new(right_panel_x - 1.0, panel_bottom),
        );

        self.last_viewport_size = (
            viewport_bounds.width().max(1.0) as u32,
            viewport_bounds.height().max(1.0) as u32,
        );
        self.last_viewport_bounds = viewport_bounds;
        self.last_screen_size = screen_size;

        ui.register_panel(3, viewport_bounds);
        let grid_response = ui.add(viewport_grid::ViewportGrid::new(
            viewport_bounds,
            &self.viewport_grid_state,
            &self.viewport_texture_ids,
            &self.theme,
        ));

        if grid_response.hovered {
            let min = viewport_bounds.min;
            let max = viewport_bounds.max;
            crate::input::update_active_viewport(
                &mut self.viewport_grid_state,
                ui.mouse_pos(),
                min,
                max,
            );
        }

        // Gizmo mode buttons (positioned inside viewport, top-left)
        {
            let gizmo_modes: &[(usize, &str)] = &[(0, "W:Move"), (1, "E:Rotate"), (2, "R:Scale")];
            let gizmo_button_width = 85.0;
            let gizmo_button_height = 22.0;
            let gizmo_padding = 8.0;
            let gizmo_start_x = viewport_bounds.min.x() + gizmo_padding;
            let gizmo_start_y = viewport_bounds.min.y() + gizmo_padding + 16.0;

            let mut selected = self.gizmo_mode as usize;

            for &(index, label) in gizmo_modes {
                let btn_x = gizmo_start_x + index as f32 * (gizmo_button_width + 2.0);
                let btn_bounds = Rect2D::from_origin_size(
                    Vec2::new(btn_x, gizmo_start_y),
                    Vec2::new(gizmo_button_width, gizmo_button_height),
                );

                if ui
                    .add(
                        RadioButton::new(&mut selected, index, label)
                            .bounds(btn_bounds)
                            .id(&format!("gizmo_{label}")),
                    )
                    .changed
                {
                    self.pending_actions
                        .push(EditorAction::SetGizmoMode(selected as u8));
                }
            }
        }

        let asset_browser_bounds = Rect2D::from_origin_size(
            Vec2::new(0.0, panel_bottom),
            Vec2::new(screen_size.x(), asset_browser_height),
        );
        ui.register_panel(4, asset_browser_bounds);
        build_asset_browser(
            &mut self.asset_browser,
            ui,
            &self.theme,
            asset_browser_bounds,
            self.focused_panel == super::FocusedPanel::AssetBrowser,
            loader,
            thumbnail_texture_handles,
        );

        for action in self.asset_browser.take_actions() {
            match action {
                AssetAction::DragToViewport {
                    path,
                    asset_type,
                    screen_pos,
                } => {
                    if viewport_bounds.contains(screen_pos) {
                        match asset_type {
                            AssetType::Model => {
                                self.pending_actions
                                    .push(EditorAction::SpawnModelAtPath { path, screen_pos });
                            }
                            _ => {
                                self.pending_actions.push(EditorAction::SpawnModel(
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
                        self.asset_browser.scan_directory(thumbnail_texture_handles);
                    }
                }
                AssetAction::Delete(path) => {
                    if path.is_dir() {
                        if let Err(e) = std::fs::remove_dir_all(&path) {
                            log::warn!("Failed to delete folder: {}", e);
                        } else {
                            log::info!("Deleted folder: {:?}", path);
                            self.asset_browser.scan_directory(thumbnail_texture_handles);
                        }
                    } else if let Err(e) = std::fs::remove_file(&path) {
                        log::warn!("Failed to delete file: {}", e);
                    } else {
                        log::info!("Deleted file: {:?}", path);
                        self.asset_browser.scan_directory(thumbnail_texture_handles);
                    }
                }
                AssetAction::Rename { old_path, new_path } => {
                    if old_path != new_path {
                        if let Err(e) = std::fs::rename(&old_path, &new_path) {
                            log::warn!("Failed to rename {:?} to {:?}: {}", old_path, new_path, e);
                        } else {
                            log::info!("Renamed {:?} to {:?}", old_path, new_path);
                            self.asset_browser.scan_directory(thumbnail_texture_handles);
                        }
                    }
                }
                AssetAction::Open(path) => {
                    if path.is_dir() {
                        self.asset_browser
                            .navigate_to(&path, thumbnail_texture_handles);
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
                            self.asset_browser.scan_directory(thumbnail_texture_handles);
                        }
                    }
                }
            }
        }

        ui.draw_rect(
            Rect2D::from_origin_size(
                Vec2::new(0.0, panel_bottom),
                Vec2::new(screen_size.x(), 1.0),
            ),
            ui.style().separator,
        );

        let selected_count = if self.asset_browser.selected_indices.is_empty() {
            if self.asset_browser.selected_index.is_some() {
                1
            } else {
                0
            }
        } else {
            self.asset_browser.selected_indices.len()
        };
        let total_assets = self.asset_browser.assets.len();
        ui.add(status_bar::StatusBar::new(status_bar::StatusBarConfig {
            screen_size,
            height: status_bar_height,
            fps,
            frame_count,
            entity_count: entities.len(),
            selected_count,
            total_assets,
            is_playing: self.is_playing,
            theme: &self.theme,
            save_confirmation_timer: self.save_confirmation_timer,
        }));

        if let Some(panel_id) = ui.focused_panel() {
            self.focused_panel = match panel_id {
                1 => super::FocusedPanel::Hierarchy,
                2 => super::FocusedPanel::Inspector,
                3 => super::FocusedPanel::Viewport,
                4 => super::FocusedPanel::AssetBrowser,
                _ => self.focused_panel,
            };
        }

        if self.asset_browser.is_dragging
            && let Some(drag_idx) = self.asset_browser.drag_asset
            && let Some(asset) = self.asset_browser.assets.get(drag_idx)
        {
            let mouse_pos = ui.mouse_pos();

            let preview_size = 64.0;
            let preview_offset = Vec2::new(preview_size * 0.5, preview_size * 0.5);

            ui.with_z_index(katla_ui::z_index::TOOLTIP, |ui| {
                let preview_bounds = Rect2D::from_origin_size(
                    mouse_pos - preview_offset,
                    Vec2::new(preview_size, preview_size),
                );
                ui.draw_rect(preview_bounds, self.theme.background.with_alpha(0.9));
                ui.draw_rect_border(
                    preview_bounds,
                    self.theme.background.with_alpha(0.9),
                    self.theme.highlight,
                    2.0,
                );

                let icon_char = asset.asset_type.icon();
                let icon_size = preview_size * 0.4;
                ui.draw_icon(
                    icon_char,
                    Vec2::new(
                        preview_bounds.center().x() - icon_size * 0.5,
                        preview_bounds.center().y() - icon_size * 0.5 - 8.0,
                    ),
                    icon_size,
                    self.theme.highlight,
                );

                let max_chars = 12;
                let display_name = if asset.name.len() > max_chars {
                    format!("{}...", &asset.name[..max_chars])
                } else {
                    asset.name.clone()
                };
                ui.draw_text(
                    &display_name,
                    Vec2::new(
                        preview_bounds.min.x() + 4.0,
                        preview_bounds.min.y() + preview_size - 16.0,
                    ),
                    self.theme.text_primary,
                    ui.scaled_font_size(FontSize::XSmall),
                );
            });
        }
    }
}
