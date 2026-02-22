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

// =============================================================================
// POPUP CONFIGURATION
// =============================================================================

/// Popup positioning mode.
#[derive(Debug, Clone, Copy)]
pub enum PopupPosition {
    /// Position at current cursor (context menu style).
    AtCursor,
    /// Position at a specific screen position.
    AtPosition(Vec2),
    /// Position below a trigger button (dropdown style).
    BelowButton(Rect2D),
    /// Fixed position and size (pre-sized popup).
    Fixed(Rect2D),
    /// Centered on screen with specified dimensions (modal style).
    Centered { width: f32, height: f32 },
}

/// Popup visual style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupStyle {
    /// Standard menu with shadow and border.
    Menu,
    /// Modal dialog with dark background overlay.
    Modal,
    /// Tooltip style (no shadow).
    Tooltip,
}

/// Popup close behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseBehavior {
    /// Close when clicking outside the popup.
    ClickOutside,
    /// Only close programmatically (modal behavior).
    ExplicitOnly,
}

/// Builder for popup configuration.
///
/// Use the builder methods to configure position, style, and behavior,
/// then pass to `ui.popup()` or use convenience wrappers.
#[derive(Debug, Clone)]
pub struct Popup {
    pub(crate) id: String,
    pub(crate) position: PopupPosition,
    pub(crate) style: PopupStyle,
    pub(crate) close_behavior: CloseBehavior,
    /// Whether to show a caret icon on the trigger button (for dropdowns).
    pub(crate) show_caret: bool,
}

impl Popup {
    /// Create a new popup configuration with the given ID.
    ///
    /// Default configuration:
    /// - Position: AtCursor
    /// - Style: Menu
    /// - Close behavior: ClickOutside
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            position: PopupPosition::AtCursor,
            style: PopupStyle::Menu,
            close_behavior: CloseBehavior::ClickOutside,
            show_caret: true,
        }
    }

    /// Position popup at the current cursor position.
    pub fn at_cursor(mut self) -> Self {
        self.position = PopupPosition::AtCursor;
        self
    }

    /// Position popup at a specific screen position.
    pub fn at_position(mut self, pos: Vec2) -> Self {
        self.position = PopupPosition::AtPosition(pos);
        self
    }

    /// Position popup below a trigger button.
    pub fn below_button(mut self, trigger: Rect2D) -> Self {
        self.position = PopupPosition::BelowButton(trigger);
        self
    }

    /// Use fixed position and size.
    pub fn fixed(mut self, bounds: Rect2D) -> Self {
        self.position = PopupPosition::Fixed(bounds);
        self
    }

    /// Center on screen with specified dimensions.
    pub fn centered(mut self, width: f32, height: f32) -> Self {
        self.position = PopupPosition::Centered { width, height };
        self
    }

    /// Use standard menu style (shadow, border).
    pub fn menu(mut self) -> Self {
        self.style = PopupStyle::Menu;
        self
    }

    /// Use modal style (dark overlay, centered, explicit close only).
    pub fn modal(mut self) -> Self {
        self.style = PopupStyle::Modal;
        self.close_behavior = CloseBehavior::ExplicitOnly;
        self
    }

    /// Use tooltip style (no shadow).
    pub fn tooltip(mut self) -> Self {
        self.style = PopupStyle::Tooltip;
        self
    }

    /// Set close behavior.
    pub fn close_behavior(mut self, behavior: CloseBehavior) -> Self {
        self.close_behavior = behavior;
        self
    }

    /// Show/hide caret icon on trigger button (for dropdowns).
    pub fn show_caret(mut self, show: bool) -> Self {
        self.show_caret = show;
        self
    }
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

    /// Get the bounds of the current popup.
    pub fn get_popup_bounds(&self) -> Rect2D {
        self.popup_bounds.unwrap_or_else(|| Rect2D::from_size(Vec2::new(0.0, 0.0)))
    }

    /// Open a context menu at the current mouse position.
    ///
    /// Call this when detecting a right-click on an area.
    /// Returns true if the menu was just opened.
    pub fn open_context_menu(&mut self, id: &str) -> bool {
        let context_id = self.generate_id(id);

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
        if let Some(popup_id) = self.popup_id {
            self.storage
                .insert(popup_id, WidgetState::DropdownOpen(false));
        }
        self.popup_id = None;
        self.popup_bounds = None;
    }

    pub fn track_popup_item(&mut self, item_bounds: Rect2D) {
        self.popup_content_bounds = Some(match self.popup_content_bounds {
            None => item_bounds,
            Some(existing) => Rect2D::new(
                Vec2::new(existing.min.x().min(item_bounds.min.x()), existing.min.y().min(item_bounds.min.y())),
                Vec2::new(existing.max.x().max(item_bounds.max.x()), existing.max.y().max(item_bounds.max.y())),
            ),
        });
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
    // Auto-positioning Popup Items (for use within popup closures)
    // -------------------------------------------------------------------------

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

    /// Get the menu item height for layout.
    pub fn menu_item_height(&self) -> f32 {
        self.style.menu_item_height
    }

    // =========================================================================
    // POPUP API
    // =========================================================================

    /// Show a popup with custom configuration.
    ///
    /// Returns `Some(R)` if the popup was open, containing the closure's return value.
    /// Returns `None` if the popup is not open.
    ///
    /// # Example
    /// ```ignore
    /// let result = ui.popup(Popup::new("menu").at_cursor(), |ui| {
    ///     if ui.menu_item_clicked("Option 1") { return "opt1"; }
    ///     if ui.menu_item_clicked("Option 2") { return "opt2"; }
    ///     "none"
    /// });
    /// ```
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
    /// Opens when right-click is detected anywhere. Use `open_context_menu_at()` to
    /// open programmatically at a specific position.
    ///
    /// # Example
    /// ```ignore
    /// ui.context_menu("entity_context", |ui| {
    ///     if ui.menu_item_clicked("Delete") {
    ///         // handle delete
    ///         ui.close_current_popup();
    ///     }
    /// });
    /// ```
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
    ///
    /// # Example
    /// ```ignore
    /// let result = ui.dropdown("options", trigger_bounds, |ui| {
    ///     if ui.menu_item_clicked("Option A") { return "a"; }
    ///     if ui.menu_item_clicked("Option B") { return "b"; }
    ///     "none"
    /// });
    /// ```
    pub fn dropdown<F, R>(&mut self, id: &str, trigger: Rect2D, content: F) -> Option<R>
    where
        F: FnOnce(&mut Self) -> R,
    {
        self.popup(Popup::new(id).below_button(trigger), content)
    }

    /// Modal dialog (centered, blocks background).
    ///
    /// Use `open_popup(id)` to show the dialog.
    ///
    /// # Example
    /// ```ignore
    /// if some_condition {
    ///     ui.open_popup("confirm");
    /// }
    ///
    /// ui.modal("confirm", 320.0, 150.0, |ui| {
    ///     ui.popup_text("Are you sure?");
    ///     if ui.popup_button("Yes") {
    ///         // handle confirm
    ///         ui.close_current_popup();
    ///     }
    /// });
    /// ```
    pub fn modal<F, R>(&mut self, id: &str, width: f32, height: f32, content: F) -> Option<R>
    where
        F: FnOnce(&mut Self) -> R,
    {
        self.popup(Popup::new(id).centered(width, height).modal(), content)
    }

    /// Menu bar dropdown with trigger button (closure-based).
    ///
    /// Draws the trigger button and handles the dropdown popup.
    /// Use `show_caret(false)` for menu bar items that don't need a caret.
    ///
    /// # Example
    /// ```ignore
    /// let bounds = Rect2D::from_origin_size(cursor, Vec2::new(60.0, 24.0));
    /// ui.menu_bar_dropdown("file_menu", "File", bounds, |ui| {
    ///     if ui.menu_item_clicked("New") { /* ... */ }
    ///     if ui.menu_item_clicked("Open...") { /* ... */ }
    ///     ui.menu_separator();
    ///     if ui.menu_item_clicked("Quit") { /* ... */ }
    /// });
    /// ```
    pub fn menu_bar_dropdown<F, R>(&mut self, id: &str, label: &str, bounds: Rect2D, content: F) -> Option<R>
    where
        F: FnOnce(&mut Self) -> R,
    {
        let dropdown_id = self.generate_id(id);

        // Get open state
        let is_open = self.popup_id == Some(dropdown_id);

        // Draw trigger button and handle interaction
        let hovered = self.update_hover(dropdown_id, bounds);

        // Hover-to-switch when another popup is open
        if hovered && self.popup_id.is_some() && self.popup_id != Some(dropdown_id) && !self.popup_opened_this_frame {
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

    /// Menu item with automatic positioning inside a popup.
    ///
    /// Returns true if clicked. Use inside `context_menu()`, `dropdown()`, or `modal()`.
    /// Items are positioned automatically - no manual bounds needed.
    ///
    /// # Example
    /// ```ignore
    /// ui.context_menu("menu", |ui| {
    ///     if ui.menu_item_clicked("Delete") {
    ///         delete_item();
    ///         ui.close_current_popup();
    ///     }
    /// });
    /// ```
    pub fn menu_item_clicked(&mut self, label: &str) -> bool {
        self.menu_item_clicked_ex(label, None, true, "")
    }

    /// Menu item with icon.
    pub fn menu_item_clicked_with_icon(&mut self, label: &str, icon: char) -> bool {
        self.menu_item_clicked_ex(label, Some(icon), true, "")
    }

    /// Menu item with icon, shortcut hint, and enabled state.
    pub fn menu_item_clicked_with_icon_and_shortcut(&mut self, label: &str, icon: char, enabled: bool, shortcut: &str) -> bool {
        self.menu_item_clicked_ex(label, Some(icon), enabled, shortcut)
    }

    /// Menu item with all options.
    fn menu_item_clicked_ex(&mut self, label: &str, icon: Option<char>, enabled: bool, shortcut: &str) -> bool {
        let item_height = self.style.menu_item_height;

        let item_bounds = Rect2D::from_origin_size(
            self.popup_cursor,
            Vec2::new(self.popup_width, item_height),
        );

        // Track for auto-sizing
        self.track_popup_item(item_bounds);

        // Use provided icon or default based on label
        let icon_char = icon.unwrap_or(ForkAwesome::ANGLE_RIGHT);

        // Draw and check click
        let clicked = self.draw_popup_item_contents(label, icon_char, enabled, item_bounds, shortcut);

        // Advance cursor
        self.popup_cursor = Vec2::new(self.popup_cursor.x(), self.popup_cursor.y() + item_height);

        clicked
    }

    /// Toggle menu item with automatic positioning.
    ///
    /// Shows checkmark when `checked` is true.
    pub fn toggle_menu_item_clicked(&mut self, label: &str, checked: bool) -> bool {
        let item_height = self.style.menu_item_height;

        let item_bounds = Rect2D::from_origin_size(
            self.popup_cursor,
            Vec2::new(self.popup_width, item_height),
        );

        // Track for auto-sizing
        self.track_popup_item(item_bounds);

        let hovered = self.is_hovered(item_bounds);

        // Hover background
        if hovered {
            self.draw_rect(item_bounds, self.style.menu_hovered);
        }

        // Checkmark or space
        let icon = if checked { ForkAwesome::CHECK } else { ' ' };
        let text_size = self.scaled_font_size(FontSize::Small);
        let text_y = item_bounds.min.y() + 6.0;

        self.draw_icon_aligned(
            icon,
            Vec2::new(item_bounds.min.x() + 8.0, text_y),
            12.0,
            self.style.text_color,
            FontId::DEFAULT,
        );

        // Label
        self.draw_text(
            label,
            Vec2::new(item_bounds.min.x() + 28.0, text_y),
            self.style.text_color,
            text_size,
        );

        // Click detection
        let clicked = hovered && self.input.mouse_clicked(mouse_button::LEFT);

        // Advance cursor
        self.popup_cursor = Vec2::new(self.popup_cursor.x(), self.popup_cursor.y() + item_height);

        clicked
    }

    /// Menu separator with automatic positioning.
    pub fn menu_separator(&mut self) {
        let separator_height = 8.0;

        let sep_bounds = Rect2D::from_origin_size(
            self.popup_cursor,
            Vec2::new(self.popup_width, separator_height),
        );

        // Track for auto-sizing
        self.track_popup_item(sep_bounds);

        // Draw line
        self.draw_line(
            Vec2::new(sep_bounds.min.x() + 8.0, sep_bounds.center().y()),
            Vec2::new(sep_bounds.max.x() - 8.0, sep_bounds.center().y()),
            self.style.separator,
            1.0,
        );

        // Advance cursor
        self.popup_cursor = Vec2::new(self.popup_cursor.x(), self.popup_cursor.y() + separator_height);
    }

    // =========================================================================
    // INTERNAL HELPERS
    // =========================================================================

    /// Calculate popup position based on config.
    fn calculate_popup_position(&self, config: &Popup) -> Vec2 {
        match config.position {
            PopupPosition::AtCursor => {
                // Get stored position or use current mouse position
                let popup_id = self.generate_id(&config.id);
                self.storage
                    .get(&popup_id)
                    .and_then(|s| {
                        if let WidgetState::ContextMenuPos(p) = s {
                            Some(*p)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(self.input.mouse_pos)
            }
            PopupPosition::AtPosition(pos) => pos,
            PopupPosition::BelowButton(trigger) => {
                Vec2::new(trigger.min.x(), trigger.max.y())
            }
            PopupPosition::Fixed(bounds) => bounds.min,
            PopupPosition::Centered { width, height } => {
                Vec2::new(
                    (self.screen_size.x() - width) * 0.5,
                    (self.screen_size.y() - height) * 0.5,
                )
            }
        }
    }

    /// Calculate final popup bounds from tracked content.
    fn calculate_final_popup_bounds(&self, config: &Popup, position: Vec2) -> Rect2D {
        match config.position {
            PopupPosition::Fixed(bounds) => bounds,
            PopupPosition::Centered { width, height } => {
                Rect2D::from_origin_size(position, Vec2::new(width, height))
            }
            _ => {
                // Auto-size from tracked content
                let content_bounds = self.popup_content_bounds.unwrap_or_else(|| {
                    Rect2D::from_origin_size(position, Vec2::new(self.style.menu_min_width, self.style.menu_item_height))
                });

                let min_width = self.style.menu_min_width;
                let min_height = self.style.menu_item_height;
                let final_width = content_bounds.width().max(min_width);
                let final_height = content_bounds.height().max(min_height);

                Rect2D::from_origin_size(content_bounds.min, Vec2::new(final_width, final_height))
            }
        }
    }

    /// Draw popup background (shadow + bg + border).
    fn draw_popup_background(&mut self, bounds: Rect2D, style: &PopupStyle) {
        // Shadow (not for tooltips)
        if *style != PopupStyle::Tooltip {
            let shadow_offset = Vec2::new(4.0, 4.0);
            let shadow_bounds = Rect2D::new(bounds.min + shadow_offset, bounds.max + shadow_offset);
            self.draw_rect(shadow_bounds, self.style.popup_shadow);
        }

        // Background
        self.draw_rect(bounds, self.style.popup_bg);

        // Border
        self.draw_rect_border(bounds, Color::TRANSPARENT, self.style.popup_border, 1.0);
    }

    /// Handle popup close behavior.
    fn handle_popup_close(&mut self, config: &Popup, bounds: Rect2D) {
        // Capture mouse when over popup
        if bounds.contains(self.input.mouse_pos) {
            self.input.want_capture_mouse = true;
        }

        // Handle click-outside-to-close
        if config.close_behavior == CloseBehavior::ClickOutside {
            if self.input.mouse_clicked(mouse_button::LEFT) && !bounds.contains(self.input.mouse_pos) {
                self.close_current_popup();
            }
        }

        // Handle Escape-to-close
        if self.input.key_pressed(crate::input::KeyCode::Escape) {
            self.close_current_popup();
        }

        // Capture keyboard for modals
        if config.style == PopupStyle::Modal {
            self.input.want_capture_keyboard = true;
        }
    }
}
