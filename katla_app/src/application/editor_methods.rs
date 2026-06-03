//! Editor-specific Application method implementations.
//!
//! All methods in this file are gated behind `#[cfg(feature = "editor")]`.
//! Non-editor stubs live in `no_editor_methods.rs`.

use std::io::Write;

use super::Application;
use katla_gfx::GpuRenderer;
use katla_math::Vec2;

impl Application {
    pub(crate) fn on_viewport_texture_recreated(&mut self, slot: u32) {
        self.editor.editor_ui.set_viewport_bindless_index(slot);
    }

    pub(crate) fn filter_scroll_for_editor(&self, wheel_y: f32) -> f32 {
        let mouse_pos = self.ui_context.input().mouse_pos;
        let ui_claimed = self.ui_context.hover_z_index() > katla_ui::z_index::DEFAULT
            || self.ui_context.prev_hover_z_index() > katla_ui::z_index::DEFAULT;
        if ui_claimed
            || !self
                .editor
                .editor_ui
                .last_viewport_bounds
                .contains(mouse_pos)
        {
            0.0
        } else {
            wheel_y
        }
    }

    pub(crate) fn should_track_mouse_motion(&self) -> bool {
        !self.editor.gizmo_state.is_dragging()
    }

    pub(crate) fn should_send_game_input(&self) -> bool {
        if self.play_mode == super::game_state::PlayMode::Playing {
            return true;
        }
        self.editor.editor_ui.focused_panel == crate::ui::FocusedPanel::Viewport
            && !self.editor.gizmo_state.is_dragging()
            && !self.editor.gizmo_state.consumed_click
    }

    pub(crate) fn on_cursor_moved(&mut self, mouse_pos: Vec2) {
        self.update_gizmo_interaction(mouse_pos);
    }

    pub(crate) fn on_mouse_input(
        &mut self,
        state: &winit::event::ElementState,
        button: &winit::event::MouseButton,
    ) {
        self.handle_editor_mouse_press(state, button);
        self.handle_editor_mouse_release(state, button);
    }

    pub(crate) fn on_keyboard_input(
        &mut self,
        event: &winit::event::KeyEvent,
        keycode: winit::keyboard::KeyCode,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) {
        self.handle_editor_keyboard_shortcuts(event, keycode);
        self.handle_editor_gizmo_shortcuts(event, keycode, event_loop);
    }

    pub(crate) fn attach_billboard_icon(
        &mut self,
        entity_id: katla_ecs::EntityId,
        icon: crate::components::billboard::BillboardIcon,
    ) {
        use crate::components::BillboardComponent;
        self.world
            .add_component(entity_id, BillboardComponent::new(icon));
    }

    pub(crate) fn save_editor_state(&mut self) {
        self.editor.gui_state.left_panel_width = self.editor.editor_ui.left_panel_width;
        self.editor.gui_state.right_panel_width = self.editor.editor_ui.right_panel_width;
        self.editor.gui_state.asset_browser_height =
            self.editor.editor_ui.asset_browser.panel_height;

        if let Err(e) = self.editor.gui_state.save() {
            log::warn!("Failed to save GUI state: {}", e);
        } else {
            log::info!("Saved GUI state to disk");
        }
    }

    pub(crate) fn render_editor_frame(&mut self, dt: f32) {
        use super::editor;

        #[cfg(not(target_os = "macos"))]
        {
            let frame_idx = self.renderer.current_frame();
            if let Some(base_ldr_index) = self.frame_graph.get_ldr_texture_base_index() {
                let actual_ldr_index = base_ldr_index + frame_idx as u32;
                self.editor
                    .editor_ui
                    .set_viewport_bindless_index(actual_ldr_index);
            }

            log::debug!("Generating UI draw list...");
            let ui_draw_list = editor::generate_ui_draw_list(self, dt);
            log::debug!("UI draw list generated");

            // Save capture state for next frame's input routing.
            // Must happen after generate_ui_draw_list (which sets the flags) and
            // before process_editor_actions (which calls clear_frame_state).
            self.editor.editor_ui.prev_want_capture_keyboard =
                self.ui_context.input().want_capture_keyboard;
            self.editor.editor_ui.prev_want_capture_mouse =
                self.ui_context.input().want_capture_mouse;

            // Upload font atlas AFTER draw list generation (which rasterizes new glyphs)
            // and BEFORE render_frame (which samples from the GPU atlas).
            // Doing it after render_frame would cause a one-frame lag where text
            // samples from stale GPU data.
            editor::upload_font_atlas(self);

            // Render frame to GPU (includes UI if present)
            log::debug!("Rendering frame...");
            self.render_frame(ui_draw_list, dt, self.frame_count);
            self.editor.editor_ui.last_draw_call_count = self.last_draw_call_count;
            log::debug!("Frame rendered");

            // GPU picking: queue readback if a pick was triggered this frame,
            // or check the result from a previous frame's readback.
            self.process_picking();

            // Process editor actions after UI rendering
            editor::process_editor_actions(self);
        }

        #[cfg(target_os = "macos")]
        {
            if let Some(vp_slot) = self.renderer.viewport_bindless_index() {
                self.editor.editor_ui.set_viewport_bindless_index(vp_slot);
            }

            log::debug!("Generating UI draw list (Metal)...");
            let ui_draw_list = editor::generate_ui_draw_list(self, dt);
            log::debug!("UI draw list generated (Metal)");

            self.editor.editor_ui.prev_want_capture_keyboard =
                self.ui_context.input().want_capture_keyboard;
            self.editor.editor_ui.prev_want_capture_mouse =
                self.ui_context.input().want_capture_mouse;

            editor::upload_font_atlas(self);

            // Pass viewport panel bounds to renderer (logical → physical pixels)
            let vp_bounds = self.editor.editor_ui.last_viewport_bounds;
            let sf = self.scale_factor;
            let phys_rect = katla_gfx::Rect::new(
                [vp_bounds.min.x() * sf, vp_bounds.min.y() * sf],
                [vp_bounds.max.x() * sf, vp_bounds.max.y() * sf],
            );
            self.renderer.set_viewport_panel_rect(Some(phys_rect));

            log::debug!("Rendering frame (Metal)...");
            self.render_frame(ui_draw_list, dt, self.frame_count);
            self.editor.editor_ui.last_draw_call_count = self.last_draw_call_count;
            log::debug!("Frame rendered (Metal)");

            self.process_picking();

            editor::process_editor_actions(self);
        }
    }

    pub(crate) fn poll_background_loader(&mut self) {
        use crate::ui::ThumbnailState;
        use crate::util::LoadResult;

        let results = self.editor.background_loader.poll();

        for result in results {
            match result {
                LoadResult::ImageThumbnailLoaded {
                    path,
                    width,
                    height,
                    pixels,
                    ..
                } => {
                    log::debug!("Thumbnail loaded: {:?} ({}x{})", path, width, height);

                    // Upload texture to renderer and get TextureHandle
                    // Use SRGB format for correct color rendering in UI
                    let desc = katla_gfx::TextureDescriptor::rgba8_srgb(width, height);
                    let texture_handle = self.renderer.create_texture(&desc, &pixels);

                    // Get the bindless slot for this texture
                    let bindless_slot = self
                        .renderer
                        .get_bindless_slot(texture_handle)
                        .unwrap_or_else(|| {
                            log::warn!(
                                "Thumbnail texture {:?} (handle {}) has no bindless slot",
                                path,
                                texture_handle.index()
                            );
                            0 // Fallback to slot 0
                        });

                    // Register the bindless slot with the UI renderer
                    self.editor
                        .ui_renderer
                        .register_bindless_slot(texture_handle, bindless_slot);

                    // Update the thumbnail cache entry
                    if let Some(entry) = self.editor.background_loader.get_thumbnail_mut(&path) {
                        entry.uploaded = true;
                    }

                    // Store texture handle for this path (persists across directory navigations)
                    self.editor
                        .thumbnail_texture_handles
                        .insert(path.clone(), texture_handle);

                    // Update asset browser entries with this thumbnail
                    for asset in self.editor.editor_ui.asset_browser.assets.iter_mut() {
                        if asset.path == path {
                            asset.thumbnail_state = ThumbnailState::Loaded { texture_handle };
                            log::debug!(
                                "Updated thumbnail state for {:?} with handle {}, bindless slot {}",
                                path,
                                texture_handle.index(),
                                bindless_slot
                            );
                            break;
                        }
                    }
                }
                LoadResult::Failed { path, error, .. } => {
                    log::warn!("Failed to load {:?}: {}", path, error);

                    // Update asset browser entry to show failed state
                    for asset in self.editor.editor_ui.asset_browser.assets.iter_mut() {
                        if asset.path == path {
                            asset.thumbnail_state = ThumbnailState::Failed;
                            break;
                        }
                    }
                }
                LoadResult::FullTextureLoaded { .. } => {
                    // GPU upload handled by the caller
                }
                LoadResult::GltfModelLoaded { .. } => {
                    // GPU upload handled by the caller
                }
            }
        }
    }

    /// Serialize the UI layout tree and write to stdout or a file, then exit.
    ///
    /// Only runs once. Called after `render_editor_frame` so the ViewTree has been
    /// built and laid out for the current frame.
    pub(crate) fn dump_layout_if_needed(&mut self) {
        if self.layout_dumped {
            return;
        }

        let Some(target) = &self.info.dump_layout_path else {
            return;
        };

        let screen_size = if let Some(ref window) = self.window {
            let size = window.inner_size();
            Vec2::new(size.width as f32, size.height as f32) / self.scale_factor
        } else {
            // Headless mode: use renderer swapchain extent (physical), convert to logical
            let extent = self.renderer.swapchain_extent();
            Vec2::new(extent.width as f32, extent.height as f32) / self.scale_factor
        };

        let output =
            katla_ui::declarative::serialize_layout(self.editor.editor_ui.view_tree(), screen_size);

        match target {
            super::DumpLayoutTarget::Stdout => {
                println!("{output}");
            }
            super::DumpLayoutTarget::File(path) => match std::fs::File::create(path) {
                Ok(mut file) => {
                    if let Err(e) = file.write_all(output.as_bytes()) {
                        log::error!("Failed to write layout dump to {}: {e}", path);
                    } else {
                        log::info!("Layout dump written to {path}");
                    }
                }
                Err(e) => {
                    log::error!("Failed to create layout dump file {}: {e}", path);
                }
            },
        }

        self.layout_dumped = true;
    }
}
