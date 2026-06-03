use std::path::PathBuf;

use katla_math::{Rect2D, Vec2, Vec3};
use katla_ui::{UiContext, declarative::widgets::dock_space::DockAction, mouse_button};

use super::declarative::{
    AssetBrowserAction, AssetBrowserDrawCtx, AssetRenderData, ConsoleDrawCtx, GizmoDrawCtx,
    GizmoModeChanged, HierarchyAction, HierarchyDrawCtx, InspectorDrawCtx, MixerDrawCtx,
    ParticleInspectorDrawCtx, ParticleInspectorPanelSync, PreferencesDrawCtx, PreferencesPanelSync,
    StatusBarData, ToolbarAction, ToolbarDrawCtx, ViewportGridDrawCtx, process_asset_actions,
    process_declarative_actions,
};
use super::{
    EditorAction, EditorRenderParams, EditorUI,
    asset_browser::AssetType,
    co_creator,
    types::{self as editor_types, EditorPanel, PreferencesAction},
};

use super::declarative::toolbar::TOOLBAR_HEIGHT;

use super::declarative::STATUS_BAR_HEIGHT;

impl EditorUI {
    pub(super) fn build(&mut self, ui: &mut UiContext, params: &mut EditorRenderParams) {
        let screen_size = ui.screen_size();
        let mouse_pos = ui.mouse_pos();
        let toolbar_bottom = TOOLBAR_HEIGHT;
        let status_top = screen_size.y() - STATUS_BAR_HEIGHT;

        // Snapshot mutable state
        let inspector_edit = std::mem::take(&mut self.inspector_edit);
        let hierarchy_state = std::mem::take(&mut self.hierarchy_state);
        let entities: Vec<editor_types::EntityInfo> = params.entities.to_vec();

        // ── Compute dock layout bounds from DockTree ──
        let dock_bounds = Rect2D::from_origin_size(
            Vec2::new(0.0, toolbar_bottom),
            Vec2::new(screen_size.x(), status_top - toolbar_bottom),
        );
        let panel_bounds = self.dock_tree.leaf_bounds(dock_bounds);

        // Track viewport bounds from dock
        let mut dock_viewport_bounds = None;
        for (path, content_bounds) in &panel_bounds {
            // Get the active tab for this leaf to identify which panel it is
            if let Some(node) = self.dock_tree.get(path)
                && let katla_ui::dock::DockNode::Leaf { tabs, active } = node
                && let Some(&panel_id) = tabs.get(*active)
                && EditorPanel::from_id(panel_id) == Some(EditorPanel::Viewport)
            {
                dock_viewport_bounds = Some(*content_bounds);
                self.last_viewport_size = (
                    content_bounds.width().max(1.0) as u32,
                    content_bounds.height().max(1.0) as u32,
                );
                self.last_viewport_bounds = *content_bounds;

                if content_bounds.contains(mouse_pos) {
                    crate::input::update_active_viewport(
                        &mut self.viewport_grid_state,
                        mouse_pos,
                        content_bounds.min,
                        content_bounds.max,
                    );
                }
            }
        }

        // ── Set ALL env contexts ──
        let selected_count = if self.asset_browser.selected_indices.is_empty() {
            if self.asset_browser.selected_index.is_some() {
                1
            } else {
                0
            }
        } else {
            self.asset_browser.selected_indices.len()
        };

        // DockTree (for DockSpace widget to read from Environment for initial value)
        self.view_tree.env_mut().set(self.dock_tree.clone());

        // Status bar
        self.view_tree.env_mut().set(StatusBarData {
            height: STATUS_BAR_HEIGHT,
            fps: params.fps,
            frame_time_ms: params.frame_time_ms,
            frame_count: params.frame_count,
            entity_count: params.entities.len(),
            draw_call_count: self.last_draw_call_count,
            selected_count,
            total_assets: self.asset_browser.assets.len(),
            is_playing: self.is_playing,
            theme: self.theme.clone(),
            save_confirmation_timer: self.save_confirmation_timer,
        });

        // Toolbar
        self.view_tree.env_mut().set(ToolbarDrawCtx {
            show_grid: params.preferences.show_grid,
            show_stats: params.preferences.show_stats,
            show_physics_debug: params.preferences.show_physics_debug,
            show_reverb_debug: params.preferences.show_reverb_debug,
            text_muted: self.theme.text_muted,
            is_playing: self.is_playing,
            is_paused: self.is_paused,
            highlight: self.theme.highlight,
            warning: self.theme.warning,
            accent: self.theme.accent,
            error: self.theme.error,
        });

        // Gizmo
        self.view_tree.env_mut().set(GizmoDrawCtx {
            gizmo_mode: self.gizmo_mode,
        });

        // Viewport grid (uses bounds from dock)
        if let Some(vp_bounds) = dock_viewport_bounds {
            self.view_tree.env_mut().set(ViewportGridDrawCtx {
                bounds: vp_bounds,
                state: self.viewport_grid_state.clone(),
                texture_ids: self.viewport_texture_ids,
                theme: self.theme.clone(),
                mouse_pos,
            });
        }

        // Set docked panel envs using computed bounds
        for (path, content_bounds) in &panel_bounds {
            if let Some(node) = self.dock_tree.get(path)
                && let katla_ui::dock::DockNode::Leaf { tabs, active } = node
                && let Some(&panel_id) = tabs.get(*active)
            {
                match EditorPanel::from_id(panel_id) {
                    Some(EditorPanel::Hierarchy) => {
                        self.view_tree.env_mut().set(HierarchyDrawCtx {
                            bounds: *content_bounds,
                            entities: entities.clone(),
                            hierarchy_state: hierarchy_state.clone(),
                            theme: self.theme.clone(),
                            search_filter: self.hierarchy_search_filter.clone(),
                            selected_entity: self.selected_entity,
                        });
                    }
                    Some(EditorPanel::Inspector) => {
                        self.view_tree.env_mut().set(InspectorDrawCtx {
                            bounds: *content_bounds,
                            selected_entity: self.selected_entity,
                            entities: entities.clone(),
                            edit: inspector_edit.clone(),
                            theme: self.theme.clone(),
                            available_components: self.available_components.clone(),
                            add_component_open: self.add_component_open,
                            add_component_filter: self.add_component_filter.clone(),
                            focus_script_input: self.focus_script_input,
                            audio_listener_count: params
                                .entities
                                .iter()
                                .filter(|e| e.has_audio_listener)
                                .count(),
                        });
                    }
                    Some(EditorPanel::AssetBrowser) => {
                        let ab = &self.asset_browser;
                        self.view_tree.env_mut().set(AssetBrowserDrawCtx {
                            bounds: *content_bounds,
                            theme: self.theme.clone(),
                            assets: ab
                                .assets
                                .iter()
                                .map(|a| AssetRenderData {
                                    name: a.name.clone(),
                                    path: a.path.clone(),
                                    asset_type: a.asset_type,
                                    thumbnail_state: a.thumbnail_state.clone(),
                                })
                                .collect(),
                            selected_index: ab.selected_index,
                            path_segments: ab.path_segments(),
                            can_go_back: ab.can_go_back(),
                            can_go_forward: ab.can_go_forward(),
                            search_filter: ab.search_filter.clone(),
                            context_menu_open: ab.context_menu_open,
                            context_menu_is_asset: ab.context_menu_asset.is_some(),
                            confirm_dialog_open: ab.confirm_dialog_open,
                            confirm_dialog_message: ab.confirm_dialog_message.clone(),
                            collapsed: false,
                        });
                    }
                    Some(EditorPanel::Console) => {
                        self.view_tree.env_mut().set(ConsoleDrawCtx {
                            bounds: *content_bounds,
                            theme: self.theme.clone(),
                            filter_levels: self.console_state.filter_levels,
                            search_filter: self.console_state.search_filter.clone(),
                            log_buffer: self.log_buffer.clone(),
                        });
                    }
                    Some(EditorPanel::Mixer) => {
                        self.view_tree.env_mut().set(MixerDrawCtx {
                            bounds: *content_bounds,
                            levels: params.audio_levels,
                            active_voices: params.audio_active_voices,
                            peak_voices: params.audio_peak_voices,
                            preferences: params.preferences.clone(),
                            theme: self.theme.clone(),
                        });
                    }
                    _ => {}
                }
            }
        }

        // Floating panel contexts
        let theme_key = self.theme_key().to_string();
        self.view_tree.env_mut().set(PreferencesDrawCtx {
            is_open: self.preferences_panel_state.panel.is_visible(),
            preferences: params.preferences.clone(),
            editor_settings: self.editor_settings.clone(),
            theme: self.theme.clone(),
            theme_key,
            llm_config: params.llm_config.clone(),
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

        // ── Sync DockTree to StateArena and run frame ──
        self.sync_dock_state_to_arena();

        let input_consumed =
            self.view_tree
                .frame(ui, &super::declarative::EditorOverlayView, screen_size);
        ui.set_declarative_input_consumed(input_consumed);

        // ── Discover DockSpace state IDs on first frame ──
        if self.dock_state_id.is_none() {
            self.discover_dock_state_ids();
        }

        // ── Process DockActions from DockSpace widget ──
        self.process_dock_actions();

        // ── Process declarative actions ──
        for action in self.view_tree.actions_mut().drain::<ToolbarAction>() {
            self.pending_actions.push(match action {
                ToolbarAction::NewScene => EditorAction::NewScene,
                ToolbarAction::OpenScene => EditorAction::OpenScene,
                ToolbarAction::SaveScene => EditorAction::SaveScene,
                ToolbarAction::Quit => EditorAction::Quit,
                ToolbarAction::Undo => EditorAction::Undo,
                ToolbarAction::Redo => EditorAction::Redo,
                ToolbarAction::OpenPreferences => {
                    EditorAction::OpenPanel(super::Panel::Preferences)
                }
                ToolbarAction::ToggleGrid => EditorAction::ToggleGrid,
                ToolbarAction::ToggleStats => EditorAction::ToggleStats,
                ToolbarAction::TogglePhysicsDebug => EditorAction::TogglePhysicsDebug,
                ToolbarAction::ToggleReverbDebug => EditorAction::ToggleReverbDebug,
                ToolbarAction::OpenParticleInspector => {
                    EditorAction::OpenPanel(super::Panel::ParticleInspector)
                }
                ToolbarAction::OpenCoCreator => EditorAction::OpenPanel(super::Panel::CoCreator),
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

        for action in self.view_tree.actions_mut().drain::<HierarchyAction>() {
            match action {
                HierarchyAction::SelectEntity(id) => {
                    self.selected_entity = Some(id);
                    self.pending_actions.push(EditorAction::SelectEntity(id));
                }
            }
        }

        for sync in self
            .view_tree
            .actions_mut()
            .drain::<ParticleInspectorPanelSync>()
        {
            self.particle_inspector_state.panel.position = sync.position;
            self.particle_inspector_state.panel.visibility = sync.visibility;
        }
        for action in self
            .view_tree
            .actions_mut()
            .drain::<super::ParticleInspectorAction>()
        {
            self.apply_particle_inspector_action(action);
        }

        for sync in self
            .view_tree
            .actions_mut()
            .drain::<super::declarative::CoCreatorPanelSync>()
        {
            self.co_creator.panel.visibility = sync.visibility;
        }
        for action in self
            .view_tree
            .actions_mut()
            .drain::<super::declarative::CoCreatorSubmitAction>()
        {
            if !action.text.trim().is_empty() {
                self.co_creator.submit_message(&action.text);
                self.pending_actions
                    .push(EditorAction::CoCreatorRequest(action.text));
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
            self.preferences_panel_state.panel.visibility = sync.visibility;
        }
        for action in self.view_tree.actions_mut().drain::<PreferencesAction>() {
            self.apply_preferences_action(action);
        }

        // ── Process asset browser ──
        if self.asset_browser.needs_rescan() {
            self.asset_browser
                .scan_directory(params.thumbnail_texture_handles);
        }
        self.request_visible_thumbnails(params);

        let ab_actions: Vec<AssetBrowserAction> = self.view_tree.actions_mut().drain();
        let viewport_bounds = self.last_viewport_bounds;
        let asset_actions = process_declarative_actions(
            &mut self.asset_browser,
            params.thumbnail_texture_handles,
            viewport_bounds,
            ab_actions,
        );
        self.pending_actions.extend(asset_actions);
        let remaining = process_asset_actions(
            &mut self.asset_browser,
            params.thumbnail_texture_handles,
            viewport_bounds,
        );
        self.pending_actions.extend(remaining);

        // ── Update focused panel from mouse position ──
        if ui.mouse_clicked(mouse_button::LEFT) {
            for (path, bounds) in &panel_bounds {
                if bounds.contains(mouse_pos)
                    && let Some(node) = self.dock_tree.get(path)
                    && let katla_ui::dock::DockNode::Leaf { tabs, active } = node
                    && let Some(&panel_id) = tabs.get(*active)
                {
                    self.focused_panel = match EditorPanel::from_id(panel_id) {
                        Some(EditorPanel::Viewport) => super::FocusedPanel::Viewport,
                        Some(EditorPanel::Hierarchy) => super::FocusedPanel::Hierarchy,
                        Some(EditorPanel::Inspector) => super::FocusedPanel::Inspector,
                        Some(EditorPanel::AssetBrowser) => super::FocusedPanel::AssetBrowser,
                        _ => self.focused_panel,
                    };
                }
            }
        }

        // ── Restore state ──
        self.inspector_edit = inspector_edit;
        self.hierarchy_state = hierarchy_state;
        self.last_screen_size = screen_size;
        self.preferences_panel_state.llm_config = params.llm_config.clone();
    }

    /// Sync the DockTree from EditorUI to StateArena so the DockSpace widget can read it.
    fn sync_dock_state_to_arena(&mut self) {
        if let Some(dock_state_id) = self.dock_state_id {
            self.view_tree
                .state_arena_mut()
                .set(dock_state_id, self.dock_tree.clone());
        }
    }

    /// Find the DockSpace<u64> node in the view tree and cache its StateIds.
    fn discover_dock_state_ids(&mut self) {
        use katla_ui::declarative::widgets::dock_space::DockSpace;

        for (_, node) in self.view_tree.iter_nodes() {
            if let Some(ds) = node.widget.as_any().downcast_ref::<DockSpace<u64>>() {
                self.dock_state_id = Some(ds.dock_state_id);
                self.drag_state_id = Some(ds.drag_state_id);
                break;
            }
        }
    }

    /// Drain DockAction<u64> from the ActionStream and apply mutations to the DockTree.
    fn process_dock_actions(&mut self) {
        let actions: Vec<DockAction<u64>> = self.view_tree.actions_mut().drain();
        for action in actions {
            match action {
                DockAction::TabMoved {
                    from_path,
                    to_path,
                    zone,
                    tab,
                } => {
                    let _ = self.dock_tree.move_tab(&from_path, &to_path, zone);
                    let _ = tab; // used by move_tab
                }
                DockAction::TabClosed { path, tab } => {
                    let _ = self.dock_tree.remove_tab(&path, &tab);
                }
                DockAction::SplitResized { path, ratio } => {
                    let _ = self.dock_tree.set_ratio(&path, ratio);
                }
                DockAction::TabActivated { path, tab } => {
                    let _ = self.dock_tree.activate_tab(&path, &tab);
                }
            }
        }
    }

    fn request_visible_thumbnails(&mut self, params: &mut EditorRenderParams) {
        let scroll_offset = self.asset_browser.scroll_state.scroll_offset;
        let content_height = self.last_viewport_bounds.height();
        let item_size = 64.0;
        let row_height = item_size + 24.0;
        let col_count = self.asset_browser.last_col_count.max(1);

        let mut thumbs: Vec<(usize, PathBuf)> = Vec::new();
        for (i, asset) in self.asset_browser.assets.iter().enumerate() {
            if asset.asset_type != AssetType::Image {
                continue;
            }
            let row = i / col_count;
            let item_y = row as f32 * row_height - scroll_offset;
            if item_y + row_height < 0.0 || item_y > content_height {
                continue;
            }
            if matches!(
                asset.thumbnail_state,
                super::asset_browser::ThumbnailState::Pending
            ) && !params.loader.is_loading(&asset.path)
            {
                thumbs.push((i, asset.path.clone()));
            }
        }
        for (idx, path) in thumbs.into_iter().take(4) {
            params.loader.request_thumbnail(path, item_size as u32);
            self.asset_browser.assets[idx].thumbnail_state =
                super::asset_browser::ThumbnailState::Loading;
        }
    }
}
