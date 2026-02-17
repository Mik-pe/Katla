//! UI context - the main entry point for UI rendering.
//!
//! The `UiContext` manages all UI state and provides the immediate mode API
//! for building user interfaces.

use std::collections::HashMap;

use katla_math::{Color, Rect2D, Vec2};

use crate::draw_list::{DrawList, TextureId};
use crate::icons::ForkAwesome;
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
    /// Current screen size (logical pixels).
    screen_size: Vec2,
    /// DPI scale factor (physical pixels per logical pixel).
    scale_factor: f32,
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
    /// Currently open popup ID.
    popup_id: Option<WidgetId>,
    /// Bounds of the current popup (for click-outside detection).
    popup_bounds: Option<Rect2D>,
    /// Whether a popup was opened this frame (prevents immediate close).
    popup_opened_this_frame: bool,
    /// Current Z-index for rendering (higher = on top).
    z_index: u32,
    /// Z-index stack for nested containers.
    z_stack: Vec<u32>,
}

/// Z-index constants for UI layers.
pub mod z_index {
    /// Default layer for regular UI elements.
    pub const DEFAULT: u32 = 0;
    /// Layer for floating panels/windows.
    pub const PANEL: u32 = 100;
    /// Layer for dropdowns and popups.
    pub const POPUP: u32 = 200;
    /// Layer for tooltips (always on top).
    pub const TOOLTIP: u32 = 300;
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
    /// Dropdown open state.
    DropdownOpen(bool),
    /// Context menu position.
    ContextMenuPos(Vec2),
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
            scale_factor: 1.0,
            clip_stack: Vec::new(),
            id_stack: Vec::new(),
            id_counter: 0,
            storage: HashMap::new(),
            hovered_id: None,
            active_id: None,
            in_frame: false,
            cursor: Vec2::new(0.0, 0.0),
            row_height: 0.0,
            popup_id: None,
            popup_bounds: None,
            popup_opened_this_frame: false,
            z_index: z_index::DEFAULT,
            z_stack: Vec::new(),
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
    /// `screen_size` is the current window/render target size in logical pixels.
    /// `scale_factor` is the DPI scale factor (physical pixels per logical pixel).
    pub fn begin(&mut self, screen_size: Vec2, scale_factor: f32) {
        debug_assert!(!self.in_frame, "begin() called while already in frame");

        self.in_frame = true;
        self.screen_size = screen_size;
        self.scale_factor = scale_factor;
        self.draw_list.clear();
        self.id_stack.clear();
        self.z_stack.clear();
        self.z_index = z_index::DEFAULT;
        self.id_counter = 0;
        self.hovered_id = None;
        self.cursor = Vec2::new(0.0, 0.0);
        self.row_height = 0.0;

        // Check for click outside popup to close it
        // NOTE: We check BEFORE resetting popup_opened_this_frame so that
        // popups opened in the previous frame don't get closed immediately
        if self.popup_id.is_some() && !self.popup_opened_this_frame {
            if self.input.mouse_pressed[crate::input::mouse_button::LEFT] {
                let mouse_outside = self
                    .popup_bounds
                    .map_or(true, |bounds| !bounds.contains(self.input.mouse_pos));
                if mouse_outside {
                    self.popup_id = None;
                    self.popup_bounds = None;
                }
            }
        }

        // Reset the flag AFTER the check
        self.popup_opened_this_frame = false;

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

    /// Get the current DPI scale factor.
    #[inline]
    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    /// Scale a logical pixel value to physical pixels.
    #[inline]
    pub fn scale(&self, logical: f32) -> f32 {
        logical * self.scale_factor
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
        *self
            .clip_stack
            .last()
            .unwrap_or(&Rect2D::from_size(self.screen_size))
    }

    /// Push a new clip rectangle (intersection with current).
    pub fn push_clip(&mut self, rect: Rect2D) {
        let current = self.clip_rect();
        let clipped = current
            .intersection(&rect)
            .unwrap_or(Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0)));
        self.clip_stack.push(clipped);
        self.draw_list.set_clip(clipped);
    }

    /// Push an absolute clip rectangle (no intersection with current).
    ///
    /// Use this for popups that need to render outside their parent container.
    pub fn push_clip_absolute(&mut self, rect: Rect2D) {
        self.clip_stack.push(rect);
        self.draw_list.set_clip(rect);
    }

    /// Pop a clip rectangle.
    pub fn pop_clip(&mut self) {
        if self.clip_stack.len() > 1 {
            self.clip_stack.pop();
            self.draw_list.set_clip(self.clip_rect());
        }
    }

    // -------------------------------------------------------------------------
    // Z-Index Management
    // -------------------------------------------------------------------------

    /// Set the current Z-index for rendering.
    ///
    /// Higher Z values are rendered on top of lower Z values.
    /// Use the constants in `z_index` module for common layers.
    pub fn set_z_index(&mut self, z: u32) {
        self.z_index = z;
        self.draw_list.set_z_index(z);
    }

    /// Get the current Z-index.
    pub fn z_index(&self) -> u32 {
        self.z_index
    }

    /// Push a new Z-index onto the stack and set it as current.
    pub fn push_z_index(&mut self, z: u32) {
        self.z_stack.push(self.z_index);
        self.set_z_index(z);
    }

    /// Pop a Z-index from the stack and restore the previous value.
    pub fn pop_z_index(&mut self) {
        if let Some(prev_z) = self.z_stack.pop() {
            self.set_z_index(prev_z);
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
    pub fn draw_rect_border(
        &mut self,
        bounds: Rect2D,
        fill: Color,
        border: Color,
        border_width: f32,
    ) {
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

    /// Draw a textured image.
    ///
    /// The texture is specified via UV coordinates. For viewport texture,
    /// use uv_min with x >= 1.0 (e.g., (1.0, 0.0) to (2.0, 1.0)).
    /// For font atlas, use uv_min with x < 1.0.
    pub fn draw_image(&mut self, bounds: Rect2D, uv_min: Vec2, uv_max: Vec2, color: Color) {
        self.draw_list.set_clip(self.clip_rect());
        self.draw_list.add_image(bounds, uv_min, uv_max, color);
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
    ///
    /// `position` is the TOP-LEFT of the text bounding box.
    /// This is the most intuitive API for UI work.
    pub fn draw_text(&mut self, text: &str, position: Vec2, color: Color, size: f32) {
        let mut cursor_x = position.x();

        // Get the actual font ascent for proper baseline positioning
        // Baseline is at position.y + ascent (ascent is distance from baseline to font top)
        let ascent = self.font_ascent(size);
        let baseline_y = position.y() + ascent;

        for c in text.chars() {
            // Try to get cached glyph (scale font size by DPI factor)
            if let Some(glyph) =
                self.fonts
                    .get_or_rasterize(self.current_font, c, size, self.scale_factor)
            {
                // Skip empty glyphs (spaces)
                if glyph.size.x() == 0.0 || glyph.size.y() == 0.0 {
                    cursor_x += glyph.advance;
                    continue;
                }

                // Calculate glyph position:
                // - cursor_x + glyph.offset_x = left edge of glyph
                // - baseline_y - glyph.top_offset = top edge of glyph (top_offset is distance up from baseline)
                let glyph_pos = Vec2::new(cursor_x + glyph.offset_x, baseline_y - glyph.top_offset);

                // Snap glyph position to pixel grid for crisp rendering
                let snapped_pos = Vec2::new(glyph_pos.x().round(), glyph_pos.y().round());

                let bounds = Rect2D::from_origin_size(snapped_pos, glyph.size);

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
                let bounds = Rect2D::from_origin_size(position, placeholder_size);
                self.draw_rect_border(bounds, Color::TRANSPARENT, color, 1.0);
                cursor_x += placeholder_size.x();
            }
        }
    }

    /// Measure text dimensions in logical pixels.
    pub fn measure_text(&self, text: &str, size: f32) -> Vec2 {
        self.fonts
            .measure_text(self.current_font, text, size, self.scale_factor)
    }

    /// Get the font ascent (baseline to font top) in logical pixels.
    ///
    /// This is needed for proper text positioning.
    pub fn font_ascent(&self, size: f32) -> f32 {
        self.fonts
            .get_font_metrics(self.current_font, size, self.scale_factor)
            .map(|(ascent, _, _)| ascent)
            .unwrap_or(size * 0.75) // Fallback heuristic
    }

    /// Draw an icon from an icon font (like ForkAwesome).
    ///
    /// This is a convenience method that temporarily switches to the icon font,
    /// renders the icon, and restores the previous font.
    ///
    /// # Arguments
    /// * `icon` - The icon character (use constants from `icons::ForkAwesome`)
    /// * `position` - Top-left position of the icon
    /// * `size` - Font size in pixels
    /// * `color` - RGBA color
    ///
    /// # Example
    /// ```ignore
    /// use katla_ui::{FontId, icons::ForkAwesome};
    ///
    /// ui.draw_icon(ForkAwesome::CUBE, pos, 16.0, [1.0, 1.0, 1.0, 1.0]);
    /// ```
    pub fn draw_icon(&mut self, icon: char, position: Vec2, size: f32, color: Color) {
        let prev_font = self.current_font;
        self.current_font = FontId::ICON;

        let mut buf = [0u8; 4];
        let icon_str = icon.encode_utf8(&mut buf);
        self.draw_text(icon_str, position, color, size);

        self.current_font = prev_font;
    }

    /// Draw an icon aligned with adjacent text.
    ///
    /// This method uses the reference font's ascent for baseline positioning,
    /// ensuring icons align properly with text rendered in that font.
    /// Use this when drawing icons alongside regular text.
    ///
    /// # Arguments
    /// * `icon` - The icon character (use constants from `icons::ForkAwesome`)
    /// * `position` - Top-left position (same as you'd use for adjacent text)
    /// * `size` - Font size in pixels
    /// * `color` - RGBA color
    /// * `ref_font` - Reference font to use for baseline alignment (usually FontId::DEFAULT)
    pub fn draw_icon_aligned(
        &mut self,
        icon: char,
        position: Vec2,
        size: f32,
        color: Color,
        ref_font: FontId,
    ) {
        // Get text font metrics
        let text_ascent = self
            .fonts
            .get_font_metrics(ref_font, size, self.scale_factor)
            .map(|(a, _, _)| a)
            .unwrap_or(size * 0.75);

        // Get icon's actual rendered size
        let icon_glyph = self
            .fonts
            .get_or_rasterize(FontId::ICON, icon, size, self.scale_factor);

        if let Some(glyph) = icon_glyph {
            if glyph.size.x() > 0.0 && glyph.size.y() > 0.0 {
                // Text centerline: position.y + text_ascent/2
                // Icon center should match: icon_top + icon_height/2 = text_center
                let text_center_y = position.y() + text_ascent * 0.5;
                let icon_top_y = text_center_y - glyph.size.y() * 0.5;

                let glyph_pos =
                    Vec2::new((position.x() + glyph.offset_x).round(), icon_top_y.round());
                let bounds = katla_math::Rect2D::from_origin_size(glyph_pos, glyph.size);
                self.draw_list.set_clip(self.clip_rect());
                self.draw_list.add_textured_rect(
                    bounds,
                    glyph.uv_rect,
                    color,
                    TextureId::FONT_ATLAS,
                );
            }
        }
    }

    /// Draw an icon centered within bounds.
    ///
    /// This is useful for icon buttons where you want the icon centered.
    pub fn draw_icon_centered(&mut self, icon: char, bounds: Rect2D, size: f32, color: Color) {
        // Get icon metrics to center it
        let prev_font = self.current_font;
        self.current_font = FontId::ICON;

        let mut buf = [0u8; 4];
        let icon_str = icon.encode_utf8(&mut buf);
        let icon_size = self.measure_text(icon_str, size);

        let pos = Vec2::new(
            bounds.center().x() - icon_size.x() * 0.5,
            bounds.center().y() - icon_size.y() * 0.5,
        );

        self.draw_text(icon_str, pos, color, size);
        self.current_font = prev_font;
    }

    /// Measure an icon's dimensions.
    pub fn measure_icon(&mut self, icon: char, size: f32) -> Vec2 {
        let prev_font = self.current_font;
        self.current_font = FontId::ICON;

        let mut buf = [0u8; 4];
        let icon_str = icon.encode_utf8(&mut buf);
        let dims = self.measure_text(icon_str, size);

        self.current_font = prev_font;
        dims
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
        self.cursor = Vec2::new(
            0.0,
            self.cursor.y() + self.row_height + self.style.item_spacing,
        );
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
    ///
    /// This uses the imgui/egui approach: widgets at lower Z levels can only
    /// be hovered if the cursor is NOT inside a higher-level popup's bounds.
    /// This allows clicking outside popups to work correctly while still
    /// blocking hover for widgets covered by the popup.
    pub fn is_hovered(&self, bounds: Rect2D) -> bool {
        // If a popup is open and cursor is inside popup bounds,
        // block hover for widgets at lower Z levels
        if let Some(popup_bounds) = self.popup_bounds {
            if popup_bounds.contains(self.input.mouse_pos) && self.z_index < z_index::POPUP {
                return false;
            }
        }
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

        // Only clear active_id if we're the active widget
        if clicked {
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
            if self.input.mouse_down[crate::input::mouse_button::LEFT] {
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
    pub fn text_input(&mut self, id: &str, text: &mut String, bounds: Rect2D) -> bool {
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
    // Container Widgets
    // -------------------------------------------------------------------------

    /// Begin a window container.
    ///
    /// Returns a WindowState for window information.
    /// Call `end_window()` after adding contents.
    pub fn begin_window(&mut self, id: &str, bounds: Rect2D) -> WindowState {
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
    ) -> WindowState {
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

        WindowState {
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
        options: Option<GraphOptions>,
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

    // -------------------------------------------------------------------------
    // Menu Widgets
    // -------------------------------------------------------------------------

    /// Draw a menu item (clickable item styled for menus).
    ///
    /// Returns true if clicked this frame.
    pub fn menu_item(&mut self, id: &str, label: &str, bounds: Rect2D) -> bool {
        let widget_id = self.generate_id(id);
        let clicked = self.button_behavior(widget_id, bounds);

        // Determine colors based on state
        let bg_color = if self.active_id == Some(widget_id) {
            self.style.menu_active
        } else if self.hovered_id == Some(widget_id) || self.is_hovered(bounds) {
            self.style.menu_hovered
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

    /// Begin a dropdown menu.
    ///
    /// Returns true if the dropdown is open and should have menu items drawn.
    /// Call `end_dropdown()` after adding contents.
    /// The `bounds` is the trigger button area; popup appears below it.
    pub fn begin_dropdown(&mut self, id: &str, label: &str, bounds: Rect2D) -> bool {
        let dropdown_id = self.generate_id(id);

        // Get or initialize open state
        let is_open = self
            .storage
            .get(&dropdown_id)
            .map(|s| matches!(s, WidgetState::DropdownOpen(true)))
            .unwrap_or(false);

        // Draw trigger button
        let hovered = self.update_hover(dropdown_id, bounds);

        // Toggle on click
        if self.button_behavior(dropdown_id, bounds) {
            let new_open = !is_open;
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
        let text_pos = Vec2::new(
            bounds.center().x() - text_size.x() * 0.5 - 10.0,
            bounds.center().y() - text_size.y() * 0.5,
        );
        self.draw_text(
            label,
            text_pos,
            self.style.button_text,
            self.style.font_size,
        );

        // Draw dropdown icon
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

        // If open, prepare popup area
        if is_open {
            // Switch to popup Z-index
            self.push_z_index(z_index::POPUP);

            let popup_bounds = Rect2D::from_origin_size(
                Vec2::new(bounds.min.x(), bounds.max.y()),
                Vec2::new(
                    bounds.width().max(self.style.menu_min_width),
                    200.0, // Will be clipped by content
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

    /// End a dropdown menu.
    pub fn end_dropdown(&mut self) {
        self.pop_clip();
        self.pop_id();
        self.pop_z_index();
    }

    /// Begin a context menu (right-click popup).
    ///
    /// Returns true if the context menu is open and should have items drawn.
    /// Call `end_context_menu()` after adding contents.
    /// Typically called after checking `is_context_menu_open()` or unconditionally.
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
            // Switch to popup Z-index
            self.push_z_index(z_index::POPUP);

            // Calculate popup bounds
            let popup_size = Vec2::new(self.style.menu_min_width, 200.0); // Height will be clipped

            // Keep on screen
            let mut popup_pos = pos;
            if popup_pos.x() + popup_size.x() > self.screen_size.x() {
                popup_pos = Vec2::new(self.screen_size.x() - popup_size.x() - 5.0, popup_pos.y());
            }
            if popup_pos.y() + popup_size.y() > self.screen_size.y() {
                popup_pos = Vec2::new(popup_pos.x(), self.screen_size.y() - popup_size.y() - 5.0);
            }

            let popup_bounds = Rect2D::from_origin_size(popup_pos, popup_size);

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

    /// End a context menu.
    pub fn end_context_menu(&mut self) {
        self.pop_clip();
        self.pop_id();
        self.pop_z_index();
    }

    /// Open a context menu at the current mouse position.
    ///
    /// Call this when detecting a right-click on an area.
    /// Returns true if the menu was just opened.
    pub fn open_context_menu(&mut self, id: &str) -> bool {
        let context_id = self.generate_id(id);

        // Check for right-click
        if self.input.mouse_pressed[crate::input::mouse_button::RIGHT] {
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
        } else if self.active_id == Some(combo_id) {
            self.style.combo_hovered
        } else if hovered {
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
                    200.0, // Will be clipped by content
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

    /// Get the menu item height for layout.
    pub fn menu_item_height(&self) -> f32 {
        self.style.menu_item_height
    }
}

/// Options for configuring a graph widget.
#[derive(Debug, Clone)]
pub struct GraphOptions {
    /// Minimum Y value (auto-calculated if None).
    pub min_value: Option<f32>,
    /// Maximum Y value (auto-calculated if None).
    pub max_value: Option<f32>,
    /// Color of the line.
    pub line_color: Color,
    /// Color of the fill under the line (None = no fill).
    pub fill_color: Option<Color>,
    /// Background color.
    pub bg_color: Color,
    /// Grid line color (None = no grid).
    pub grid_color: Option<Color>,
    /// Number of horizontal grid lines.
    pub grid_lines: u32,
    /// Thickness of the line.
    pub line_thickness: f32,
    /// Whether to show the current value text.
    pub show_value: bool,
}

impl Default for GraphOptions {
    fn default() -> Self {
        Self {
            min_value: None,
            max_value: None,
            line_color: Color::GREEN,
            fill_color: Some(Color::new(0.0, 1.0, 0.0, 0.3)),
            bg_color: Color::new(0.1, 0.1, 0.1, 0.9),
            grid_color: Some(Color::new(0.3, 0.3, 0.3, 0.5)),
            grid_lines: 4,
            line_thickness: 2.0,
            show_value: true,
        }
    }
}

impl GraphOptions {
    /// Create graph options for FPS display (0-120 range, green).
    pub fn fps() -> Self {
        Self {
            min_value: Some(0.0),
            max_value: Some(120.0),
            line_color: Color::rgb(0.2, 0.9, 0.2),
            fill_color: Some(Color::new(0.2, 0.9, 0.2, 0.25)),
            ..Default::default()
        }
    }

    /// Create graph options for frame time display (0-50ms range, orange).
    pub fn frame_time() -> Self {
        Self {
            min_value: Some(0.0),
            max_value: Some(50.0),
            line_color: Color::rgb(1.0, 0.6, 0.2),
            fill_color: Some(Color::new(1.0, 0.6, 0.2, 0.25)),
            ..Default::default()
        }
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
    /// Height of the title bar (0 if no title).
    pub title_height: f32,
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

        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        assert_eq!(ctx.screen_size(), Vec2::new(800.0, 600.0));

        let draw_list = ctx.end();
        assert!(draw_list.is_empty());
    }

    #[test]
    fn test_draw_rect() {
        let mut ctx = UiContext::new();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);

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
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);

        let id1 = ctx.generate_id("test");
        let id2 = ctx.generate_id("test");
        let id3 = ctx.generate_id("other");

        // Same label should produce different IDs due to counter
        assert_ne!(id1, id2);
        assert_ne!(id1, id3);

        ctx.end();
    }
}
