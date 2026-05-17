use katla_ecs::EntityId;
use katla_math::{Color, Rect2D, Vec2};
use katla_ui::{
    FontSize, ForkAwesome, UiContext, mouse_button,
    widgets::{DockArea, ResizeHandle},
};

use super::declarative::{
    ConsoleDrawCtx, EditorRootView, HierarchyDrawCtx, InspectorDrawCtx, PreferencesDrawCtx,
    StatusBarData, ToolbarDrawCtx, build_asset_browser_from_ctx, set_asset_browser_ctx,
    set_co_creator_ctx, set_console_ctx, set_gizmo_ctx, set_hierarchy_ctx, set_inspector_ctx,
    set_particle_inspector_ctx, set_preferences_ctx, set_toolbar_ctx, set_viewport_grid_ctx,
    take_co_creator_ctx, take_console_ctx, take_gizmo_actions, take_hierarchy_ctx,
    take_inspector_ctx, take_particle_inspector_ctx, take_preferences_ctx, take_toolbar_ctx,
    take_viewport_grid_hovered,
};
use super::{
    EditorAction, EditorRenderParams, EditorUI, co_creator,
    types::{self as editor_types, BottomPanelTab},
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
            screen_size,
            height: status_bar_height,
            fps: params.fps,
            frame_count: params.frame_count,
            entity_count: params.entities.len(),
            selected_count,
            total_assets,
            is_playing: self.is_playing,
            theme: self.theme.clone(),
            save_confirmation_timer: self.save_confirmation_timer,
        };
        self.view_tree.env_mut().set(status_data.clone());
        ui.set_scratch(status_data);

        let toolbar_height = 36.0;
        self.toolbar_state.undo_count = params.undo_count;
        self.toolbar_state.redo_count = params.redo_count;

        let toolbar_ctx = ToolbarDrawCtx::new(
            std::mem::take(&mut self.toolbar_state),
            screen_size.x(),
            params.preferences,
            self.theme.text_muted,
            self.is_playing,
            self.is_paused,
            self.theme.highlight,
            self.theme.success,
            self.theme.warning,
        );
        set_toolbar_ctx(toolbar_ctx);
        set_gizmo_ctx(self.gizmo_mode, self.last_viewport_bounds);

        set_viewport_grid_ctx(
            self.last_viewport_bounds,
            &self.viewport_grid_state,
            &self.viewport_texture_ids,
            &self.theme,
        );

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

        let left_panel_bounds_for_hierarchy = Rect2D::from_origin_size(
            Vec2::new(0.0, toolbar_height),
            Vec2::new(self.left_panel_width, panel_height),
        );
        let hierarchy_ctx = HierarchyDrawCtx {
            bounds: left_panel_bounds_for_hierarchy,
            entities: params.entities.to_vec(),
            selected_entity: self.selected_entity,
            hierarchy_state: std::mem::take(&mut self.hierarchy_state),
            theme: self.theme.clone(),
            pending_actions: Vec::new(),
            search_filter: self.hierarchy_search_filter.clone(),
        };
        set_hierarchy_ctx(hierarchy_ctx);

        let inspector_bounds = Rect2D::from_origin_size(
            Vec2::new(right_panel_x, toolbar_height),
            Vec2::new(self.right_panel_width, panel_height),
        );
        let inspector_ctx = InspectorDrawCtx {
            bounds: inspector_bounds,
            selected_entity: self.selected_entity,
            entities: params.entities.to_vec(),
            edit: std::mem::take(&mut self.inspector_edit),
            scroll_state: std::mem::take(&mut self.inspector_scroll_state),
            theme: self.theme.clone(),
            pending_actions: Vec::new(),
            available_components: self.available_components.clone(),
            add_component_open: self.add_component_open,
            add_component_filter: self.add_component_filter.clone(),
        };
        set_inspector_ctx(inspector_ctx);

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

        if let Some((grid_bounds, hovered_slot)) = take_viewport_grid_hovered() {
            if hovered_slot.is_some() {
                let min = grid_bounds.min;
                let max = grid_bounds.max;
                crate::input::update_active_viewport(
                    &mut self.viewport_grid_state,
                    ui.mouse_pos(),
                    min,
                    max,
                );
            }
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
                set_asset_browser_ctx(
                    bottom_content_bounds,
                    self.theme.clone(),
                    self.focused_panel == super::FocusedPanel::AssetBrowser,
                    viewport_bounds,
                );
            }
            BottomPanelTab::Console => {
                set_console_ctx(ConsoleDrawCtx {
                    bounds: bottom_content_bounds,
                    theme: self.theme.clone(),
                    scroll_state: std::mem::take(&mut self.console_state.scroll_state),
                    filter_levels: self.console_state.filter_levels,
                    search_filter: std::mem::take(&mut self.console_state.search_filter),
                    log_buffer: self.log_buffer.clone(),
                    pending_actions: Vec::new(),
                    auto_scroll: self.console_state.auto_scroll,
                    selection_anchor: self.console_state.selection_anchor,
                    selection_cursor: self.console_state.selection_cursor,
                });
            }
        }

        // Set contexts for panels that need them before the view tree frame
        set_preferences_ctx(PreferencesDrawCtx {
            screen_size,
            state: std::mem::take(&mut self.preferences_panel_state),
            preferences: params.preferences.clone(),
            editor_settings: self.editor_settings.clone(),
            theme: self.theme.clone(),
            theme_key: self.theme_key().to_string(),
            pending_actions: Vec::new(),
        });

        set_particle_inspector_ctx(
            std::mem::take(&mut self.particle_inspector_state.panel),
            std::mem::take(&mut self.particle_inspector_state.scroll_state),
            self.selected_particle_emitter,
            &self.theme,
            &self.particle_inspector_data,
        );

        if self.co_creator.is_open() {
            let style = co_creator::CoCreatorStyle::from_theme(&self.theme);
            set_co_creator_ctx(
                std::mem::take(&mut self.co_creator),
                style,
                screen_size,
                params.agent_undo_count,
            );
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

        if let Some(toolbar_ctx) = take_toolbar_ctx() {
            self.toolbar_state = toolbar_ctx.state;
            self.pending_actions
                .append(&mut self.toolbar_state.pending_actions);
        }

        self.pending_actions.append(&mut take_gizmo_actions());

        if let Some(inspector_ctx) = take_inspector_ctx() {
            self.inspector_edit = inspector_ctx.edit;
            self.inspector_scroll_state = inspector_ctx.scroll_state;
            self.add_component_open = inspector_ctx.add_component_open;
            self.add_component_filter = inspector_ctx.add_component_filter;
            self.pending_actions
                .extend_from_slice(&inspector_ctx.pending_actions);
        }

        if let Some(hierarchy_ctx) = take_hierarchy_ctx() {
            self.hierarchy_state = hierarchy_ctx.hierarchy_state;
            self.hierarchy_search_filter = hierarchy_ctx.search_filter;
            self.selected_entity = hierarchy_ctx.selected_entity;
            self.pending_actions
                .extend_from_slice(&hierarchy_ctx.pending_actions);
        }

        if let Some((panel, scroll_state, selected_emitter, actions)) =
            take_particle_inspector_ctx()
        {
            self.particle_inspector_state.panel = panel;
            self.particle_inspector_state.scroll_state = scroll_state;
            self.selected_particle_emitter = selected_emitter;
            for action in actions {
                self.apply_particle_inspector_action(action);
            }
        }

        if let Some((state, response)) = take_co_creator_ctx() {
            self.co_creator = state;
            if let Some(text) = response.submitted_text {
                self.pending_actions
                    .push(EditorAction::CoCreatorRequest(text));
            }
            if response.undo_clicked {
                self.pending_actions.push(EditorAction::AgentUndo);
            }
        }

        if let Some(prefs_ctx) = take_preferences_ctx() {
            self.preferences_panel_state = prefs_ctx.state;
            for action in prefs_ctx.pending_actions {
                self.apply_preferences_action(action);
            }
        }

        if self.bottom_panel_tab == BottomPanelTab::Console {
            if let Some(console_ctx) = take_console_ctx() {
                self.console_state.scroll_state = console_ctx.scroll_state;
                self.console_state.filter_levels = console_ctx.filter_levels;
                self.console_state.search_filter = console_ctx.search_filter;
                self.console_state.auto_scroll = console_ctx.auto_scroll;
                self.console_state.selection_anchor = console_ctx.selection_anchor;
                self.console_state.selection_cursor = console_ctx.selection_cursor;
                for action in console_ctx.pending_actions {
                    self.pending_actions.push(action);
                }
            }
        }

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
