//! High-level popup API: context_menu, dropdown, modal, popup.
//!
//! All functions use `&mut bool` for open state, allowing the caller to control
//! when popups open and close.

use katla_math::{Color, Rect2D, Vec2};

use super::super::{z_index, UiContext};
use super::{Popup, PopupPosition, PopupStyle};

impl UiContext {
    /// Show a popup with custom configuration.
    ///
    /// Returns `true` if the popup was open (and rendered), `false` if not.
    ///
    /// The `open` parameter controls whether the popup is shown. Set it to `true` to open,
    /// and the closure can set it to `false` to close.
    pub fn popup<F>(&mut self, config: Popup, open: &mut bool, content: F) -> bool
    where
        F: FnOnce(&mut Self, &mut bool),
    {
        // Use stable ID for popups - they need consistent IDs across frames
        let popup_id = self.make_stable_id(&config.id);

        // Check if we should render
        if !*open {
            // Clear popup state when closed
            if self.popup_id == Some(popup_id) {
                self.popup_id = None;
                self.popup_position = None;
                self.popup_bounds = None;
            }
            return false;
        }

        // Mark this popup as open
        let was_just_opened = self.popup_id != Some(popup_id);
        self.popup_id = Some(popup_id);
        if was_just_opened {
            self.popup_opened_this_frame = true;
            self.active_id = None;
            self.focused_id = None;

            // Capture mouse position for AtCursor popups
            if matches!(config.position, PopupPosition::AtCursor) {
                self.popup_position = Some(self.input.mouse_pos);
            }
        }

        // Determine position based on config
        let position = self.calculate_popup_position(&config);

        // Determine z-index based on style
        let z = match config.style {
            PopupStyle::Modal => z_index::TOOLTIP,
            _ => z_index::POPUP,
        };

        // Initialize popup state
        self.popup_content_bounds = None;
        self.popup_cursor = position;
        self.popup_width = self.style.menu_min_width;

        // For modal, draw dark overlay first
        if config.style == PopupStyle::Modal {
            let screen_bounds = Rect2D::from_size(self.screen_size);
            self.draw_rect(screen_bounds, Color::new(0.0, 0.0, 0.0, 0.5));
        }

        // Set up rendering state
        self.push_z_index(z);

        // Set clip (full screen for auto-sizing, bounds for fixed)
        let clip = match config.position {
            PopupPosition::Fixed(bounds) => bounds,
            _ => Rect2D::new(Vec2::new(0.0, 0.0), self.screen_size),
        };
        self.push_clip_absolute(clip);
        self.push_id(&config.id);

        // Store initial popup bounds for get_popup_bounds()
        // For Centered popups (modals), use the specified size immediately
        let initial_bounds = match config.position {
            PopupPosition::Centered { width, height } => {
                Rect2D::from_origin_size(position, Vec2::new(width, height))
            }
            _ => Rect2D::from_origin_size(
                position,
                Vec2::new(self.popup_width, self.style.menu_item_height),
            ),
        };
        self.popup_bounds = Some(initial_bounds);

        // Run content closure
        content(self, open);

        // Calculate final bounds from tracked content
        let final_bounds = self.calculate_final_popup_bounds(&config, position);

        // Draw background at lower z-index (so content appears on top)
        if config.style != PopupStyle::Tooltip {
            self.pop_z_index();
            self.push_z_index(z - 1);
            self.draw_popup_background(final_bounds, &config.style);
            self.pop_z_index();
            self.push_z_index(z);
        }

        // Store final bounds for click-outside detection
        self.popup_bounds = Some(final_bounds);

        // Handle close behavior - but skip if just opened this frame
        // (the opening click shouldn't immediately close the popup)
        if !was_just_opened && self.handle_popup_close(&config, final_bounds) {
            *open = false;
        }

        // If the closure closed the popup, clear our state
        if !*open {
            self.popup_id = None;
            self.popup_position = None;
            self.popup_bounds = None;
        }

        // Clean up
        self.pop_clip();
        self.pop_id();
        self.pop_z_index();

        true
    }

    /// Context menu at cursor position.
    ///
    /// Opens when `open` is set to `true`. The position is captured from the mouse
    /// when first opened.
    ///
    /// # Example
    /// ```ignore
    /// let mut menu_open = false;
    ///
    /// // Open on right-click
    /// if ui.input.mouse_clicked(RIGHT) && !ui.has_open_popup() {
    ///     menu_open = true;
    /// }
    ///
    /// ui.context_menu("menu", &mut menu_open, |ui, open| {
    ///     if ui.menu_item_clicked("Option") {
    ///         *open = false;  // Close menu
    ///     }
    /// });
    /// ```
    pub fn context_menu<F>(&mut self, id: &str, open: &mut bool, content: F)
    where
        F: FnOnce(&mut Self, &mut bool),
    {
        self.popup(Popup::new(id).at_cursor(), open, content);
    }

    pub(crate) fn context_menu_at<F>(&mut self, id: &str, pos: Vec2, open: &mut bool, content: F)
    where
        F: FnOnce(&mut Self, &mut bool),
    {
        self.popup(Popup::new(id).at_position(pos), open, content);
    }

    pub(crate) fn dropdown<F>(&mut self, id: &str, trigger: Rect2D, open: &mut bool, content: F)
    where
        F: FnOnce(&mut Self, &mut bool),
    {
        self.popup(Popup::new(id).below_button(trigger), open, content);
    }

    pub fn modal<F>(
        &mut self,
        id: &str,
        width: f32,
        height: f32,
        open: &mut bool,
        content: F,
    ) where
        F: FnOnce(&mut Self, &mut bool),
    {
        self.popup(
            Popup::new(id).centered(width, height).modal(),
            open,
            content,
        );
    }

    /// Menu bar dropdown with trigger button.
    ///
    /// Draws the trigger button and handles the dropdown popup.
    ///
    /// # Example
    /// ```ignore
    /// let mut file_open = false;
    /// ui.menu_bar_dropdown("file", "File", button_bounds, &mut file_open, |ui, open| {
    ///     if ui.menu_item_clicked("New") {
    ///         *open = false;
    ///     }
    /// });
    /// ```
    pub fn menu_bar_dropdown<F>(
        &mut self,
        id: &str,
        label: &str,
        bounds: Rect2D,
        open: &mut bool,
        content: F,
    ) where
        F: FnOnce(&mut Self, &mut bool),
    {
        // Use stable ID for menus - they're always at the same position
        let dropdown_id = self.make_stable_id(id);

        // Check if this dropdown should close (from hover-to-switch in this frame)
        if self.menu_bar_close_id == Some(dropdown_id) {
            *open = false;
            self.menu_bar_close_id = None; // Clear so we don't close again
        }

        // Draw trigger button and handle interaction
        let hovered = self.update_hover(dropdown_id, bounds);

        // Toggle on click
        // Note: On release, we use self.input.is_hovered() directly instead of self.is_hovered()
        // because self.is_hovered() returns false when active_id is set (which it is during press).
        let clicked = if hovered && self.input.mouse_pressed[crate::input::mouse_button::LEFT] {
            self.active_id = Some(dropdown_id);
            false
        } else if self.active_id == Some(dropdown_id)
            && self.input.mouse_released[crate::input::mouse_button::LEFT]
        {
            self.active_id = None;
            // Check if mouse is still over button using raw input check (bypasses active_id block)
            self.input.is_hovered(bounds)
        } else {
            false
        };

        if clicked {
            *open = !*open;
        }

        // Hover-to-switch when another dropdown is open
        // When hovering over a DIFFERENT dropdown while one is open, close the
        // current popup and open this one. This provides standard menu bar behavior.
        // Note: We check popup_id != dropdown_id to avoid switching when hovering
        // the same dropdown that's already open.
        if hovered
            && self.popup_id.is_some()
            && self.popup_id != Some(dropdown_id)
            && !self.popup_opened_this_frame
        {
            // Tell the current popup's dropdown to close
            self.menu_bar_close_id = self.popup_id;
            // Close the current popup (this allows the new dropdown to take over)
            self.close_current_popup();
            *open = true;
        }

        // Draw trigger button
        let bg_color = if *open {
            self.style.menu_active
        } else if self.active_id == Some(dropdown_id) {
            self.style.button_active
        } else if hovered {
            self.style.button_hovered
        } else {
            self.style.button_normal
        };
        self.draw_rect(bounds, bg_color);

        // Draw label (centered)
        let text_size = self.measure_text(label, self.style.font_size);
        let text_pos = Vec2::new(
            bounds.center().x() - text_size.x() * 0.5,
            bounds.center().y() - text_size.y() * 0.5,
        );
        self.draw_text(
            label,
            text_pos,
            self.style.button_text,
            self.style.font_size,
        );

        // Show popup if open
        if *open {
            self.dropdown(id, bounds, open, content);
        }
    }
}
