//! Popup and menu widgets.
//!
//! Context menus, dropdowns, modal dialogs, and popup containers.

use katla_math::{Color, Rect2D, Vec2};

use crate::icons::ForkAwesome;
use crate::input::mouse_button;
use crate::text::FontId;
use crate::FontSize;

use super::state::WidgetState;
use super::{z_index, UiContext};

/// Deferred draw command for dropdown items.
#[derive(Clone)]
pub enum DeferredDraw {
    Rect { bounds: Rect2D, color: Color },
    Text { text: String, pos: Vec2, color: Color, font_size: f32 },
}

impl UiContext {
    // -------------------------------------------------------------------------
    // Popup Input Blocking
    // -------------------------------------------------------------------------

    /// Check if a popup is currently open (built-in or custom).
    #[inline]
    pub fn is_popup_open(&self) -> bool {
        self.popup_id.is_some() || self.popup_bounds.is_some()
    }

    /// Register custom popup bounds for input blocking.
    ///
    /// Call this when rendering custom popups (like context menus) that aren't
    /// using the built-in begin_popup/end_popup system. This ensures that
    /// mouse clicks are captured and don't pass through to underlying widgets.
    pub fn set_custom_popup_bounds(&mut self, bounds: Rect2D) {
        self.popup_bounds = Some(bounds);
    }

    /// Clear custom popup bounds.
    pub fn clear_custom_popup_bounds(&mut self) {
        // Only clear if there's no popup_id (i.e., it was a custom popup)
        if self.popup_id.is_none() {
            self.popup_bounds = None;
        }
    }

    /// Block input for the current popup.
    ///
    /// Call this AFTER rendering your popup content. This registers the popup
    /// bounds so that subsequent frames can check if clicks should be blocked.
    pub fn block_input_for_popup(&mut self, popup_bounds: Rect2D) {
        // Register the bounds for click-outside detection
        self.set_custom_popup_bounds(popup_bounds);

        // Capture mouse when hovering over popup (tells application to not process game input)
        if popup_bounds.contains(self.input.mouse_pos) {
            self.input.want_capture_mouse = true;
        }
    }

    /// Check if mouse is over any registered popup bounds.
    ///
    /// Widgets should call this before processing clicks to avoid
    /// responding when a popup is covering them.
    pub fn is_mouse_over_popup(&self) -> bool {
        self.popup_bounds
            .map(|bounds| bounds.contains(self.input.mouse_pos))
            .unwrap_or(false)
    }

    /// Check if a popup is currently open.
    ///
    /// Widgets should check this before processing right-clicks to avoid
    /// opening new popups when one is already open.
    pub fn has_open_popup(&self) -> bool {
        self.popup_id.is_some()
    }

    /// Pre-register popup bounds BEFORE rendering regular widgets.
    ///
    /// Call this at the START of your UI rendering if you know a popup will be open.
    /// This ensures hover/click blocking works on the SAME frame the popup opens.
    pub fn preregister_popup(&mut self, bounds: Rect2D) {
        self.popup_bounds = Some(bounds);
    }

    /// Open a popup programmatically by ID.
    ///
    /// Use this for modal dialogs, alerts, or custom popups that aren't
    /// triggered by clicking a dropdown button or right-clicking.
    pub fn open_popup(&mut self, id: &str) {
        let popup_id = self.generate_id(id);
        self.popup_id = Some(popup_id);
        self.popup_opened_this_frame = true;
        self.active_id = None;
        self.input.focused_id = None;
    }

    /// Open a popup with known bounds (preregisters for same-frame blocking).
    pub fn open_popup_with_bounds(&mut self, id: &str, bounds: Rect2D) {
        self.open_popup(id);
        self.popup_bounds = Some(bounds);
    }

    /// Check if a specific popup is currently open.
    pub fn is_popup_open_with_id(&self, id: &str) -> bool {
        let popup_id = self.generate_id(id);
        self.popup_id == Some(popup_id)
    }

    // -------------------------------------------------------------------------
    // Container Popups
    // -------------------------------------------------------------------------

    /// Begin a popup container.
    ///
    /// Returns true if the popup is open and should have contents drawn.
    /// Call `end_popup()` after adding contents.
    /// The popup closes when clicking outside.
    pub fn begin_popup(&mut self, id: &str, bounds: Rect2D) -> bool {
        let popup_id = self.generate_id(id);

        // Check if this popup is open
        let is_open = self.popup_id == Some(popup_id);

        if is_open {
            // Switch to popup Z-index
            self.push_z_index(z_index::POPUP);

            // Draw popup background with shadow
            let shadow_offset = Vec2::new(4.0, 4.0);
            let shadow_bounds = Rect2D::new(bounds.min + shadow_offset, bounds.max + shadow_offset);
            self.draw_rect(shadow_bounds, self.style.popup_shadow);

            // Draw popup background
            self.draw_rect(bounds, self.style.popup_bg);
            self.draw_rect_border(bounds, Color::TRANSPARENT, self.style.popup_border, 1.0);

            // Store bounds and push clip (absolute - no intersection with parent)
            self.popup_bounds = Some(bounds);
            self.push_clip_absolute(bounds);

            // Push ID for contents
            self.push_id(id);
        }

        is_open
    }

    /// End a popup container.
    pub fn end_popup(&mut self) {
        self.pop_clip();
        self.pop_id();
        self.pop_z_index();
    }

    // -------------------------------------------------------------------------
    // Modal Dialogs
    // -------------------------------------------------------------------------

    /// Begin a modal dialog (centered overlay with dark background).
    ///
    /// Unlike regular popups, modal dialogs:
    /// - Have a semi-transparent background overlay
    /// - Don't close when clicking outside
    /// - Use TOOLTIP z-index (300) to appear above all other UI
    ///
    /// Returns true if the dialog is open and should have contents drawn.
    /// Call `end_modal_dialog()` after adding contents.
    ///
    /// Use `open_popup("dialog_id")` or `open_popup_with_bounds()` to show the dialog.
    pub fn begin_modal_dialog(&mut self, id: &str, width: f32, height: f32) -> bool {
        let popup_id = self.generate_id(id);

        let is_open = self.popup_id == Some(popup_id);

        if is_open {
            self.push_z_index(z_index::TOOLTIP);

            // Dark background overlay
            let screen_bounds = Rect2D::from_size(self.screen_size);
            self.draw_rect(screen_bounds, Color::new(0.0, 0.0, 0.0, 0.5));

            // Centered dialog
            let dialog_pos = Vec2::new(
                (self.screen_size.x() - width) * 0.5,
                (self.screen_size.y() - height) * 0.5,
            );
            let dialog_bounds = Rect2D::from_origin_size(dialog_pos, Vec2::new(width, height));

            // Shadow
            let shadow_bounds = Rect2D::new(
                dialog_bounds.min + Vec2::new(4.0, 4.0),
                dialog_bounds.max + Vec2::new(4.0, 4.0),
            );
            self.draw_rect(shadow_bounds, Color::new(0.0, 0.0, 0.0, 0.5));

            // Background
            self.draw_rect(dialog_bounds, self.style.popup_bg);
            self.draw_rect_border(dialog_bounds, Color::TRANSPARENT, self.style.popup_border, 1.0);

            self.popup_bounds = Some(dialog_bounds);
            self.push_clip_absolute(dialog_bounds);
            self.push_id(id);

            return true;
        }

        false
    }

    /// Get the bounds of the current modal dialog.
    pub fn modal_dialog_bounds(&self) -> Rect2D {
        self.popup_bounds.unwrap_or_else(|| Rect2D::from_size(Vec2::new(0.0, 0.0)))
    }

    /// Get the bounds of the current popup (for context menus, etc).
    pub fn get_popup_bounds(&self) -> Rect2D {
        self.popup_bounds.unwrap_or_else(|| Rect2D::from_size(Vec2::new(0.0, 0.0)))
    }

    /// End a modal dialog container.
    pub fn end_modal_dialog(&mut self) {
        self.pop_clip();
        self.pop_id();
        self.pop_z_index();
    }

    // -------------------------------------------------------------------------
    // Context Menus
    // -------------------------------------------------------------------------

    /// Open a context menu at the current mouse position.
    ///
    /// Call this when detecting a right-click on an area.
    /// Returns true if the menu was just opened.
    pub fn open_context_menu(&mut self, id: &str) -> bool {
        let context_id = self.generate_id(id);

        // Check for right-click
        if self.input.mouse_pressed[mouse_button::RIGHT] {
            self.storage.insert(
                context_id,
                WidgetState::ContextMenuPos(self.input.mouse_pos),
            );
            self.popup_id = Some(context_id);
            self.popup_opened_this_frame = true;
            return true;
        }

        false
    }

    /// Open a context menu at a specific position without checking for input.
    ///
    /// Use this when you've already checked for right-click and just want to open the menu.
    /// Returns true always (menu was opened).
    pub fn open_context_menu_at(&mut self, id: &str, pos: Vec2) -> bool {
        let context_id = self.generate_id(id);

        self.storage.insert(
            context_id,
            WidgetState::ContextMenuPos(pos),
        );
        self.popup_id = Some(context_id);
        self.popup_opened_this_frame = true;
        true
    }

    /// Check if a context menu is currently open.
    pub fn is_context_menu_open(&mut self, id: &str) -> bool {
        let context_id = self.generate_id(id);
        self.popup_id == Some(context_id)
    }

    /// Close the current popup/dropdown/context menu.
    pub fn close_current_popup(&mut self) {
        // Mark any dropdown as closed
        if let Some(popup_id) = self.popup_id {
            self.storage
                .insert(popup_id, WidgetState::DropdownOpen(false));
        }
        self.popup_id = None;
        self.popup_bounds = None;
    }

    /// Begin a context menu (right-click popup).
    ///
    /// Returns true if the context menu is open and should have items drawn.
    /// Items drawn inside will automatically size the background.
    pub fn begin_context_menu(&mut self, id: &str) -> bool {
        let context_id = self.generate_id(id);

        // Get stored position
        let pos = self
            .storage
            .get(&context_id)
            .and_then(|s| {
                if let WidgetState::ContextMenuPos(p) = s {
                    Some(*p)
                } else {
                    None
                }
            })
            .unwrap_or(self.input.mouse_pos);

        // Check if this context menu is open
        let is_open = self.popup_id == Some(context_id);

        if is_open {
            // Initialize content bounds tracking
            self.popup_content_bounds = None;
            self.dropdown_deferred.clear();

            // Set up initial popup bounds for get_popup_bounds() to work
            // Will be updated with correct size in end_context_menu()
            let initial_size = Vec2::new(self.style.menu_min_width, self.style.menu_item_height);
            self.popup_bounds = Some(Rect2D::from_origin_size(pos, initial_size));

            // Switch to popup Z-index
            self.push_z_index(z_index::POPUP);

            // Use full screen clip - content can render anywhere
            let screen_bounds = Rect2D::new(Vec2::new(0.0, 0.0), self.screen_size);
            self.push_clip_absolute(screen_bounds);
            self.push_id(id);

            return true;
        }

        false
    }

    /// End a context menu and draw the background based on tracked content bounds.
    pub fn end_context_menu(&mut self) {
        // Get tracked content bounds, or use initial popup bounds as fallback
        let content_bounds = self.popup_content_bounds.unwrap_or_else(|| {
            self.popup_bounds.unwrap_or(Rect2D::from_origin_size(
                Vec2::new(0.0, 0.0),
                Vec2::new(self.style.menu_min_width, self.style.menu_item_height),
            ))
        });

        // Ensure minimum size
        let min_width = self.style.menu_min_width;
        let min_height = self.style.menu_item_height;
        let final_width = content_bounds.width().max(min_width);
        let final_height = content_bounds.height().max(min_height);

        // Use the tracked content position as the background position
        let popup_bounds = Rect2D::from_origin_size(content_bounds.min, Vec2::new(final_width, final_height));

        // Draw popup background with shadow at lower z-index
        self.pop_z_index(); // Pop to get back to previous z
        self.push_z_index(z_index::POPUP - 1); // Draw background below content

        let shadow_offset = Vec2::new(4.0, 4.0);
        let shadow_bounds = Rect2D::new(
            popup_bounds.min + shadow_offset,
            popup_bounds.max + shadow_offset,
        );
        self.draw_rect(shadow_bounds, self.style.popup_shadow);
        self.draw_rect(popup_bounds, self.style.popup_bg);
        self.draw_rect_border(
            popup_bounds,
            Color::TRANSPARENT,
            self.style.popup_border,
            1.0,
        );

        self.pop_z_index(); // Pop the background z
        self.push_z_index(z_index::POPUP); // Restore popup z

        self.popup_bounds = Some(popup_bounds);

        self.pop_clip();
        self.pop_id();
        self.pop_z_index();
    }

    // -------------------------------------------------------------------------
    // Dropdown Menus
    // -------------------------------------------------------------------------

    /// Begin a menu bar item (dropdown without caret icon).
    ///
    /// Like `begin_dropdown` but styled for top-level menu bar items:
    /// - No caret/down arrow icon
    /// - Label centered in bounds
    ///
    /// Returns true if the menu is open and should have items drawn.
    pub fn begin_menu_item(&mut self, id: &str, label: &str, bounds: Rect2D) -> bool {
        self.begin_dropdown_ex(id, label, bounds, false)
    }

    /// Begin a dropdown menu with optional caret.
    ///
    /// Returns true if the dropdown is open and should have menu items drawn.
    /// Call `end_dropdown()` after adding contents.
    /// The `bounds` is the trigger button area; popup appears below it.
    pub fn begin_dropdown(&mut self, id: &str, label: &str, bounds: Rect2D) -> bool {
        self.begin_dropdown_ex(id, label, bounds, true)
    }

    /// Internal implementation with show_caret option.
    fn begin_dropdown_ex(&mut self, id: &str, label: &str, bounds: Rect2D, show_caret: bool) -> bool {
        let dropdown_id = self.generate_id(id);

        // Get or initialize open state
        let is_open = self
            .storage
            .get(&dropdown_id)
            .map(|s| matches!(s, WidgetState::DropdownOpen(true)))
            .unwrap_or(false);

        // Draw trigger button
        let hovered = self.update_hover(dropdown_id, bounds);

        // If hovering over this dropdown while another popup is open, switch to this one
        if hovered && self.popup_id.is_some() && self.popup_id != Some(dropdown_id) && !self.popup_opened_this_frame {
            // Close the other popup
            if let Some(other_id) = self.popup_id {
                self.storage
                    .insert(other_id, WidgetState::DropdownOpen(false));
            }
            // Open this dropdown
            self.storage
                .insert(dropdown_id, WidgetState::DropdownOpen(true));
            self.popup_id = Some(dropdown_id);
            self.popup_bounds = Some(Rect2D::from_origin_size(
                Vec2::new(bounds.min.x(), bounds.max.y()),
                Vec2::new(bounds.width().max(self.style.menu_min_width), 200.0),
            ));
        }

        // Toggle on click
        if self.button_behavior(dropdown_id, bounds) {
            let new_open = !is_open;  // Simple toggle
            self.storage
                .insert(dropdown_id, WidgetState::DropdownOpen(new_open));
            if new_open {
                self.popup_id = Some(dropdown_id);
                self.popup_opened_this_frame = true;
                // Set popup bounds immediately so click-outside check works
                self.popup_bounds = Some(Rect2D::from_origin_size(
                    Vec2::new(bounds.min.x(), bounds.max.y()),
                    Vec2::new(bounds.width().max(self.style.menu_min_width), 200.0),
                ));
            } else {
                self.popup_id = None;
                self.popup_bounds = None;
            }
        }

        // Determine button colors
        let bg_color = if is_open {
            self.style.menu_active
        } else if self.active_id == Some(dropdown_id) {
            self.style.button_active
        } else if hovered {
            self.style.button_hovered
        } else {
            self.style.button_normal
        };

        self.draw_rect(bounds, bg_color);

        // Draw label centered
        let text_size = self.measure_text(label, self.style.font_size);
        let text_offset = if show_caret { 10.0 } else { 0.0 };
        let text_pos = Vec2::new(
            bounds.center().x() - text_size.x() * 0.5 - text_offset,
            bounds.center().y() - text_size.y() * 0.5,
        );
        self.draw_text(
            label,
            text_pos,
            self.style.button_text,
            self.style.font_size,
        );

        // Draw dropdown icon (only if show_caret is true)
        if show_caret {
            let icon = ForkAwesome::CARET_DOWN;
            let icon_size = self.style.font_size;
            let icon_pos = Vec2::new(
                bounds.center().x() + text_size.x() * 0.5 + 2.0,
                bounds.center().y() - icon_size * 0.5,
            );
            self.draw_icon_aligned(
                icon,
                icon_pos,
                icon_size,
                self.style.button_text,
                FontId::DEFAULT,
            );
        }

        // If open, prepare popup area
        if is_open {
            // Switch to popup Z-index
            self.push_z_index(z_index::POPUP);

            // Store popup origin for get_popup_bounds()
            let popup_origin = Vec2::new(bounds.min.x(), bounds.max.y());

            // Initialize content bounds tracking
            self.popup_content_bounds = None;
            self.dropdown_deferred.clear();

            // Store popup dimensions for later (background drawn in end_dropdown)
            // Use large initial size for clipping, will be clamped to content in end_dropdown
            let popup_bounds =
                Rect2D::from_origin_size(popup_origin, Vec2::new(self.style.menu_min_width, 500.0));

            // Don't draw background yet - we'll draw it in end_dropdown at correct size
            // Just set up clipping and state
            self.popup_bounds = Some(popup_bounds);
            self.push_clip_absolute(popup_bounds);
            self.push_id(id);

            return true;
        }

        false
    }

    /// Track popup content bounds (call from menu_item, etc.)
    /// Expands the tracked bounding box to include this item.
    pub fn track_popup_item(&mut self, item_bounds: Rect2D) {
        self.popup_content_bounds = Some(match self.popup_content_bounds {
            None => item_bounds,
            Some(existing) => Rect2D::new(
                Vec2::new(existing.min.x().min(item_bounds.min.x()), existing.min.y().min(item_bounds.min.y())),
                Vec2::new(existing.max.x().max(item_bounds.max.x()), existing.max.y().max(item_bounds.max.y())),
            ),
        });
    }

    /// Defer a draw for dropdown items (drawn after background)
    fn defer_rect(&mut self, bounds: Rect2D, color: Color) {
        self.dropdown_deferred.push(DeferredDraw::Rect { bounds, color });
    }

    fn defer_text(&mut self, text: &str, pos: Vec2, color: Color, font_size: f32) {
        self.dropdown_deferred.push(DeferredDraw::Text {
            text: text.to_string(),
            pos,
            color,
            font_size,
        });
    }

    /// End a dropdown menu.
    pub fn end_dropdown(&mut self) {
        // Get tracked content bounds, or use initial popup bounds as fallback
        let content_bounds = self.popup_content_bounds.unwrap_or_else(|| {
            self.popup_bounds.unwrap_or(Rect2D::from_origin_size(
                Vec2::new(0.0, 0.0),
                Vec2::new(self.style.menu_min_width, self.style.menu_item_height),
            ))
        });

        // Ensure minimum size
        let min_width = self.style.menu_min_width;
        let min_height = self.style.menu_item_height;
        let final_width = content_bounds.width().max(min_width);
        let final_height = content_bounds.height().max(min_height);

        // Use the tracked content position as the background position
        let correct_bounds = Rect2D::from_origin_size(content_bounds.min, Vec2::new(final_width, final_height));

        // Draw background at the correct size FIRST
        let shadow_offset = Vec2::new(4.0, 4.0);
        let shadow_bounds = Rect2D::new(
            correct_bounds.min + shadow_offset,
            correct_bounds.max + shadow_offset,
        );
        self.draw_rect(shadow_bounds, self.style.popup_shadow);
        self.draw_rect(correct_bounds, self.style.popup_bg);
        self.draw_rect_border(
            correct_bounds,
            Color::TRANSPARENT,
            self.style.popup_border,
            1.0,
        );

        // Take deferred draws to avoid borrow issues
        let deferred = std::mem::take(&mut self.dropdown_deferred);

        // Now replay deferred item draws ON TOP of background
        for cmd in deferred {
            match cmd {
                DeferredDraw::Rect { bounds, color } => {
                    self.draw_rect(bounds, color);
                }
                DeferredDraw::Text { text, pos, color, font_size } => {
                    self.draw_text(&text, pos, color, font_size);
                }
            }
        }

        self.popup_bounds = Some(correct_bounds);

        // Block input for this popup
        if correct_bounds.contains(self.input.mouse_pos) {
            self.input.want_capture_mouse = true;
        }
        self.input.want_capture_keyboard = true;

        self.pop_clip();
        self.pop_id();
        self.pop_z_index();
    }

    // -------------------------------------------------------------------------
    // Combo Box
    // -------------------------------------------------------------------------

    /// Begin a combo box (dropdown with selection).
    ///
    /// Returns true if the combo is open and should have items drawn.
    /// Call `end_combo()` after adding selectable items.
    /// The `preview` text is shown in the closed combo box.
    pub fn begin_combo(&mut self, id: &str, preview: &str, bounds: Rect2D) -> bool {
        let combo_id = self.generate_id(id);

        // Get or initialize open state
        let is_open = self
            .storage
            .get(&combo_id)
            .map(|s| matches!(s, WidgetState::DropdownOpen(true)))
            .unwrap_or(false);

        // Draw combo box
        let hovered = self.update_hover(combo_id, bounds);

        // Toggle on click
        if self.button_behavior(combo_id, bounds) {
            let new_open = !is_open;
            self.storage
                .insert(combo_id, WidgetState::DropdownOpen(new_open));
            if new_open {
                self.popup_id = Some(combo_id);
                self.popup_opened_this_frame = true;
            } else {
                self.popup_id = None;
                self.popup_bounds = None;
            }
        }

        // Determine combo colors
        let bg_color = if is_open {
            self.style.combo_bg
        } else if self.active_id == Some(combo_id) || hovered {
            self.style.combo_hovered
        } else {
            self.style.combo_bg
        };

        self.draw_rect(bounds, bg_color);
        self.draw_rect_border(bounds, Color::TRANSPARENT, self.style.combo_border, 1.0);

        // Draw preview text (top-left positioning, centered vertically)
        let text_size = self.measure_text(preview, self.style.font_size);
        let text_pos = Vec2::new(
            bounds.min.x() + self.style.menu_padding,
            bounds.center().y() - text_size.y() * 0.5,
        );
        self.draw_text(
            preview,
            text_pos,
            self.style.combo_text,
            self.style.font_size,
        );

        // Draw dropdown icon
        let icon = ForkAwesome::CARET_DOWN;
        let icon_size = self.style.font_size;
        let icon_pos = Vec2::new(
            bounds.max.x() - icon_size - self.style.menu_padding,
            bounds.center().y() - icon_size * 0.5,
        );
        self.draw_icon_aligned(
            icon,
            icon_pos,
            icon_size,
            self.style.combo_text,
            FontId::DEFAULT,
        );

        // If open, prepare popup area
        if is_open {
            // Switch to popup Z-index
            self.push_z_index(z_index::POPUP);

            let popup_bounds = Rect2D::from_origin_size(
                Vec2::new(bounds.min.x(), bounds.max.y()),
                Vec2::new(
                    bounds.width().max(self.style.menu_min_width),
                    400.0, // Generous height for menu items
                ),
            );

            // Draw popup background with shadow
            let shadow_offset = Vec2::new(4.0, 4.0);
            let shadow_bounds = Rect2D::new(
                popup_bounds.min + shadow_offset,
                popup_bounds.max + shadow_offset,
            );
            self.draw_rect(shadow_bounds, self.style.popup_shadow);
            self.draw_rect(popup_bounds, self.style.popup_bg);
            self.draw_rect_border(
                popup_bounds,
                Color::TRANSPARENT,
                self.style.popup_border,
                1.0,
            );

            self.popup_bounds = Some(popup_bounds);
            self.push_clip_absolute(popup_bounds); // Absolute clip - render outside parent
            self.push_id(id);

            return true;
        }

        false
    }

    /// End a combo box.
    pub fn end_combo(&mut self) {
        self.pop_clip();
        self.pop_id();
        self.pop_z_index();
    }

    // -------------------------------------------------------------------------
    // Clean Popup API
    // -------------------------------------------------------------------------

    /// Show a popup menu with automatic layout and sizing.
    ///
    /// Returns the value from the closure if the popup was shown.
    ///
    /// # Example
    /// ```ignore
    /// let action = ui.popup("context", |ui| {
    ///     if ui.popup_item("Open", ForkAwesome::FOLDER_OPEN, true) { return Some("open"); }
    ///     if ui.popup_item("Rename", ForkAwesome::PENCIL, true) { return Some("rename"); }
    ///     ui.popup_separator();
    ///     if ui.popup_item("Delete", ForkAwesome::TRASH, true) { return Some("delete"); }
    ///     None
    /// });
    /// if let Some(action) = action {
    ///     // handle action
    /// }
    /// ```
    ///
    /// The popup automatically:
    /// - Positions items vertically
    /// - Tracks content bounds
    /// - Draws background that fits content exactly
    pub fn popup<F, R>(&mut self, id: &str, f: F) -> Option<R>
    where
        F: FnOnce(&mut Self) -> R,
    {
        if self.begin_auto_popup(id) {
            let result = f(self);
            self.end_auto_popup();
            Some(result)
        } else {
            None
        }
    }

    /// Begin an auto-sizing popup (internal).
    fn begin_auto_popup(&mut self, id: &str) -> bool {
        let popup_id = self.generate_id(id);

        // Get stored position
        let pos = self
            .storage
            .get(&popup_id)
            .and_then(|s| {
                if let WidgetState::ContextMenuPos(p) = s {
                    Some(*p)
                } else {
                    None
                }
            })
            .unwrap_or(self.input.mouse_pos);

        // Check if popup is open
        let is_open = self.popup_id == Some(popup_id);

        if is_open {
            // Initialize popup state
            self.popup_content_bounds = None;
            self.popup_cursor = pos;
            self.popup_width = self.style.menu_min_width;

            // Set up for drawing
            self.popup_bounds = Some(Rect2D::from_origin_size(pos, Vec2::new(self.popup_width, 0.0)));
            self.push_z_index(z_index::POPUP);
            let screen_bounds = Rect2D::new(Vec2::new(0.0, 0.0), self.screen_size);
            self.push_clip_absolute(screen_bounds);
            self.push_id(id);

            return true;
        }

        false
    }

    /// Draw a popup menu item with automatic positioning.
    ///
    /// Returns true if the item was clicked.
    pub fn popup_item(&mut self, label: &str, icon: char, enabled: bool) -> bool {
        let item_height = self.style.menu_item_height;

        let item_bounds = Rect2D::from_origin_size(
            self.popup_cursor,
            Vec2::new(self.popup_width, item_height),
        );

        // Track this item for background sizing
        self.track_popup_item(item_bounds);

        // Draw the item
        let clicked = self.draw_popup_item_contents(label, icon, enabled, item_bounds, "");

        // Advance cursor
        self.popup_cursor = Vec2::new(self.popup_cursor.x(), self.popup_cursor.y() + item_height);

        clicked
    }

    /// Draw a popup menu item with a keyboard shortcut hint.
    ///
    /// Returns true if the item was clicked.
    pub fn popup_item_with_shortcut(&mut self, label: &str, icon: char, enabled: bool, shortcut: &str) -> bool {
        let item_height = self.style.menu_item_height;

        let item_bounds = Rect2D::from_origin_size(
            self.popup_cursor,
            Vec2::new(self.popup_width, item_height),
        );

        // Track this item for background sizing
        self.track_popup_item(item_bounds);

        // Draw the item
        let clicked = self.draw_popup_item_contents(label, icon, enabled, item_bounds, shortcut);

        // Advance cursor
        self.popup_cursor = Vec2::new(self.popup_cursor.x(), self.popup_cursor.y() + item_height);

        clicked
    }

    /// Draw a popup separator with automatic positioning.
    pub fn popup_separator(&mut self) {
        let separator_height = 8.0;

        let sep_bounds = Rect2D::from_origin_size(
            self.popup_cursor,
            Vec2::new(self.popup_width, separator_height),
        );

        // Track this separator for background sizing
        self.track_popup_item(sep_bounds);

        // Draw the separator line
        self.draw_line(
            Vec2::new(sep_bounds.min.x() + 8.0, sep_bounds.center().y()),
            Vec2::new(sep_bounds.max.x() - 8.0, sep_bounds.center().y()),
            self.style.separator,
            1.0,
        );

        // Advance cursor
        self.popup_cursor = Vec2::new(self.popup_cursor.x(), self.popup_cursor.y() + separator_height);
    }

    /// Internal: draw popup item contents (hover, icon, label, shortcut).
    fn draw_popup_item_contents(&mut self, label: &str, icon: char, enabled: bool, bounds: Rect2D, shortcut: &str) -> bool {
        let hovered = self.is_hovered(bounds);

        // Hover background
        if enabled && hovered {
            self.draw_rect(bounds, self.style.menu_hovered);
        }

        let text_size = self.scaled_font_size(FontSize::Small);
        let text_y = bounds.min.y() + 6.0;

        // Colors
        let icon_color = if enabled { self.style.text_color } else { self.style.text_disabled };
        let label_color = if enabled { self.style.text_color } else { self.style.text_disabled };

        // Icon
        self.draw_icon_aligned(
            icon,
            Vec2::new(bounds.min.x() + 8.0, text_y),
            12.0,
            icon_color,
            FontId::DEFAULT,
        );

        // Label
        self.draw_text(
            label,
            Vec2::new(bounds.min.x() + 28.0, text_y),
            label_color,
            text_size,
        );

        // Shortcut (right-aligned)
        if !shortcut.is_empty() {
            let shortcut_size = self.measure_text(shortcut, text_size);
            self.draw_text(
                shortcut,
                Vec2::new(bounds.max.x() - shortcut_size.x() - 8.0, text_y),
                self.style.text_disabled,
                text_size,
            );
        }

        // Click detection
        enabled && hovered && self.input.mouse_clicked(mouse_button::LEFT)
    }

    /// End an auto-sizing popup and draw the background.
    fn end_auto_popup(&mut self) {
        // Get tracked content bounds
        let content_bounds = self.popup_content_bounds.unwrap_or_else(|| {
            Rect2D::from_origin_size(self.popup_cursor, Vec2::new(self.style.menu_min_width, self.style.menu_item_height))
        });

        // Ensure minimum size
        let final_width = content_bounds.width().max(self.style.menu_min_width);
        let final_height = content_bounds.height().max(self.style.menu_item_height);

        let popup_bounds = Rect2D::from_origin_size(content_bounds.min, Vec2::new(final_width, final_height));

        // Draw background at lower z-index
        self.pop_z_index();
        self.push_z_index(z_index::POPUP - 1);

        let shadow_offset = Vec2::new(4.0, 4.0);
        let shadow_bounds = Rect2D::new(
            popup_bounds.min + shadow_offset,
            popup_bounds.max + shadow_offset,
        );
        self.draw_rect(shadow_bounds, self.style.popup_shadow);
        self.draw_rect(popup_bounds, self.style.popup_bg);
        self.draw_rect_border(popup_bounds, Color::TRANSPARENT, self.style.popup_border, 1.0);

        self.pop_z_index();
        self.push_z_index(z_index::POPUP);

        self.popup_bounds = Some(popup_bounds);

        self.pop_clip();
        self.pop_id();
        self.pop_z_index();
    }

    // -------------------------------------------------------------------------
    // Menu Widgets
    // -------------------------------------------------------------------------

    /// Draw a menu item (clickable item styled for menus).
    ///
    /// Returns true if clicked this frame.
    pub fn menu_item(&mut self, id: &str, label: &str, bounds: Rect2D) -> bool {
        let widget_id = self.generate_id(id);
        let clicked = self.button_behavior(widget_id, bounds);

        // Track height for dropdown auto-sizing
        self.track_popup_item(bounds);

        // Determine colors based on state
        let bg_color = if self.active_id == Some(widget_id) {
            self.style.menu_active
        } else if self.hovered_id == Some(widget_id) || self.is_hovered(bounds) {
            self.style.menu_hovered
        } else {
            Color::TRANSPARENT
        };

        // Defer background draw (will be drawn after popup background)
        if bg_color != Color::TRANSPARENT {
            self.defer_rect(bounds, bg_color);
        }

        // Defer text draw
        let text_size = self.measure_text(label, self.style.font_size);
        let text_pos = Vec2::new(
            bounds.min.x() + self.style.menu_padding,
            bounds.center().y() - text_size.y() * 0.5,
        );
        self.defer_text(label, text_pos, self.style.text_color, self.style.font_size);

        clicked
    }

    /// Draw a toggle menu item that shows a checkmark when enabled.
    ///
    /// Returns true if clicked this frame.
    pub fn toggle_menu_item(&mut self, id: &str, label: &str, checked: bool, bounds: Rect2D) -> bool {
        let widget_id = self.generate_id(id);
        let clicked = self.button_behavior(widget_id, bounds);

        // Track height for dropdown auto-sizing
        self.track_popup_item(bounds);

        // Determine colors based on state
        let bg_color = if self.active_id == Some(widget_id) {
            self.style.menu_active
        } else if self.hovered_id == Some(widget_id) || self.is_hovered(bounds) {
            self.style.menu_hovered
        } else {
            Color::TRANSPARENT
        };

        // Defer background draw (will be drawn after popup background)
        if bg_color != Color::TRANSPARENT {
            self.defer_rect(bounds, bg_color);
        }

        // Draw checkmark icon if checked
        let icon_size = self.style.font_size;
        let check_icon = ForkAwesome::CHECK;
        let mut text_x = bounds.min.x() + self.style.menu_padding;

        if checked {
            let icon_y = bounds.center().y() - icon_size * 0.5;
            // Use defer_text with single char for icon
            self.defer_text(&check_icon.to_string(), Vec2::new(text_x, icon_y), self.style.text_color, icon_size);
            text_x += icon_size + 4.0; // Icon + spacing
        } else {
            text_x += icon_size + 4.0; // Reserve space for alignment when unchecked
        }

        // Defer text draw
        let text_size = self.measure_text(label, self.style.font_size);
        let text_pos = Vec2::new(
            text_x,
            bounds.center().y() - text_size.y() * 0.5,
        );
        self.defer_text(label, text_pos, self.style.text_color, self.style.font_size);

        clicked
    }

    /// Draw a menu separator line.
    ///
    /// Use this inside a popup/context menu to separate groups of items.
    pub fn menu_separator(&mut self, bounds: Rect2D) {
        // Track this separator for auto-sizing the popup background
        self.track_popup_item(bounds);

        self.draw_line(
            Vec2::new(bounds.min.x() + 8.0, bounds.center().y()),
            Vec2::new(bounds.max.x() - 8.0, bounds.center().y()),
            self.style.separator,
            1.0,
        );
    }

    /// Draw a popup menu item with icon, label, and optional shortcut.
    ///
    /// Returns true if the item was clicked.
    /// Use this inside a popup/context menu after calling begin_popup() or begin_context_menu().
    ///
    /// # Arguments
    /// - `label`: Display text
    /// - `icon`: Icon character (ForkAwesome)
    /// - `enabled`: Whether the item is clickable
    /// - `shortcut`: Optional keyboard shortcut hint (e.g., "Ctrl+S")
    /// - `bounds`: Item bounds (position and size)
    pub fn popup_menu_item(&mut self, label: &str, icon: char, enabled: bool, shortcut: &str, bounds: Rect2D) -> bool {
        // Track this item for auto-sizing the popup background
        self.track_popup_item(bounds);
        // Delegate to internal helper
        self.draw_popup_item_contents(label, icon, enabled, bounds, shortcut)
    }

    /// Draw a context menu from a list of items.
    ///
    /// This is a convenience function that handles the entire context menu rendering:
    /// - Shadow and background
    /// - Item iteration with consistent styling
    /// - Click and hover handling
    /// - Click-outside and Escape-to-close
    ///
    /// Returns the label of the clicked item, if any.
    ///
    /// # Arguments
    /// - `pos`: Menu position (top-left corner)
    /// - `items`: Slice of (label, icon, enabled, shortcut) tuples
    ///
    /// # Example
    /// ```ignore
    /// let items = [
    ///     ("Open", ForkAwesome::FOLDER_OPEN, true, "Enter"),
    ///     ("Rename", ForkAwesome::PENCIL, true, "F2"),
    ///     ("---", '\0', false, ""), // Separator
    ///     ("Delete", ForkAwesome::TRASH, true, "Del"),
    /// ];
    /// if let Some(action) = ui.context_menu(pos, &items) {
    ///     match action {
    ///         "Open" => { /* ... */ }
    ///         "Delete" => { /* ... */ }
    ///         _ => {}
    ///     }
    /// }
    /// ```
    pub fn context_menu(&mut self, pos: Vec2, items: &[(&str, char, bool, &str)]) -> Option<String> {
        let item_height = 24.0;
        let separator_height = 8.0;
        let padding = 4.0;

        // Count items and separators
        let item_count = items.iter().filter(|(l, _, _, _)| *l != "---").count();
        let separator_count = items.iter().filter(|(l, _, _, _)| *l == "---").count();

        // Calculate menu dimensions
        let menu_width = self.style.menu_min_width.max(150.0);
        let menu_height = (item_count as f32 * item_height) + (separator_count as f32 * separator_height) + padding;

        // Clamp to screen bounds
        let clamped_x = pos.x().min(self.screen_size.x() - menu_width - 10.0).max(10.0);
        let clamped_y = pos.y().min(self.screen_size.y() - menu_height - 10.0).max(10.0);
        let menu_pos = Vec2::new(clamped_x, clamped_y);
        let menu_bounds = Rect2D::from_origin_size(menu_pos, Vec2::new(menu_width, menu_height));

        // Push z-index and draw background
        self.push_z_index(z_index::POPUP);

        // Shadow
        let shadow_bounds = Rect2D::new(
            menu_bounds.min + Vec2::new(3.0, 3.0),
            menu_bounds.max + Vec2::new(3.0, 3.0),
        );
        self.draw_rect(shadow_bounds, Color::new(0.0, 0.0, 0.0, 0.5));

        // Background
        self.draw_rect(menu_bounds, self.style.popup_bg);
        self.draw_rect_border(menu_bounds, Color::TRANSPARENT, self.style.popup_border, 1.0);

        // Store bounds for input blocking
        let old_popup_bounds = self.popup_bounds;
        self.popup_bounds = Some(menu_bounds);

        // Render items
        let mut clicked_action: Option<String> = None;
        let mut current_y = menu_pos.y() + 2.0;

        for (label, icon, enabled, shortcut) in items.iter() {
            if *label == "---" {
                // Separator
                let sep_bounds = Rect2D::from_origin_size(
                    Vec2::new(menu_pos.x(), current_y),
                    Vec2::new(menu_width, separator_height),
                );
                self.menu_separator(sep_bounds);
                current_y += separator_height;
            } else {
                // Regular item
                let item_bounds = Rect2D::from_origin_size(
                    Vec2::new(menu_pos.x(), current_y),
                    Vec2::new(menu_width, item_height),
                );

                if self.popup_menu_item(label, *icon, *enabled, shortcut, item_bounds) {
                    clicked_action = Some(label.to_string());
                }
                current_y += item_height;
            }
        }

        // Pop z-index
        self.pop_z_index();

        // Block input for this popup
        self.block_input_for_popup(menu_bounds);

        // Handle click-outside-to-close
        if self.input.mouse_clicked(mouse_button::LEFT) && !menu_bounds.contains(self.input.mouse_pos) {
            // Restore old bounds and close
            self.popup_bounds = old_popup_bounds;
            return clicked_action;
        }

        // Handle Escape-to-close
        if self.input.key_pressed(crate::input::KeyCode::Escape) {
            self.popup_bounds = old_popup_bounds;
            return None;
        }

        // Restore old popup bounds
        self.popup_bounds = old_popup_bounds;

        clicked_action
    }

    /// Get the menu item height for layout.
    pub fn menu_item_height(&self) -> f32 {
        self.style.menu_item_height
    }
}
