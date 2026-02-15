//! UI context - the main entry point for UI rendering.
//!
//! The `UiContext` manages all UI state and provides the immediate mode API
//! for building user interfaces.

use std::collections::HashMap;

use katla_math::{Color, Rect2D, Vec2};

use crate::draw_list::{DrawList, TextureId};
use crate::input::UiInputState;
use crate::style::UiStyle;
use crate::text::{FontId, FontSystem};

/// ID type for UI elements.
pub type WidgetId = u64;

/// Main context for immediate mode UI rendering.
///
/// This is the primary API for building UI. Typical usage:
/// 1. Call `begin()` at the start of each frame
/// 2. Call widget functions to build the UI
/// 3. Call `end()` to finalize and get the draw list
pub struct UiContext {
    /// The draw list being built this frame.
    draw_list: DrawList,
    /// Input state (updated externally).
    pub input: UiInputState,
    /// Style configuration.
    pub style: UiStyle,
    /// Font system for text rendering.
    pub fonts: FontSystem,
    /// Currently active font.
    current_font: FontId,
    /// Current screen size.
    screen_size: Vec2,
    /// Stack of clipping rectangles.
    clip_stack: Vec<Rect2D>,
    /// Stack of widget IDs for nesting.
    id_stack: Vec<WidgetId>,
    /// Counter for generating unique IDs.
    id_counter: u32,
    /// Storage for widget state (checkboxes, sliders, etc.).
    storage: HashMap<WidgetId, WidgetState>,
    /// Currently hovered widget.
    hovered_id: Option<WidgetId>,
    /// Currently active (pressed) widget.
    active_id: Option<WidgetId>,
    /// Whether we're inside a begin()/end() pair.
    in_frame: bool,
    /// Layout cursor for automatic positioning.
    cursor: Vec2,
    /// Current row height for layout.
    row_height: f32,
}

/// Persistent state for widgets.
#[derive(Debug, Clone)]
enum WidgetState {
    /// Checkbox state.
    Checkbox(bool),
    /// Slider value.
    Slider(f32),
    /// Text input content.
    TextInput(String),
    /// Window position.
    WindowPos(Vec2),
}

impl UiContext {
    /// Create a new UI context with the default dark theme.
    pub fn new() -> Self {
        Self {
            draw_list: DrawList::new(),
            input: UiInputState::new(),
            style: UiStyle::dark(),
            fonts: FontSystem::new(),
            current_font: FontId::DEFAULT,
            screen_size: Vec2::new(0.0, 0.0),
            clip_stack: Vec::new(),
            id_stack: Vec::new(),
            id_counter: 0,
            storage: HashMap::new(),
            hovered_id: None,
            active_id: None,
            in_frame: false,
            cursor: Vec2::new(0.0, 0.0),
            row_height: 0.0,
        }
    }

    /// Create a new UI context with a specific style.
    pub fn with_style(style: UiStyle) -> Self {
        Self {
            style,
            ..Self::new()
        }
    }

    /// Begin a new frame.
    ///
    /// Must be called before any widget functions.
    /// `screen_size` is the current window/render target size.
    pub fn begin(&mut self, screen_size: Vec2) {
        debug_assert!(!self.in_frame, "begin() called while already in frame");

        self.in_frame = true;
        self.screen_size = screen_size;
        self.draw_list.clear();
        self.id_stack.clear();
        self.id_counter = 0;
        self.hovered_id = None;
        self.cursor = Vec2::new(0.0, 0.0);
        self.row_height = 0.0;

        // Set initial clip to full screen
        self.clip_stack.clear();
        self.clip_stack.push(Rect2D::from_size(screen_size));
    }

    /// End the frame and get the draw list.
    ///
    /// After calling this, render the draw list using `UiRenderer`.
    pub fn end(&mut self) -> &DrawList {
        debug_assert!(self.in_frame, "end() called without begin()");

        self.draw_list.finalize();
        self.in_frame = false;

        // Clear hover if mouse was released
        if self.active_id.is_none() {
            // Mouse released, clear active
        }

        &self.draw_list
    }

    /// Get the current screen size.
    #[inline]
    pub fn screen_size(&self) -> Vec2 {
        self.screen_size
    }

    // -------------------------------------------------------------------------
    // ID Management
    // -------------------------------------------------------------------------

    /// Generate a unique ID for a widget.
    fn generate_id(&mut self, label: &str) -> WidgetId {
        let base = if let Some(&parent) = self.id_stack.last() {
            parent
        } else {
            0
        };

        // Simple hash combining parent ID with label
        let mut hash = base;
        for byte in label.bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
        }

        // Add counter to ensure uniqueness
        hash = hash.wrapping_add(self.id_counter as u64);
        self.id_counter += 1;

        hash
    }

    /// Push an ID onto the stack for nested widgets.
    pub fn push_id(&mut self, id: &str) {
        let id = self.generate_id(id);
        self.id_stack.push(id);
    }

    /// Pop an ID from the stack.
    pub fn pop_id(&mut self) {
        self.id_stack.pop();
    }

    // -------------------------------------------------------------------------
    // Clipping
    // -------------------------------------------------------------------------

    /// Get the current clip rectangle.
    #[inline]
    pub fn clip_rect(&self) -> Rect2D {
        *self.clip_stack.last().unwrap_or(&Rect2D::from_size(self.screen_size))
    }

    /// Push a new clip rectangle (intersection with current).
    pub fn push_clip(&mut self, rect: Rect2D) {
        let current = self.clip_rect();
        let clipped = current.intersection(&rect).unwrap_or(Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0)));
        self.clip_stack.push(clipped);
        self.draw_list.set_clip(clipped);
    }

    /// Pop a clip rectangle.
    pub fn pop_clip(&mut self) {
        if self.clip_stack.len() > 1 {
            self.clip_stack.pop();
            self.draw_list.set_clip(self.clip_rect());
        }
    }

    // -------------------------------------------------------------------------
    // Low-level Primitives
    // -------------------------------------------------------------------------

    /// Draw a solid-color rectangle.
    pub fn draw_rect(&mut self, bounds: Rect2D, color: Color) {
        self.draw_list.set_clip(self.clip_rect());
        self.draw_list.add_rect(bounds, color);
    }

    /// Draw a rectangle with a border.
    pub fn draw_rect_border(&mut self, bounds: Rect2D, fill: Color, border: Color, border_width: f32) {
        self.draw_rect(bounds, fill);

        // Top
        self.draw_rect(
            Rect2D::from_origin_size(bounds.min, Vec2::new(bounds.width(), border_width)),
            border,
        );
        // Bottom
        self.draw_rect(
            Rect2D::from_origin_size(
                Vec2::new(bounds.min.x(), bounds.max.y() - border_width),
                Vec2::new(bounds.width(), border_width),
            ),
            border,
        );
        // Left
        self.draw_rect(
            Rect2D::from_origin_size(bounds.min, Vec2::new(border_width, bounds.height())),
            border,
        );
        // Right
        self.draw_rect(
            Rect2D::from_origin_size(
                Vec2::new(bounds.max.x() - border_width, bounds.min.y()),
                Vec2::new(border_width, bounds.height()),
            ),
            border,
        );
    }

    /// Draw a line.
    pub fn draw_line(&mut self, start: Vec2, end: Vec2, color: Color, thickness: f32) {
        self.draw_list.set_clip(self.clip_rect());
        self.draw_list.add_line(start, end, color, thickness);
    }

    /// Draw text using the font system.
    ///
    /// Text is rendered as textured quads from the font atlas.
    /// If no font is loaded, draws placeholder rectangles.
    pub fn draw_text(&mut self, text: &str, position: Vec2, color: Color, size: f32) {
        let mut cursor_x = position.x();
        let cursor_y = position.y();

        for c in text.chars() {
            // Try to get cached glyph
            if let Some(glyph) = self.fonts.get_or_rasterize(self.current_font, c, size) {
                // Skip space (no visual)
                if c == ' ' {
                    cursor_x += glyph.advance;
                    continue;
                }

                // Calculate glyph position
                let glyph_pos = Vec2::new(
                    cursor_x + glyph.offset.x(),
                    cursor_y + glyph.offset.y() + size, // Adjust for baseline
                );

                let bounds = Rect2D::from_origin_size(glyph_pos, glyph.size);

                // Draw glyph as textured quad
                self.draw_list.set_clip(self.clip_rect());
                self.draw_list.add_textured_rect(
                    bounds,
                    glyph.uv_rect,
                    color,
                    TextureId::FONT_ATLAS,
                );

                cursor_x += glyph.advance;
            } else {
                // No glyph available - draw placeholder
                let placeholder_size = Vec2::new(size * 0.6, size);
                let bounds = Rect2D::from_origin_size(Vec2::new(cursor_x, cursor_y), placeholder_size);
                self.draw_rect_border(bounds, Color::TRANSPARENT, color, 1.0);
                cursor_x += placeholder_size.x();
            }
        }
    }

    /// Measure text dimensions.
    pub fn measure_text(&self, text: &str, size: f32) -> Vec2 {
        self.fonts.measure_text(self.current_font, text, size)
    }

    /// Set the current font for text rendering.
    pub fn set_font(&mut self, font_id: FontId) {
        self.current_font = font_id;
    }

    /// Get the current font ID.
    pub fn current_font(&self) -> FontId {
        self.current_font
    }

    // -------------------------------------------------------------------------
    // Layout Helpers
    // -------------------------------------------------------------------------

    /// Set the cursor position for automatic layout.
    pub fn set_cursor(&mut self, pos: Vec2) {
        self.cursor = pos;
    }

    /// Get the current cursor position.
    pub fn cursor(&self) -> Vec2 {
        self.cursor
    }

    /// Move cursor to next line.
    pub fn newline(&mut self) {
        self.cursor = Vec2::new(0.0, self.cursor.y() + self.row_height + self.style.item_spacing);
        self.row_height = 0.0;
    }

    /// Get bounds for the next item in a horizontal layout.
    pub fn next_item(&mut self, size: Vec2) -> Rect2D {
        let bounds = Rect2D::from_origin_size(self.cursor, size);
        self.cursor = Vec2::new(
            self.cursor.x() + size.x() + self.style.item_spacing,
            self.cursor.y(),
        );
        self.row_height = self.row_height.max(size.y());
        bounds
    }

    /// Begin a horizontal layout row.
    pub fn layout_row(&mut self, height: f32) {
        self.row_height = height;
    }

    // -------------------------------------------------------------------------
    // Widget Helpers
    // -------------------------------------------------------------------------

    /// Check if a widget is being hovered.
    fn is_hovered(&self, bounds: Rect2D) -> bool {
        self.input.is_hovered(bounds) && self.active_id.is_none()
    }

    /// Update hover state for a widget.
    fn update_hover(&mut self, id: WidgetId, bounds: Rect2D) -> bool {
        let hovered = self.is_hovered(bounds);
        if hovered {
            self.hovered_id = Some(id);
            self.input.want_capture_mouse = true;
        }
        hovered
    }

    /// Handle button behavior (returns true if clicked).
    fn button_behavior(&mut self, id: WidgetId, bounds: Rect2D) -> bool {
        let hovered = self.update_hover(id, bounds);

        if hovered && self.input.mouse_pressed[crate::input::mouse_button::LEFT] {
            self.active_id = Some(id);
        }

        let clicked = self.active_id == Some(id)
            && self.input.mouse_released[crate::input::mouse_button::LEFT];

        if self.input.mouse_released[crate::input::mouse_button::LEFT] {
            self.active_id = None;
        }

        clicked
    }

    // -------------------------------------------------------------------------
    // Widgets
    // -------------------------------------------------------------------------

    /// Draw a label (non-interactive text).
    pub fn label(&mut self, text: &str, bounds: Rect2D) {
        let text_size = self.measure_text(text, self.style.font_size);
        let centered = Vec2::new(
            bounds.min.x() + (bounds.width() - text_size.x()) * 0.5,
            bounds.min.y() + (bounds.height() - text_size.y()) * 0.5,
        );
        self.draw_text(text, centered, self.style.text_color, self.style.font_size);
    }

    /// Draw a button. Returns true if clicked this frame.
    pub fn button(&mut self, id: &str, text: &str, bounds: Rect2D) -> bool {
        let widget_id = self.generate_id(id);

        let clicked = self.button_behavior(widget_id, bounds);

        // Determine colors based on state
        let (bg_color, text_color) = if self.active_id == Some(widget_id) {
            (self.style.button_active, self.style.button_text)
        } else if self.hovered_id == Some(widget_id) || self.input.is_hovered(bounds) {
            (self.style.button_hovered, self.style.button_text)
        } else {
            (self.style.button_normal, self.style.button_text)
        };

        // Draw button background
        self.draw_rect(bounds, bg_color);

        // Draw button text (centered)
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

        // Draw check mark if checked
        if *checked {
            // Simple X mark
            let pad = check_size * 0.25;
            let inner = check_bounds.contract(pad);
            self.draw_line(inner.min, inner.max, Color::WHITE, 2.0);
            self.draw_line(
                Vec2::new(inner.min.x(), inner.max.y()),
                Vec2::new(inner.max.x(), inner.min.y()),
                Color::WHITE,
                2.0,
            );
        }

        // Draw label
        let label_pos = Vec2::new(check_bounds.max.x() + 8.0, bounds.min.y());
        self.draw_text(label, label_pos, self.style.text_color, self.style.font_size);

        clicked
    }

    /// Draw a slider. Returns true if value changed.
    pub fn slider(&mut self, id: &str, value: &mut f32, min: f32, max: f32, bounds: Rect2D) -> bool {
        let widget_id = self.generate_id(id);

        let hovered = self.update_hover(widget_id, bounds);
        let active = self.active_id == Some(widget_id);

        // Handle dragging
        let mut changed = false;

        if active {
            if self.input.mouse_down[crate::input::mouse_button::LEFT] {
                let t = ((self.input.mouse_pos.x() - bounds.min.x()) / bounds.width())
                    .clamp(0.0, 1.0);
                let new_value = min + t * (max - min);
                if (new_value - *value).abs() > 0.0001 {
                    *value = new_value;
                    changed = true;
                }
            } else {
                self.active_id = None;
            }
        } else if hovered && self.input.mouse_pressed[crate::input::mouse_button::LEFT] {
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
            self.style.slider_grab
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
    pub fn text_input(
        &mut self,
        id: &str,
        text: &mut String,
        bounds: Rect2D,
    ) -> bool {
        let widget_id = self.generate_id(id);
        let hovered = self.update_hover(widget_id, bounds);

        // Focus on click
        if hovered && self.input.mouse_pressed[crate::input::mouse_button::LEFT] {
            self.input.focused_id = Some(widget_id);
        }

        let focused = self.input.focused_id == Some(widget_id);
        let mut changed = false;

        // Handle keyboard input when focused
        if focused {
            self.input.want_capture_keyboard = true;

            // Process character input
            for c in &self.input.characters.clone() {
                if *c == '\x08' {
                    // Backspace
                    if !text.is_empty() {
                        text.pop();
                        changed = true;
                    }
                } else if *c >= ' ' && text.len() < 256 {
                    // Printable character
                    text.push(*c);
                    changed = true;
                }
            }

            // Handle special keys
            use crate::input::KeyCode;
            if self.input.key_pressed(KeyCode::Enter) {
                // Could trigger a callback here
            }
            if self.input.key_pressed(KeyCode::Escape) {
                self.input.focused_id = None;
            }
        }

        // Draw background
        let bg_color = if focused {
            self.style.input_bg
        } else {
            self.style.input_bg
        };
        self.draw_rect(bounds, bg_color);
        self.draw_rect_border(bounds, Color::TRANSPARENT, self.style.input_border, 1.0);

        // Draw text with clipping
        let padding = 4.0;
        let text_bounds = bounds.contract(padding);
        self.push_clip(text_bounds);

        let text_pos = Vec2::new(
            bounds.min.x() + padding,
            bounds.center().y() - self.style.font_size * 0.5,
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
    pub fn text_area(
        &mut self,
        id: &str,
        text: &mut String,
        bounds: Rect2D,
    ) -> bool {
        let widget_id = self.generate_id(id);
        let hovered = self.update_hover(widget_id, bounds);

        if hovered && self.input.mouse_pressed[crate::input::mouse_button::LEFT] {
            self.input.focused_id = Some(widget_id);
        }

        let focused = self.input.focused_id == Some(widget_id);
        let mut changed = false;

        if focused {
            self.input.want_capture_keyboard = true;

            for c in &self.input.characters.clone() {
                if *c == '\x08' {
                    if !text.is_empty() {
                        text.pop();
                        changed = true;
                    }
                } else if *c >= ' ' || *c == '\n' {
                    if text.len() < 4096 {
                        text.push(*c);
                        changed = true;
                    }
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
            if y + self.style.font_size > bounds.min.y() + padding
                && y < bounds.max.y() - padding
            {
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
    // Container Widgets
    // -------------------------------------------------------------------------

    /// Begin a window container.
    ///
    /// Returns a WindowState for window information.
    /// Call `end_window()` after adding contents.
    pub fn begin_window(&mut self, id: &str, bounds: Rect2D) -> WindowState {
        let window_id = self.generate_id(id);

        // Draw window background
        self.draw_rect(bounds, self.style.window_bg);

        // Draw border
        self.draw_rect_border(
            bounds.contract(1.0),
            self.style.window_bg,
            self.style.window_border,
            1.0,
        );

        // Push clip rect for content
        let content_bounds = Rect2D::new(
            Vec2::new(bounds.min.x(), bounds.min.y()),
            bounds.max,
        );
        self.push_clip(content_bounds);

        WindowState {
            id: window_id,
            bounds,
            content_cursor: Vec2::new(
                bounds.min.x() + self.style.window_padding,
                bounds.min.y() + self.style.window_padding,
            ),
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

        // Draw arrow indicator
        let arrow = if *open { "▼ " } else { "► " };
        let text = format!("{}{}", arrow, label);
        let text_pos = Vec2::new(
            bounds.min.x() + 4.0,
            bounds.center().y() - self.style.font_size * 0.5,
        );
        self.draw_text(&text, text_pos, self.style.text_color, self.style.font_size);

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
            let fill_bounds = Rect2D::from_origin_size(
                bounds.min,
                Vec2::new(fill_width, bounds.height()),
            );
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
}

/// State for a window being built.
pub struct WindowState {
    /// Window widget ID.
    pub id: WidgetId,
    /// Window bounds.
    pub bounds: Rect2D,
    /// Cursor for content layout.
    pub content_cursor: Vec2,
}

impl Default for UiContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_creation() {
        let ctx = UiContext::new();
        assert_eq!(ctx.screen_size(), Vec2::new(0.0, 0.0));
    }

    #[test]
    fn test_begin_end_frame() {
        let mut ctx = UiContext::new();

        ctx.begin(Vec2::new(800.0, 600.0));
        assert_eq!(ctx.screen_size(), Vec2::new(800.0, 600.0));

        let draw_list = ctx.end();
        assert!(draw_list.is_empty());
    }

    #[test]
    fn test_draw_rect() {
        let mut ctx = UiContext::new();
        ctx.begin(Vec2::new(800.0, 600.0));

        ctx.draw_rect(
            Rect2D::from_origin_size(Vec2::new(10.0, 10.0), Vec2::new(100.0, 50.0)),
            Color::RED,
        );

        let draw_list = ctx.end();
        assert_eq!(draw_list.vertex_count(), 4);
        assert_eq!(draw_list.index_count(), 6);
    }

    #[test]
    fn test_id_generation() {
        let mut ctx = UiContext::new();
        ctx.begin(Vec2::new(800.0, 600.0));

        let id1 = ctx.generate_id("test");
        let id2 = ctx.generate_id("test");
        let id3 = ctx.generate_id("other");

        // Same label should produce different IDs due to counter
        assert_ne!(id1, id2);
        assert_ne!(id1, id3);

        ctx.end();
    }
}
