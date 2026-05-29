use katla_ecs::EntityId;
use katla_math::{Rect2D, Vec2, Vec3};
use katla_ui::{
    FontSize, ForkAwesome, UiContext, mouse_button,
    widgets::{DockArea, ResizeHandle},
};

use super::declarative::{
    AssetBrowserDrawCtx, ConsoleDrawCtx, EditorRootView, GizmoDrawCtx, GizmoModeChanged,
    HierarchyDrawCtx, InspectorDrawCtx, ParticleInspectorDrawCtx, ParticleInspectorPanelSync,
    PreferencesDrawCtx, PreferencesPanelSync, StatusBarData, ToolbarAction, ToolbarDrawCtx,
    ViewportGridDrawCtx, build_asset_browser_from_ctx,
};
use super::{
    EditorAction, EditorRenderParams, EditorUI, co_creator,
    types::{self as editor_types, BottomPanelTab, PreferencesAction},
};

const BOTTOM_TAB_HEIGHT: f32 = 28.0;

/// Draw the bottom panel tab bar and return the content bounds below it.
fn draw_bottom_tab_bar(
    ui: &mut UiContext,
    bottom_bounds: Rect2D,
    active_tab: BottomPanelTab,
    theme: &katla_ui::ColorScheme,
    collapsed: bool,
) -> (Rect2D, Option<BottomPanelTab>) {
    let tab_bar_bounds = Rect2D::from_origin_size(
        bottom_bounds.min,
        Vec2::new(bottom_bounds.width(), BOTTOM_TAB_HEIGHT),
    );
    ui.draw_rect(tab_bar_bounds, theme.panel_header);

    // Bottom border on tab bar
    ui.draw_line(
        Vec2::new(tab_bar_bounds.min.x(), tab_bar_bounds.max.y()),
        Vec2::new(tab_bar_bounds.max.x(), tab_bar_bounds.max.y()),
        theme.border,
        1.0,
    );

    let mut new_tab = None;
    let padding = ui.style().panel_padding;
    let spacing = ui.style().item_inner_spacing;
    let font_size = ui.scaled_font_size(FontSize::Medium);
    let tab_height = 24.0;

    // Collapse toggle (left side)
    let toggle_size = 20.0;
    let toggle_bounds = Rect2D::from_origin_size(
        Vec2::new(
            bottom_bounds.min.x() + spacing,
            bottom_bounds.min.y() + (BOTTOM_TAB_HEIGHT - toggle_size) * 0.5,
        ),
        Vec2::new(toggle_size, toggle_size),
    );
    let toggle_icon = if collapsed {
        ForkAwesome::CHEVRON_UP
    } else {
        ForkAwesome::CHEVRON_DOWN
    };
    if ui
        .add(katla_ui::widgets::ImageButton::new(toggle_icon).bounds(toggle_bounds))
        .clicked
    {
        // Collapse/expand handled by caller via AssetBrowserState
    }

    // Tab buttons after the toggle
    let mut x = toggle_bounds.max.x() + spacing;
    for tab in BottomPanelTab::all() {
        let label = tab.label();
        let label_size = ui.measure_text(label, font_size);
        let tab_w = label_size.x() + padding * 3.0;
        let tab_bounds = Rect2D::from_origin_size(
            Vec2::new(
                x,
                bottom_bounds.min.y() + (BOTTOM_TAB_HEIGHT - tab_height) * 0.5,
            ),
            Vec2::new(tab_w, tab_height),
        );

        let is_active = *tab == active_tab;
        if is_active {
            ui.draw_rect(tab_bounds, theme.panel_bg);
            // Active tab highlight bar at bottom
            ui.draw_rect(
                Rect2D::from_origin_size(
                    Vec2::new(tab_bounds.min.x(), tab_bounds.max.y() - 2.0),
                    Vec2::new(tab_bounds.width(), 2.0),
                ),
                theme.highlight,
            );
        }

        let text_color = if is_active {
            theme.text_primary
        } else {
            theme.text_secondary
        };

        ui.draw_text(
            label,
            Vec2::new(
                tab_bounds.min.x() + padding,
                tab_bounds.min.y() + (tab_height - label_size.y()) * 0.5,
            ),
            text_color,
            font_size,
        );

        if ui.is_hovered(tab_bounds) && ui.mouse_clicked(mouse_button::LEFT) {
            new_tab = Some(*tab);
        }

        x += tab_w + spacing;
    }

    // Content area below the tab bar
    let content_bounds = if collapsed {
        Rect2D::from_origin_size(bottom_bounds.min, Vec2::new(bottom_bounds.width(), 0.0))
    } else {
        Rect2D::from_origin_size(
            Vec2::new(
                bottom_bounds.min.x(),
                bottom_bounds.min.y() + BOTTOM_TAB_HEIGHT,
            ),
            Vec2::new(
                bottom_bounds.width(),
                bottom_bounds.height() - BOTTOM_TAB_HEIGHT,
            ),
        )
    };

    (content_bounds, new_tab)
}

impl EditorUI {
    /// Build the editor UI.
    pub(super) fn build(&mut self, ui: &mut UiContext, params: &mut EditorRenderParams) {
        let screen_size = ui.screen_size();

        let status_bar_height = 22.0;
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

        let status_data = StatusBarData {
            height: status_bar_height,
            fps: params.fps,
            frame_time_ms: params.frame_time_ms,
            frame_count: params.frame_count,
            entity_count: params.entities.len(),
            draw_call_count: self.last_draw_call_count,
            selected_count,
            total_assets,
            is_playing: self.is_playing,
            theme: self.theme.clone(),
            save_confirmation_timer: self.save_confirmation_timer,
        };
        self.view_tree.env_mut().set(status_data);

        let toolbar_height = 36.0;
        self.view_tree.env_mut().set(ToolbarDrawCtx {
            show_grid: params.preferences.show_grid,
            show_stats: params.preferences.show_stats,
            show_physics_debug: params.preferences.show_physics_debug,
            text_muted: self.theme.text_muted,
            is_playing: self.is_playing,
            is_paused: self.is_paused,
            highlight: self.theme.highlight,
            success: self.theme.success,
            warning: self.theme.warning,
        });
        self.view_tree.env_mut().set(GizmoDrawCtx {
            gizmo_mode: self.gizmo_mode,
        });

        self.view_tree.env_mut().set(ViewportGridDrawCtx {
            bounds: self.last_viewport_bounds,
            state: self.viewport_grid_state.clone(),
            texture_ids: self.viewport_texture_ids,
            theme: self.theme.clone(),
            mouse_pos: ui.mouse_pos(),
        });

        let right_panel_x = screen_size.x() - self.right_panel_width;

        // Compute bottom panel height early so side panels don't overlap it
        let bottom_panel_height = if self.asset_browser.collapsed {
            BOTTOM_TAB_HEIGHT
        } else {
            self.asset_browser.panel_height
        };
        let panel_top = toolbar_height;
        let panel_bottom = screen_size.y() - status_bar_height - bottom_panel_height;
        let panel_height = panel_bottom - panel_top;

        let _left_panel_bounds_for_hierarchy = Rect2D::from_origin_size(
            Vec2::new(0.0, toolbar_height),
            Vec2::new(self.left_panel_width, panel_height),
        );
        self.view_tree.env_mut().set(HierarchyDrawCtx {
            entities: params.entities.to_vec(),
            hierarchy_state: std::mem::take(&mut self.hierarchy_state),
            theme: self.theme.clone(),
            search_filter: self.hierarchy_search_filter.clone(),
        });

        self.view_tree.env_mut().set(InspectorDrawCtx {
            selected_entity: self.selected_entity,
            entities: params.entities.to_vec(),
            edit: std::mem::take(&mut self.inspector_edit),
            theme: self.theme.clone(),
            available_components: self.available_components.clone(),
            add_component_open: self.add_component_open,
            add_component_filter: self.add_component_filter.clone(),
            focus_script_input: self.focus_script_input,
        });

        let resize_handle_width = 5.0;
        let min_panel_width = 150.0;
        let min_viewport_width = 200.0;
        let min_asset_browser_height = 100.0;

        let left_resize_bounds = Rect2D::from_origin_size(
            Vec2::new(self.left_panel_width - resize_handle_width / 2.0, panel_top),
            Vec2::new(resize_handle_width, panel_height),
        );

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
                .inverted()
                .min_value(min_panel_width)
                .max_value(max_right_width)
                .show(ui);

        if !self.asset_browser.collapsed {
            let max_height =
                (screen_size.y() - status_bar_height - toolbar_height - min_viewport_width)
                    .max(min_asset_browser_height);
            self.asset_browser.panel_height =
                ResizeHandle::vertical(asset_resize_bounds, self.asset_browser.panel_height)
                    .inverted()
                    .min_value(min_asset_browser_height + BOTTOM_TAB_HEIGHT)
                    .max_value(max_height)
                    .show(ui);
        }

        let left_panel_bounds = Rect2D::from_origin_size(
            Vec2::new(0.0, panel_top),
            Vec2::new(self.left_panel_width, panel_height),
        );
        ui.register_panel(1, left_panel_bounds);

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

        // Update active viewport based on mouse position within viewport bounds.
        // This replaces the old two-phase scratch read-back where the Custom draw
        // function computed hovered_slot and layout.rs read it back after the frame.
        let mouse_pos = ui.mouse_pos();
        if viewport_bounds.contains(mouse_pos) {
            crate::input::update_active_viewport(
                &mut self.viewport_grid_state,
                mouse_pos,
                viewport_bounds.min,
                viewport_bounds.max,
            );
        }

        let bottom_bounds = Rect2D::from_origin_size(
            Vec2::new(0.0, panel_bottom),
            Vec2::new(screen_size.x(), bottom_panel_height),
        );
        ui.register_panel(4, bottom_bounds);

        // Draw tab bar and get content bounds for the active tab
        let (bottom_content_bounds, clicked_tab) = draw_bottom_tab_bar(
            ui,
            bottom_bounds,
            self.bottom_panel_tab,
            &self.theme,
            self.asset_browser.collapsed,
        );

        if let Some(tab) = clicked_tab {
            self.bottom_panel_tab = tab;
        }

        // Handle collapse toggle from tab bar
        let toggle_bounds_x = bottom_bounds.min.x() + ui.style().item_inner_spacing;
        let toggle_bounds_y = bottom_bounds.min.y() + (BOTTOM_TAB_HEIGHT - 20.0) * 0.5;
        let toggle_bounds = Rect2D::from_origin_size(
            Vec2::new(toggle_bounds_x, toggle_bounds_y),
            Vec2::new(20.0, 20.0),
        );
        if ui.is_hovered(toggle_bounds) && ui.mouse_clicked(katla_ui::mouse_button::LEFT) {
            self.asset_browser.collapsed = !self.asset_browser.collapsed;
        }

        // Only set context for the active bottom tab
        match self.bottom_panel_tab {
            BottomPanelTab::AssetBrowser => {
                ui.set_scratch(AssetBrowserDrawCtx {
                    bounds: bottom_content_bounds,
                    theme: self.theme.clone(),
                    is_focused: self.focused_panel == super::FocusedPanel::AssetBrowser,
                    viewport_bounds,
                });
            }
            BottomPanelTab::Console => {
                self.view_tree.env_mut().set(ConsoleDrawCtx {
                    theme: self.theme.clone(),
                    filter_levels: self.console_state.filter_levels,
                    search_filter: self.console_state.search_filter.clone(),
                    log_buffer: self.log_buffer.clone(),
                });
            }
        }

        // Set contexts for panels that need them before the view tree frame
        let prefs_is_open = self.preferences_panel_state.panel.is_visible();
        let prefs_llm_config = self.preferences_panel_state.llm_config.clone();
        let prefs_theme_key = self.theme_key().to_string();

        self.view_tree.env_mut().set(PreferencesDrawCtx {
            is_open: prefs_is_open,
            preferences: params.preferences.clone(),
            editor_settings: self.editor_settings.clone(),
            theme: self.theme.clone(),
            theme_key: prefs_theme_key,
            llm_config: prefs_llm_config,
        });

        self.view_tree.env_mut().set(ParticleInspectorDrawCtx {
            data: self.particle_inspector_data.clone(),
            theme: self.theme.clone(),
            is_open: self.particle_inspector_state.panel.is_visible(),
        });

        {
            let style = co_creator::CoCreatorStyle::from_theme(&self.theme);
            self.view_tree
                .env_mut()
                .set(super::declarative::CoCreatorDrawCtx {
                    messages: self
                        .co_creator
                        .messages
                        .iter()
                        .map(|m| (m.role.clone(), m.text.clone()))
                        .collect(),
                    processing: self.co_creator.processing,
                    status_message: self.co_creator.status_message.clone(),
                    user_msg_color: style.user_msg_color,
                    assistant_msg_color: style.assistant_msg_color,
                    system_msg_color: style.system_msg_color,
                    text_muted: style.text_muted,
                    agent_undo_count: params.agent_undo_count,
                    is_open: self.co_creator.is_open(),
                });
        }

        let input_consumed = self.view_tree.frame(ui, &EditorRootView, screen_size);
        ui.set_declarative_input_consumed(input_consumed);
        for action in self.view_tree.actions_mut().drain::<EditorAction>() {
            self.pending_actions.push(action);
        }

        // Build the asset browser from the declarative context (only if active tab)
        let entities = params.entities;
        let loader = &mut *params.loader;
        let thumbnail_texture_handles = params.thumbnail_texture_handles;

        if self.bottom_panel_tab == BottomPanelTab::AssetBrowser {
            let asset_actions = build_asset_browser_from_ctx(
                &mut self.asset_browser,
                ui,
                loader,
                thumbnail_texture_handles,
            );
            self.pending_actions.extend(asset_actions);
        }

        for action in self.view_tree.actions_mut().drain::<ToolbarAction>() {
            self.pending_actions.push(match action {
                ToolbarAction::NewScene => EditorAction::NewScene,
                ToolbarAction::OpenScene => EditorAction::OpenScene,
                ToolbarAction::SaveScene => EditorAction::SaveScene,
                ToolbarAction::Quit => EditorAction::Quit,
                ToolbarAction::Undo => EditorAction::Undo,
                ToolbarAction::Redo => EditorAction::Redo,
                ToolbarAction::OpenPreferences => {
                    EditorAction::OpenPanel(crate::ui::editor_ui::Panel::Preferences)
                }
                ToolbarAction::ToggleGrid => EditorAction::ToggleGrid,
                ToolbarAction::ToggleStats => EditorAction::ToggleStats,
                ToolbarAction::TogglePhysicsDebug => EditorAction::TogglePhysicsDebug,
                ToolbarAction::OpenParticleInspector => {
                    EditorAction::OpenPanel(crate::ui::editor_ui::Panel::ParticleInspector)
                }
                ToolbarAction::OpenCoCreator => {
                    EditorAction::OpenPanel(crate::ui::editor_ui::Panel::CoCreator)
                }
                ToolbarAction::SpawnModel(model) => {
                    EditorAction::SpawnModel(model, Vec3::new(0.0, 0.0, 0.0))
                }
                ToolbarAction::PlayStart => EditorAction::PlayStart,
                ToolbarAction::PlayPause => EditorAction::PlayPause,
                ToolbarAction::PlayStop => EditorAction::PlayStop,
            });
        }

        for action in self.view_tree.actions_mut().drain::<GizmoModeChanged>() {
            self.pending_actions
                .push(EditorAction::SetGizmoMode(action.0));
        }

        // TODO: Implement InspectorSync action emission to sync state back from declarative panel

        // TODO: Implement HierarchySync action emission to sync state back from declarative panel

        for sync in self
            .view_tree
            .actions_mut()
            .drain::<ParticleInspectorPanelSync>()
        {
            self.particle_inspector_state.panel.position = sync.position;
            self.particle_inspector_state.panel.visibility = match sync.visibility {
                katla_ui::declarative::DraggablePanelVisibility::Hidden => {
                    katla_ui::widgets::PanelState::Hidden
                }
                katla_ui::declarative::DraggablePanelVisibility::JustOpened => {
                    katla_ui::widgets::PanelState::JustOpened
                }
                katla_ui::declarative::DraggablePanelVisibility::Visible => {
                    katla_ui::widgets::PanelState::Visible
                }
            };
        }
        for action in self
            .view_tree
            .actions_mut()
            .drain::<crate::ui::ParticleInspectorAction>()
        {
            self.apply_particle_inspector_action(action);
        }

        for sync in self
            .view_tree
            .actions_mut()
            .drain::<super::declarative::CoCreatorPanelSync>()
        {
            self.co_creator.panel.visibility = match sync.visibility {
                katla_ui::declarative::DraggablePanelVisibility::Hidden => {
                    katla_ui::widgets::PanelState::Hidden
                }
                katla_ui::declarative::DraggablePanelVisibility::JustOpened => {
                    katla_ui::widgets::PanelState::JustOpened
                }
                katla_ui::declarative::DraggablePanelVisibility::Visible => {
                    katla_ui::widgets::PanelState::Visible
                }
            };
        }
        for action in self
            .view_tree
            .actions_mut()
            .drain::<super::declarative::CoCreatorSubmitAction>()
        {
            let text = action.text;
            if !text.trim().is_empty() {
                self.co_creator.submit_message(&text);
                self.pending_actions
                    .push(EditorAction::CoCreatorRequest(text));
            }
        }
        for _ in self
            .view_tree
            .actions_mut()
            .drain::<super::declarative::CoCreatorUndoAction>()
        {
            self.pending_actions.push(EditorAction::AgentUndo);
        }

        for sync in self.view_tree.actions_mut().drain::<PreferencesPanelSync>() {
            self.preferences_panel_state.panel.visibility = match sync.visibility {
                katla_ui::declarative::DraggablePanelVisibility::Hidden => {
                    katla_ui::widgets::PanelState::Hidden
                }
                katla_ui::declarative::DraggablePanelVisibility::JustOpened => {
                    katla_ui::widgets::PanelState::JustOpened
                }
                katla_ui::declarative::DraggablePanelVisibility::Visible => {
                    katla_ui::widgets::PanelState::Visible
                }
            };
        }
        for action in self.view_tree.actions_mut().drain::<PreferencesAction>() {
            self.apply_preferences_action(action);
        }

        // TODO: Implement ConsoleSync action emission to sync state back from declarative panel

        self.preferences_panel_state.llm_config = params.llm_config.clone();

        use std::collections::HashMap;
        let parent_map: HashMap<EntityId, Option<EntityId>> =
            entities.iter().map(|e| (e.id, e.parent_id)).collect();

        let visible_entities: Vec<EntityId> = entities
            .iter()
            .filter(|e| {
                editor_types::is_entity_visible_fast(
                    e,
                    &parent_map,
                    &self.hierarchy_state.expanded_entities,
                )
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

        ui.draw_rect(
            Rect2D::from_origin_size(
                Vec2::new(0.0, panel_bottom),
                Vec2::new(screen_size.x(), 1.0),
            ),
            ui.style().separator,
        );

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
                let display_name = if asset.name.chars().count() > max_chars {
                    format!(
                        "{}...",
                        asset.name.chars().take(max_chars).collect::<String>()
                    )
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

        // Dockable layout skeleton — renders alongside the hardcoded layout for
        // visual verification. Toggle via `use_dock_layout` on EditorUI.
        if self.use_dock_layout {
            let dock_bounds = Rect2D::from_origin_size(
                Vec2::new(0.0, toolbar_height),
                Vec2::new(screen_size.x(), screen_size.y() - toolbar_height),
            );

            let theme = &self.theme;
            let entities_ref = entities;

            ui.add(
                DockArea::new(&mut self.dock_layout, |ui, content_bounds, panel_id| {
                    let Some(panel) = super::EditorPanel::from_id(panel_id) else {
                        return;
                    };

                    let label = panel.name();
                    let text_size = ui.measure_text(label, ui.scaled_font_size(FontSize::Medium));
                    let text_pos =
                        Vec2::new(content_bounds.min.x() + 8.0, content_bounds.min.y() + 8.0);
                    ui.draw_text(
                        label,
                        text_pos,
                        theme.text_primary,
                        ui.scaled_font_size(FontSize::Medium),
                    );

                    let dims = format!(
                        "{:.0} x {:.0}",
                        content_bounds.width(),
                        content_bounds.height()
                    );
                    let _dims_size = ui.measure_text(&dims, ui.scaled_font_size(FontSize::XSmall));
                    let dims_pos = Vec2::new(
                        content_bounds.min.x() + 8.0,
                        text_pos.y() + text_size.y() + 4.0,
                    );
                    ui.draw_text(
                        &dims,
                        dims_pos,
                        theme.text_secondary,
                        ui.scaled_font_size(FontSize::XSmall),
                    );
                    let _ = entities_ref;
                })
                .bounds(dock_bounds),
            );
        }
    }
}
