//! High-level popup API: context_menu, dropdown, modal, menu_bar_dropdown.

use katla_math::{Color, Rect2D, Vec2};

use super::super::state::WidgetState;
use super::super::{z_index, UiContext};
use super::{Popup, PopupPosition, PopupStyle};

impl UiContext {
    /// Show a popup with custom configuration.
    ///
    /// Returns `Some(R)` if the popup was open, containing the closure's return value.
    /// Returns `None` if the popup is not open.
    pub fn popup<F, R>(&mut self, config: Popup, content: F) -> Option<R>
    where
        F: FnOnce(&mut Self) -> R,
    {
        let popup_id = self.generate_id(&config.id);

        // Check if this popup is open
        let is_open = self.popup_id == Some(popup_id);
        if !is_open {
            return None;
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
        let initial_bounds = Rect2D::from_origin_size(
            position,
            Vec2::new(self.popup_width, self.style.menu_item_height),
        );
        self.popup_bounds = Some(initial_bounds);

        // Run content closure
        let result = content(self);

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

        // Handle close behavior
        self.handle_popup_close(&config, final_bounds);

        // Clean up
        self.pop_clip();
        self.pop_id();
        self.pop_z_index();

        Some(result)
    }

    /// Context menu at cursor position (closure-based).
    ///
    /// Opens when right-click is detected anywhere.
    pub fn context_menu<F, R>(&mut self, id: &str, content: F) -> Option<R>
    where
        F: FnOnce(&mut Self) -> R,
    {
        // Auto-open on right-click
        self.open_context_menu(id);

        // Show popup if open
        self.popup(Popup::new(id).at_cursor(), content)
    }

    /// Dropdown below a trigger button (closure-based).
    ///
    /// Returns Some(R) when dropdown is open, None when closed.
    pub fn dropdown<F, R>(&mut self, id: &str, trigger: Rect2D, content: F) -> Option<R>
    where
        F: FnOnce(&mut Self) -> R,
    {
        self.popup(Popup::new(id).below_button(trigger), content)
    }

    /// Modal dialog (centered, blocks background).
    ///
    /// Use `open_popup(id)` to show the dialog.
    pub fn modal<F, R>(&mut self, id: &str, width: f32, height: f32, content: F) -> Option<R>
    where
        F: FnOnce(&mut Self) -> R,
    {
        self.popup(Popup::new(id).centered(width, height).modal(), content)
    }

    /// Menu bar dropdown with trigger button (closure-based).
    ///
    /// Draws the trigger button and handles the dropdown popup.
    pub fn menu_bar_dropdown<F, R>(
        &mut self,
        id: &str,
        label: &str,
        bounds: Rect2D,
        content: F,
    ) -> Option<R>
    where
        F: FnOnce(&mut Self) -> R,
    {
        let dropdown_id = self.generate_id(id);

        // Get open state
        let is_open = self.popup_id == Some(dropdown_id);

        // Draw trigger button and handle interaction
        let hovered = self.update_hover(dropdown_id, bounds);

        // Hover-to-switch when another popup is open
        if hovered
            && self.popup_id.is_some()
            && self.popup_id != Some(dropdown_id)
            && !self.popup_opened_this_frame
        {
            if let Some(other_id) = self.popup_id {
                self.storage.insert(other_id, WidgetState::DropdownOpen(false));
            }
            self.storage.insert(dropdown_id, WidgetState::DropdownOpen(true));
            self.popup_id = Some(dropdown_id);
            self.popup_bounds = Some(Rect2D::from_origin_size(
                Vec2::new(bounds.min.x(), bounds.max.y()),
                Vec2::new(bounds.width().max(self.style.menu_min_width), 200.0),
            ));
        }

        // Toggle on click
        if self.button_behavior(dropdown_id, bounds) {
            let new_open = !is_open;
            self.storage.insert(dropdown_id, WidgetState::DropdownOpen(new_open));
            if new_open {
                self.popup_id = Some(dropdown_id);
                self.popup_opened_this_frame = true;
                self.popup_bounds = Some(Rect2D::from_origin_size(
                    Vec2::new(bounds.min.x(), bounds.max.y()),
                    Vec2::new(bounds.width().max(self.style.menu_min_width), 200.0),
                ));
            } else {
                self.popup_id = None;
                self.popup_bounds = None;
            }
        }

        // Draw trigger button
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

        // Draw label (centered)
        let text_size = self.measure_text(label, self.style.font_size);
        let text_pos = Vec2::new(
            bounds.center().x() - text_size.x() * 0.5,
            bounds.center().y() - text_size.y() * 0.5,
        );
        self.draw_text(label, text_pos, self.style.button_text, self.style.font_size);

        // Show popup if open
        if is_open {
            self.dropdown(id, bounds, content)
        } else {
            None
        }
    }
}
