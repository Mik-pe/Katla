//! Basic UI widgets.
//!
//! Button, checkbox, slider, text input, label, and other standard widgets.

use katla_math::{Color, Rect2D, Vec2};

use crate::icons::ForkAwesome;
use crate::input::{mouse_button, KeyCode};
use crate::text::FontId;

use super::UiContext;

impl UiContext {
    // -------------------------------------------------------------------------
    // Widget Helpers
    // -------------------------------------------------------------------------

    /// Check if a widget is being hovered.
    ///
    /// This uses the imgui/egui approach: widgets at lower Z levels can only
    /// be hovered if the cursor is NOT inside a higher-level popup's bounds.
    /// This allows clicking outside popups to work correctly while still
    /// blocking hover for widgets covered by the popup.
    pub fn is_hovered(&self, bounds: Rect2D) -> bool {
        // Block hover if a popup consumed the click this frame (prevents click-through)
        if self.popup_consume_click {
            return false;
        }
        // If a popup is open and cursor is inside popup bounds,
        // block hover for widgets at lower Z levels
        if let Some(popup_bounds) = self.popup_bounds {
            if popup_bounds.contains(self.input.mouse_pos) && self.z_index < super::z_index::POPUP {
                return false;
            }
        }
        self.input.is_hovered(bounds) && self.active_id.is_none()
    }

    /// Update hover state for a widget.
    pub fn update_hover(&mut self, id: super::WidgetId, bounds: Rect2D) -> bool {
        let hovered = self.is_hovered(bounds);
        if hovered {
            self.hovered_id = Some(id);
            self.input.want_capture_mouse = true;
        }
        hovered
    }

    /// Handle button behavior (returns true if clicked).
    pub fn button_behavior(&mut self, id: super::WidgetId, bounds: Rect2D) -> bool {
        let hovered = self.update_hover(id, bounds);

        if hovered && self.input.mouse_pressed[mouse_button::LEFT] {
            self.active_id = Some(id);
        }

        let clicked = self.active_id == Some(id)
            && self.input.mouse_released[mouse_button::LEFT];

        // Only clear active_id if we're the active widget
        if clicked {
            self.active_id = None;
        }

        clicked
    }

    // -------------------------------------------------------------------------
    // Basic Widgets
    // -------------------------------------------------------------------------

    /// Draw a label (non-interactive text).
    pub fn label(&mut self, text: &str, bounds: Rect2D) {
        let text_size = self.measure_text(text, self.style.font_size);
        // Center text in bounds (top-left positioning)
        let text_pos = Vec2::new(
            bounds.min.x() + (bounds.width() - text_size.x()) * 0.5,
            bounds.center().y() - text_size.y() * 0.5,
        );
        self.draw_text(text, text_pos, self.style.text_color, self.style.font_size);
    }

    /// Draw a button. Returns true if clicked this frame.
    pub fn button(&mut self, id: &str, text: &str, bounds: Rect2D) -> bool {
        let widget_id = self.generate_id(id);

        let clicked = self.button_behavior(widget_id, bounds);

        // Determine colors based on state
        let (bg_color, text_color) = if self.active_id == Some(widget_id) {
            (self.style.button_active, self.style.button_text)
        } else if self.hovered_id == Some(widget_id) || self.is_hovered(bounds) {
            (self.style.button_hovered, self.style.button_text)
        } else {
            (self.style.button_normal, self.style.button_text)
        };

        // Draw button background
        self.draw_rect(bounds, bg_color);

        // Draw button text (centered, top-left positioning)
        let text_size = self.measure_text(text, self.style.font_size);
        let text_pos = Vec2::new(
            bounds.center().x() - text_size.x() * 0.5,
            bounds.center().y() - text_size.y() * 0.5,
        );
        self.draw_text(text, text_pos, text_color, self.style.font_size);

        clicked
    }

    /// Draw a checkbox. Returns true if value changed.
    pub fn checkbox(&mut self, id: &str, label: &str, checked: &mut bool, bounds: Rect2D) -> bool {
        let widget_id = self.generate_id(id);
        let clicked = self.button_behavior(widget_id, bounds);

        if clicked {
            *checked = !*checked;
        }

        // Draw checkbox background
        let check_size = bounds.height().min(20.0);
        let check_bounds = Rect2D::from_origin_size(
            Vec2::new(bounds.min.x(), bounds.center().y() - check_size * 0.5),
            Vec2::new(check_size, check_size),
        );

        let bg_color = if *checked {
            self.style.checkbox_check
        } else {
            self.style.checkbox_bg
        };
        self.draw_rect_border(check_bounds, bg_color, self.style.checkbox_border, 1.0);

        // Draw check icon if checked
        if *checked {
            let icon_size = check_size * 0.7;
            self.draw_icon_centered(ForkAwesome::CHECK, check_bounds, icon_size, Color::WHITE);
        }

        // Draw label (top-left positioning, vertically centered)
        let text_size = self.measure_text(label, self.style.font_size);
        let label_pos = Vec2::new(
            check_bounds.max.x() + 8.0,
            bounds.center().y() - text_size.y() * 0.5,
        );
        self.draw_text(
            label,
            label_pos,
            self.style.text_color,
            self.style.font_size,
        );

        clicked
    }

    /// Draw a slider. Returns true if value changed.
    pub fn slider(
        &mut self,
        id: &str,
        value: &mut f32,
        min: f32,
        max: f32,
        bounds: Rect2D,
    ) -> bool {
        let widget_id = self.generate_id(id);

        let hovered = self.update_hover(widget_id, bounds);
        let active = self.active_id == Some(widget_id);

        // Handle dragging
        let mut changed = false;

        if active {
            if self.input.mouse_down[mouse_button::LEFT] {
                let t =
                    ((self.input.mouse_pos.x() - bounds.min.x()) / bounds.width()).clamp(0.0, 1.0);
                let new_value = min + t * (max - min);
                if (new_value - *value).abs() > 0.0001 {
                    *value = new_value;
                    changed = true;
                }
            } else {
                self.active_id = None;
            }
        } else if hovered && self.input.mouse_pressed[mouse_button::LEFT] {
            self.active_id = Some(widget_id);
        }

        // Draw track
        let track_height = 4.0;
        let track_bounds = Rect2D::from_center_size(
            Vec2::new(bounds.center().x(), bounds.center().y()),
            Vec2::new(bounds.width(), track_height),
        );
        self.draw_rect(track_bounds, self.style.slider_track);

        // Draw grab
        let t = (*value - min) / (max - min);
        let grab_size = 12.0;
        let grab_pos = bounds.min.x() + t * (bounds.width() - grab_size);
        let grab_bounds = Rect2D::from_origin_size(
            Vec2::new(grab_pos, bounds.center().y() - grab_size * 0.5),
            Vec2::new(grab_size, grab_size),
        );
        let grab_color = if active {
            self.style.slider_grab_active
        } else if hovered {
            self.style.slider_grab_hovered
        } else {
            self.style.slider_grab
        };
        self.draw_rect(grab_bounds, grab_color);

        changed
    }

    /// Draw a separator line.
    pub fn separator(&mut self, bounds: Rect2D) {
        let y = bounds.center().y();
        self.draw_line(
            Vec2::new(bounds.min.x(), y),
            Vec2::new(bounds.max.x(), y),
            self.style.separator,
            1.0,
        );
    }

    /// Draw a text input field.
    ///
    /// Returns true if the text was modified this frame.
    /// Handles keyboard input, cursor positioning, and selection.
    pub fn text_input(&mut self, id: &str, text: &mut String, bounds: Rect2D) -> bool {
        let widget_id = self.generate_id(id);
        let hovered = self.update_hover(widget_id, bounds);

        // Focus on click
        if hovered && self.input.mouse_pressed[mouse_button::LEFT] {
            self.input.focused_id = Some(widget_id);
        }

        let focused = self.input.focused_id == Some(widget_id);
        let mut changed = false;

        // Handle keyboard input when focused
        if focused {
            self.input.want_capture_keyboard = true;

            // Process character input
            for &c in &self.input.characters {
                if c == '\x08' {
                    // Backspace
                    if !text.is_empty() {
                        text.pop();
                        changed = true;
                    }
                } else if c >= ' ' && text.len() < self.style.text_input_max_length {
                    // Printable character
                    text.push(c);
                    changed = true;
                }
            }

            // Handle special keys
            if self.input.key_pressed(KeyCode::Enter) {
                // Could trigger a callback here
            }
            if self.input.key_pressed(KeyCode::Escape) {
                self.input.focused_id = None;
            }
        }

        // Draw background
        self.draw_rect(bounds, self.style.input_bg);
        self.draw_rect_border(bounds, Color::TRANSPARENT, self.style.input_border, 1.0);

        // Draw text with clipping (top-left positioning, centered vertically)
        let padding = 4.0;
        let text_bounds = bounds.contract(padding);
        self.push_clip(text_bounds);

        let text_size = self.measure_text(text, self.style.font_size);
        let text_pos = Vec2::new(
            bounds.min.x() + padding,
            bounds.center().y() - text_size.y() * 0.5,
        );
        self.draw_text(text, text_pos, self.style.input_text, self.style.font_size);

        // Draw cursor when focused
        if focused {
            let cursor_x = text_pos.x() + self.measure_text(text, self.style.font_size).x();
            self.draw_line(
                Vec2::new(cursor_x, text_pos.y()),
                Vec2::new(cursor_x, text_pos.y() + self.style.font_size),
                self.style.input_cursor,
                1.0,
            );
        }

        self.pop_clip();

        changed
    }

    /// Draw a multiline text area.
    ///
    /// Returns true if the text was modified this frame.
    pub fn text_area(&mut self, id: &str, text: &mut String, bounds: Rect2D) -> bool {
        let widget_id = self.generate_id(id);
        let hovered = self.update_hover(widget_id, bounds);

        if hovered && self.input.mouse_pressed[mouse_button::LEFT] {
            self.input.focused_id = Some(widget_id);
        }

        let focused = self.input.focused_id == Some(widget_id);
        let mut changed = false;

        if focused {
            self.input.want_capture_keyboard = true;

            for &c in &self.input.characters {
                if c == '\x08' {
                    if !text.is_empty() {
                        text.pop();
                        changed = true;
                    }
                } else if (c >= ' ' || c == '\n') && text.len() < self.style.text_area_max_length {
                    text.push(c);
                    changed = true;
                }
            }
        }

        // Draw background
        self.draw_rect(bounds, self.style.input_bg);
        self.draw_rect_border(bounds, Color::TRANSPARENT, self.style.input_border, 1.0);

        // Draw text with clipping
        let padding = 4.0;
        self.push_clip(bounds.contract(padding));

        let mut y = bounds.min.y() + padding;
        for line in text.split('\n') {
            if y + self.style.font_size > bounds.min.y() + padding && y < bounds.max.y() - padding {
                self.draw_text(
                    line,
                    Vec2::new(bounds.min.x() + padding, y),
                    self.style.input_text,
                    self.style.font_size,
                );
            }
            y += self.style.font_size + 2.0;
        }

        self.pop_clip();

        changed
    }

    // -------------------------------------------------------------------------
    // Selectable
    // -------------------------------------------------------------------------

    /// Draw a selectable item with selection state.
    ///
    /// Returns true if clicked this frame.
    /// The `selected` parameter controls whether the item is highlighted as selected.
    pub fn selectable(&mut self, id: &str, label: &str, selected: bool, bounds: Rect2D) -> bool {
        let widget_id = self.generate_id(id);
        let clicked = self.button_behavior(widget_id, bounds);

        // Determine colors based on state
        let bg_color = if selected {
            self.style.selectable_selected
        } else if self.active_id == Some(widget_id) {
            self.style.menu_active
        } else if self.hovered_id == Some(widget_id) || self.is_hovered(bounds) {
            self.style.selectable_hovered
        } else {
            Color::TRANSPARENT
        };

        // Draw background
        if bg_color != Color::TRANSPARENT {
            self.draw_rect(bounds, bg_color);
        }

        // Draw label (top-left positioning, centered vertically)
        let text_size = self.measure_text(label, self.style.font_size);
        let text_pos = Vec2::new(
            bounds.min.x() + self.style.menu_padding,
            bounds.center().y() - text_size.y() * 0.5,
        );
        self.draw_text(label, text_pos, self.style.text_color, self.style.font_size);

        clicked
    }

    /// Draw a toggle button with an optional check icon when enabled.
    ///
    /// Returns true if clicked this frame.
    /// Colors are passed as parameters to allow theme customization.
    ///
    /// TODO: Consider using a ToggleStyle struct for the color parameters.
    #[allow(clippy::too_many_arguments)]
    pub fn toggle_button(&mut self, id: &str, label: &str, checked: bool, bounds: Rect2D, checked_color: Color, unchecked_color: Color, text_color: Color) -> bool {
        let clicked = self.button(id, "", bounds);

        let bg_color = if checked { checked_color } else { unchecked_color };
        self.draw_rect(bounds, bg_color);

        // Draw check icon and label
        let font_size = self.style.font_size;
        let icon_size = font_size;
        let padding = font_size;
        let text_x = bounds.min.x() + padding;
        let text_y = bounds.min.y() + 6.0;

        if checked {
            let check_icon = ForkAwesome::CHECK;
            self.draw_icon(check_icon, Vec2::new(text_x, text_y), icon_size, text_color);
            self.draw_text(label, Vec2::new(text_x + icon_size + 4.0, text_y), text_color, font_size);
        } else {
            // Reserve space for alignment
            self.draw_text(label, Vec2::new(text_x + icon_size + 4.0, text_y), text_color, font_size);
        }

        clicked
    }

    // -------------------------------------------------------------------------
    // Container Widgets
    // -------------------------------------------------------------------------

    /// Begin a window container.
    ///
    /// Returns a WindowState for window information.
    /// Call `end_window()` after adding contents.
    pub fn begin_window(&mut self, id: &str, bounds: Rect2D) -> super::WindowState {
        self.begin_window_with_title(id, None, bounds)
    }

    /// Begin a window container with an optional title bar.
    ///
    /// If title is provided, draws a title bar at the top.
    /// Returns a WindowState for window information.
    /// Call `end_window()` after adding contents.
    pub fn begin_window_with_title(
        &mut self,
        id: &str,
        title: Option<&str>,
        bounds: Rect2D,
    ) -> super::WindowState {
        let window_id = self.generate_id(id);

        // Title bar height
        let title_height = if title.is_some() { 25.0 } else { 0.0 };

        // Draw window background
        self.draw_rect(bounds, self.style.window_bg);

        // Draw title bar if provided
        if let Some(title_text) = title {
            let title_bounds =
                Rect2D::from_origin_size(bounds.min, Vec2::new(bounds.width(), title_height));
            self.draw_rect(title_bounds, self.style.window_title_bg);

            // Draw title text (top-left positioning, centered vertically in title bar)
            let text_size = self.measure_text(title_text, self.style.font_size);
            let text_pos = Vec2::new(
                bounds.min.x() + self.style.window_padding,
                bounds.min.y() + (title_height - text_size.y()) * 0.5,
            );
            self.draw_text(
                title_text,
                text_pos,
                self.style.text_color,
                self.style.font_size,
            );
        }

        // Draw border around entire window
        self.draw_rect_border(
            bounds.contract(1.0),
            self.style.window_bg,
            self.style.window_border,
            1.0,
        );

        // Content area starts below title bar
        let content_top = bounds.min.y() + title_height;
        let content_bounds = Rect2D::new(Vec2::new(bounds.min.x(), content_top), bounds.max);
        self.push_clip(content_bounds);

        super::WindowState {
            id: window_id,
            bounds,
            content_cursor: Vec2::new(
                bounds.min.x() + self.style.window_padding,
                content_top + self.style.window_padding,
            ),
            title_height,
        }
    }

    /// End a window container.
    pub fn end_window(&mut self) {
        self.pop_clip();
    }

    /// Begin a collapsible header/panel.
    ///
    /// Returns true if the header is expanded.
    pub fn begin_header(&mut self, id: &str, label: &str, open: &mut bool, bounds: Rect2D) -> bool {
        let widget_id = self.generate_id(id);

        // Click to toggle
        if self.button_behavior(widget_id, bounds) {
            *open = !*open;
        }

        // Draw header background
        let bg_color = if *open {
            self.style.window_title_bg_active
        } else {
            self.style.window_title_bg
        };
        self.draw_rect(bounds, bg_color);

        // Draw expand/collapse icon
        let icon = if *open {
            ForkAwesome::CHEVRON_DOWN
        } else {
            ForkAwesome::CHEVRON_RIGHT
        };
        let icon_size = self.style.font_size;
        let icon_pos = Vec2::new(bounds.min.x() + 4.0, bounds.center().y() - icon_size * 0.5);
        self.draw_icon_aligned(
            icon,
            icon_pos,
            icon_size,
            self.style.text_color,
            FontId::DEFAULT,
        );

        // Draw label text after icon
        let text_size = self.measure_text(label, self.style.font_size);
        let text_pos = Vec2::new(
            bounds.min.x() + icon_size + 8.0,
            bounds.center().y() - text_size.y() * 0.5,
        );
        self.draw_text(label, text_pos, self.style.text_color, self.style.font_size);

        *open
    }

    /// Begin a child region with clipping.
    ///
    /// Returns the content area bounds.
    pub fn begin_child(&mut self, _id: &str, bounds: Rect2D) -> Rect2D {
        // Draw background
        self.draw_rect(bounds, self.style.window_bg);

        // Push clip
        self.push_clip(bounds);

        // Return content area (with padding)
        bounds.contract(self.style.window_padding)
    }

    /// End a child region.
    pub fn end_child(&mut self) {
        self.pop_clip();
    }

    // -------------------------------------------------------------------------
    // Utility Widgets
    // -------------------------------------------------------------------------

    /// Draw a progress bar.
    pub fn progress_bar(&mut self, progress: f32, bounds: Rect2D, overlay: Option<&str>) {
        let progress_clamped = progress.clamp(0.0, 1.0);

        // Background
        self.draw_rect(bounds, self.style.slider_track);

        // Fill
        if progress_clamped > 0.0 {
            let fill_width = bounds.width() * progress_clamped;
            let fill_bounds =
                Rect2D::from_origin_size(bounds.min, Vec2::new(fill_width, bounds.height()));
            self.draw_rect(fill_bounds, self.style.slider_grab);
        }

        // Overlay text
        if let Some(text) = overlay {
            let text_size = self.measure_text(text, self.style.font_size);
            let text_pos = Vec2::new(
                bounds.center().x() - text_size.x() * 0.5,
                bounds.center().y() - text_size.y() * 0.5,
            );
            self.draw_text(text, text_pos, self.style.text_color, self.style.font_size);
        }
    }

    /// Draw a tooltip at the current mouse position.
    pub fn tooltip(&mut self, text: &str) {
        let padding = 4.0;
        let text_size = self.measure_text(text, self.style.font_size);
        let tip_size = Vec2::new(text_size.x() + padding * 2.0, text_size.y() + padding * 2.0);

        // Position near mouse
        let mut tip_pos = self.input.mouse_pos + Vec2::new(10.0, 10.0);

        // Keep on screen
        if tip_pos.x() + tip_size.x() > self.screen_size.x() {
            tip_pos = Vec2::new(tip_pos.x() - tip_size.x() - 20.0, tip_pos.y());
        }
        if tip_pos.y() + tip_size.y() > self.screen_size.y() {
            tip_pos = Vec2::new(tip_pos.x(), tip_pos.y() - tip_size.y() - 20.0);
        }

        let bounds = Rect2D::from_origin_size(tip_pos, tip_size);

        // Draw tooltip
        self.draw_rect(bounds, self.style.window_bg);
        self.draw_rect_border(bounds, Color::TRANSPARENT, self.style.border, 1.0);
        self.draw_text(
            text,
            Vec2::new(tip_pos.x() + padding, tip_pos.y() + padding),
            self.style.text_color,
            self.style.font_size,
        );
    }

    /// Draw a color preview rectangle.
    pub fn color_rect(&mut self, color: Color, bounds: Rect2D) {
        self.draw_rect(bounds, color);
        self.draw_rect_border(bounds, Color::TRANSPARENT, self.style.border, 1.0);
    }

    /// Draw an image/texture in the given bounds.
    ///
    /// The texture is stretched to fill the bounds.
    /// Use UV rect to display a portion of the texture.
    pub fn image(
        &mut self,
        texture: crate::TextureId,
        bounds: Rect2D,
        uv: Option<Rect2D>,
        tint: Option<Color>,
    ) {
        let uv_rect = uv.unwrap_or(Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0)));
        let color = tint.unwrap_or(Color::WHITE);
        self.draw_list.set_clip(self.clip_rect());
        self.draw_list
            .add_textured_rect(bounds, uv_rect, color, texture);
    }

    /// Draw an image with a border (useful for viewport frames).
    pub fn image_bordered(
        &mut self,
        texture: crate::TextureId,
        bounds: Rect2D,
        uv: Option<Rect2D>,
        tint: Option<Color>,
        border_color: Color,
    ) {
        self.image(texture, bounds, uv, tint);
        self.draw_rect_border(bounds, Color::TRANSPARENT, border_color, 1.0);
    }

    /// Draw a real-time line graph.
    ///
    /// Values should be ordered oldest to newest (left to right).
    /// The graph will auto-scale if min/max not provided in options.
    pub fn graph(
        &mut self,
        id: &str,
        label: Option<&str>,
        values: &[f32],
        bounds: Rect2D,
        options: Option<super::GraphOptions>,
    ) {
        let _ = id; // ID reserved for future interactivity
        let opts = options.unwrap_or_default();

        // Handle empty values case
        if values.is_empty() {
            self.draw_rect(bounds, opts.bg_color);
            if let Some(label_text) = label {
                let text_pos = Vec2::new(bounds.min.x() + 5.0, bounds.min.y() + 5.0);
                self.draw_text(
                    label_text,
                    text_pos,
                    self.style.text_color,
                    self.style.font_size,
                );
            }
            return;
        }

        // Calculate min/max
        let min_val = opts
            .min_value
            .unwrap_or_else(|| values.iter().cloned().fold(f32::INFINITY, f32::min));
        let max_val = opts
            .max_value
            .unwrap_or_else(|| values.iter().cloned().fold(f32::NEG_INFINITY, f32::max));

        // Ensure we have a valid range
        let range = if (max_val - min_val).abs() < 0.001 {
            1.0 // Avoid division by zero
        } else {
            max_val - min_val
        };

        // Layout: label area at top, graph area below
        let label_height = if label.is_some() { 18.0 } else { 0.0 };
        let padding = 3.0;

        let graph_bounds = Rect2D::new(
            Vec2::new(
                bounds.min.x() + padding,
                bounds.min.y() + label_height + padding,
            ),
            Vec2::new(bounds.max.x() - padding, bounds.max.y() - padding),
        );

        // 1. Draw background
        self.draw_rect(bounds, opts.bg_color);

        // 2. Draw label if provided
        if let Some(label_text) = label {
            let text_pos = Vec2::new(bounds.min.x() + 5.0, bounds.min.y() + 2.0);
            self.draw_text(
                label_text,
                text_pos,
                self.style.text_color,
                self.style.font_size,
            );
        }

        // 3. Draw grid lines (horizontal)
        if let Some(grid_color) = opts.grid_color {
            if graph_bounds.height() > 0.0 && opts.grid_lines > 0 {
                for i in 1..opts.grid_lines {
                    let t = i as f32 / opts.grid_lines as f32;
                    let y = graph_bounds.max.y() - t * graph_bounds.height();
                    self.draw_line(
                        Vec2::new(graph_bounds.min.x(), y),
                        Vec2::new(graph_bounds.max.x(), y),
                        grid_color,
                        1.0,
                    );
                }
            }
        }

        // Skip drawing if graph area is too small
        if graph_bounds.width() < 2.0 || graph_bounds.height() < 2.0 || values.len() < 2 {
            return;
        }

        // 4. Convert values to screen coordinates
        let points: Vec<Vec2> = values
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                let t = if values.len() > 1 {
                    i as f32 / (values.len() - 1) as f32
                } else {
                    0.5
                };
                let x = graph_bounds.min.x() + t * graph_bounds.width();
                let normalized = ((v - min_val) / range).clamp(0.0, 1.0);
                let y = graph_bounds.max.y() - normalized * graph_bounds.height();
                Vec2::new(x, y)
            })
            .collect();

        // 5. Draw filled area under the line (as vertical strips)
        if let Some(fill_color) = opts.fill_color {
            let bottom_y = graph_bounds.max.y();

            self.push_clip(graph_bounds);

            // Draw vertical quads between each pair of adjacent points
            for i in 0..points.len().saturating_sub(1) {
                let p0 = points[i];
                let p1 = points[i + 1];

                // Create a quad: top-left, top-right, bottom-right, bottom-left
                self.draw_list.add_convex_poly(
                    &[
                        Vec2::new(p0.x(), p0.y()),   // top-left
                        Vec2::new(p1.x(), p1.y()),   // top-right
                        Vec2::new(p1.x(), bottom_y), // bottom-right
                        Vec2::new(p0.x(), bottom_y), // bottom-left
                    ],
                    fill_color,
                );
            }

            self.pop_clip();
        }

        // 6. Draw the line segments
        self.push_clip(graph_bounds);
        for i in 0..points.len().saturating_sub(1) {
            self.draw_line(
                points[i],
                points[i + 1],
                opts.line_color,
                opts.line_thickness,
            );
        }
        self.pop_clip();

        // 7. Draw current value text
        if opts.show_value {
            if let Some(&last_val) = values.last() {
                let value_text = format!("{:.1}", last_val);
                let text_size = self.measure_text(&value_text, self.style.font_size);
                let text_pos = Vec2::new(
                    graph_bounds.max.x() - text_size.x() - 5.0,
                    graph_bounds.min.y() + 2.0,
                );
                self.draw_text(&value_text, text_pos, opts.line_color, self.style.font_size);
            }
        }
    }
}
