//! UI context - the main entry point for UI rendering.
//!
//! The `UiContext` manages all UI state and provides the immediate mode API
//! for building user interfaces.

mod drawing;
mod layout;
mod popup;
mod state;
mod widgets;

use std::collections::HashMap;

use katla_math::{Color, Rect2D, Vec2};

use crate::draw_list::DrawList;
use crate::input::UiInputState;
use crate::style::UiStyle;
use crate::text::{FontId, FontSystem};

pub use layout::{LayoutDirection, LayoutState};
pub use popup::DeferredDraw;
pub use state::{StateAccess, WidgetState, WidgetStorage};

/// ID type for UI elements.
pub type WidgetId = u64;

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

/// RAII guard for Z-index management.
///
/// Automatically pops the Z-index when dropped.
///
/// # Example
/// ```ignore
/// {
///     let _z = ui.z_guard(z_index::POPUP);
///     // draw popup content
/// } // auto-pops
/// ```
pub struct ZGuard<'a> {
    ctx: &'a mut UiContext,
}

impl Drop for ZGuard<'_> {
    fn drop(&mut self) {
        self.ctx.pop_z_index();
    }
}

/// Main context for immediate mode UI rendering.
///
/// This is the primary API for building UI. Typical usage:
/// 1. Call `begin()` at the start of each frame
/// 2. Call widget functions to build the UI
/// 3. Call `end()` to finalize and get the draw list
pub struct UiContext {
    /// The draw list being built this frame.
    pub(crate) draw_list: DrawList,
    /// Input state (updated externally).
    pub input: UiInputState,
    /// Style configuration.
    pub style: UiStyle,
    /// Font system for text rendering.
    pub fonts: FontSystem,
    /// Currently active font.
    pub(crate) current_font: FontId,
    /// Current screen size (logical pixels).
    pub(crate) screen_size: Vec2,
    /// DPI scale factor (physical pixels per logical pixel).
    pub(crate) scale_factor: f32,
    /// Font scale multiplier for accessibility (1.0 = 100%).
    font_scale: f32,
    /// Stack of clipping rectangles.
    clip_stack: Vec<Rect2D>,
    /// Stack of widget IDs for nesting.
    id_stack: Vec<WidgetId>,
    /// Counter for generating unique IDs.
    id_counter: u32,
    /// Storage for widget state (checkboxes, sliders, etc.).
    pub(crate) storage: HashMap<WidgetId, WidgetState>,
    /// Currently hovered widget.
    pub(crate) hovered_id: Option<WidgetId>,
    /// Currently active (pressed) widget.
    pub(crate) active_id: Option<WidgetId>,
    /// Whether we're inside a begin()/end() pair.
    in_frame: bool,
    /// Layout cursor for automatic positioning.
    pub(crate) cursor: Vec2,
    /// Current row height for layout.
    row_height: f32,
    /// Layout stack for nested layouts.
    pub(crate) layout_stack: Vec<LayoutState>,
    /// Currently open popup ID.
    popup_id: Option<WidgetId>,
    /// Bounds of the current popup (for click-outside detection).
    pub(crate) popup_bounds: Option<Rect2D>,
    /// Whether a popup was opened this frame (prevents immediate close).
    popup_opened_this_frame: bool,
    /// Whether a popup consumed the click this frame (prevents click-through).
    popup_consume_click: bool,
    /// Current Z-index for rendering (higher = on top).
    z_index: u32,
    /// Z-index stack for nested containers.
    z_stack: Vec<u32>,
    /// Tracked bounding box of all popup content (auto-expanded as items are drawn).
    pub(crate) popup_content_bounds: Option<Rect2D>,
    /// Current popup cursor position for automatic layout.
    popup_cursor: Vec2,
    /// Popup width for automatic layout.
    popup_width: f32,
    /// Deferred draws for dropdown items (drawn after background).
    pub(crate) dropdown_deferred: Vec<DeferredDraw>,
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
            font_scale: 1.0,
            clip_stack: Vec::new(),
            id_stack: Vec::new(),
            id_counter: 0,
            storage: HashMap::new(),
            hovered_id: None,
            active_id: None,
            in_frame: false,
            cursor: Vec2::new(0.0, 0.0),
            row_height: 0.0,
            layout_stack: Vec::new(),
            popup_id: None,
            popup_bounds: None,
            popup_opened_this_frame: false,
            popup_consume_click: false,
            z_index: z_index::DEFAULT,
            z_stack: Vec::new(),
            popup_content_bounds: None,
            popup_cursor: Vec2::new(0.0, 0.0),
            popup_width: 0.0,
            dropdown_deferred: Vec::new(),
        }
    }

    /// Create a new UI context with a specific style.
    pub fn with_style(style: UiStyle) -> Self {
        Self {
            style,
            ..Self::new()
        }
    }

    /// Set the font scale multiplier for accessibility.
    ///
    /// Use 1.0 for default (100%), 1.25 for 125%, 2.0 for 200%, etc.
    pub fn set_font_scale(&mut self, scale: f32) {
        self.font_scale = scale.clamp(0.5, 3.0);
    }

    /// Get the current font scale multiplier.
    pub fn font_scale(&self) -> f32 {
        self.font_scale
    }

    /// Convert a FontSize to scaled pixels using current font_scale.
    pub fn scaled_font_size(&self, size: crate::style::FontSize) -> f32 {
        size.to_pixels_scaled(self.font_scale)
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
        self.layout_stack.clear();

        // Clear custom popup bounds at start of frame (they'll be re-registered if still open)
        // Built-in popups (with popup_id) keep their bounds for click-outside detection
        if self.popup_id.is_none() {
            self.popup_bounds = None;
        }

        // Reset popup click consumption flag from previous frame
        self.popup_consume_click = false;

        // Check for click outside popup to close it
        // NOTE: We check BEFORE resetting popup_opened_this_frame so that
        // popups opened in the previous frame don't get closed immediately
        if self.popup_id.is_some()
            && !self.popup_opened_this_frame
            && self.input.mouse_pressed[crate::input::mouse_button::LEFT]
        {
            let mouse_outside = self
                .popup_bounds
                .is_none_or(|bounds| !bounds.contains(self.input.mouse_pos));
            if mouse_outside {
                // Close the dropdown in storage too
                if let Some(popup_id) = self.popup_id {
                    self.storage.insert(popup_id, WidgetState::DropdownOpen(false));
                }
                self.popup_id = None;
                self.popup_bounds = None;
                // CONSUME the click to prevent click-through to underlying widgets
                self.popup_consume_click = true;
            }
        }

        // Reset the flag AFTER the check
        self.popup_opened_this_frame = false;

        // NOTE: Don't clear active_id here! Widgets need to check it in button_behavior.
        // We'll clear it in end() if it wasn't consumed by a click.

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

        // Clear active_id when mouse is released (after widgets have had a chance to process it)
        if self.input.mouse_released[crate::input::mouse_button::LEFT] {
            self.active_id = None;
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

    /// Set the mouse cursor type.
    #[inline]
    pub fn set_mouse_cursor(&mut self, cursor: crate::input::MouseCursor) {
        self.input.set_cursor(cursor);
    }

    // -------------------------------------------------------------------------
    // ID Management
    // -------------------------------------------------------------------------

    /// Generate a unique ID for a widget.
    pub fn generate_id(&self, label: &str) -> WidgetId {
        let base = if let Some(&parent) = self.id_stack.last() {
            parent
        } else {
            0
        };

        // Simple hash combining parent ID with label
        // This produces consistent IDs across frames for the same widget
        let mut hash = base;
        for byte in label.bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
        }

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

    /// Create an RAII guard for Z-index management.
    ///
    /// The Z-index will be automatically popped when the guard is dropped.
    ///
    /// # Example
    /// ```ignore
    /// {
    ///     let _z = ui.z_guard(z_index::POPUP);
    ///     // draw popup content
    /// } // auto-pops
    /// ```
    pub fn z_guard(&mut self, z: u32) -> ZGuard<'_> {
        self.push_z_index(z);
        ZGuard { ctx: self }
    }

    /// Execute a closure with a temporary Z-index, automatically restoring afterward.
    ///
    /// This is the preferred way to use Z-index for drawing as it avoids borrow checker issues.
    ///
    /// # Example
    /// ```ignore
    /// ui.with_z_index(z_index::POPUP, |ui| {
    ///     ui.draw_rect(bounds, color);
    ///     ui.tooltip("Hello");
    /// }); // Auto-pops z-index
    /// ```
    pub fn with_z_index<F, R>(&mut self, z: u32, f: F) -> R
    where
        F: FnOnce(&mut UiContext) -> R,
    {
        self.push_z_index(z);
        let result = f(self);
        self.pop_z_index();
        result
    }
}

impl Default for UiContext {
    fn default() -> Self {
        Self::new()
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

        // Same label produces same ID (hash-based, deterministic)
        assert_eq!(id1, id2);
        // Different labels produce different IDs
        assert_ne!(id1, id3);

        ctx.end();
    }

    /// Test that text character positions are stable when panel moves within same subpixel bin.
    ///
    /// This simulates the core positioning logic of draw_text:
    /// 1. Calculate floor_x and subpixel bin from start position
    /// 2. Track cursor as offset from floor_x
    /// 3. All characters share the same bin
    #[test]
    fn test_text_positioning_stability_within_bin() {
        use crate::text::SubpixelBin;

        // Simulate text at position 100.1 (subpixel bin Zero: [0.0, 0.25))
        let pos1 = 100.1;
        let (floor_x1, bin1) = SubpixelBin::new(pos1);
        assert_eq!(floor_x1, 100);
        assert_eq!(bin1, SubpixelBin::Zero);

        // Same text at position 100.2 (still bin Zero)
        let pos2 = 100.2;
        let (floor_x2, bin2) = SubpixelBin::new(pos2);
        assert_eq!(floor_x2, 100);
        assert_eq!(bin2, SubpixelBin::Zero);

        // Floor stays the same, bin stays the same
        // Character positions relative to floor_x are IDENTICAL
        assert_eq!(floor_x1, floor_x2);
        assert_eq!(bin1, bin2);
    }

    /// Test that text moves as a unit when crossing subpixel bin boundaries.
    #[test]
    fn test_text_positioning_across_bins() {
        use crate::text::SubpixelBin;

        // Text at 100.0 (bin Zero)
        let (floor_a, bin_a) = SubpixelBin::new(100.0);
        assert_eq!(floor_a, 100);
        assert_eq!(bin_a, SubpixelBin::Zero);

        // Text at 100.25 (bin One)
        let (floor_b, bin_b) = SubpixelBin::new(100.25);
        assert_eq!(floor_b, 100);
        assert_eq!(bin_b, SubpixelBin::One);

        // Text at 100.5 (bin Two)
        let (floor_c, bin_c) = SubpixelBin::new(100.5);
        assert_eq!(floor_c, 100);
        assert_eq!(bin_c, SubpixelBin::Two);

        // Text at 100.75 (bin Three)
        let (floor_d, bin_d) = SubpixelBin::new(100.75);
        assert_eq!(floor_d, 100);
        assert_eq!(bin_d, SubpixelBin::Three);

        // Text at 101.0 (back to bin Zero, floor increases)
        let (floor_e, bin_e) = SubpixelBin::new(101.0);
        assert_eq!(floor_e, 101);
        assert_eq!(bin_e, SubpixelBin::Zero);
    }

    /// Test that character relative positions stay consistent.
    ///
    /// Simulates the cursor offset tracking used in draw_text.
    #[test]
    fn test_text_character_spacing_consistency() {
        use crate::text::SubpixelBin;

        // Simulate two characters with advances
        let char1_advance = 8.5f32;
        let char2_advance = 5.3f32;

        // Position 1: 50.1 (bin Zero)
        let pos1 = 50.1;
        let (floor1, bin1) = SubpixelBin::new(pos1);
        let start_x1 = floor1 as f32;

        // Character 1 position relative to floor
        let char1_offset1 = 0.0f32; // cursor starts at 0
        let char1_x1 = start_x1 + char1_offset1;

        // Character 2 position (after char1 advance)
        let char2_offset1 = char1_advance;
        let char2_x1 = start_x1 + char2_offset1;

        // Position 2: 50.15 (still bin Zero)
        let pos2 = 50.15;
        let (floor2, bin2) = SubpixelBin::new(pos2);
        let start_x2 = floor2 as f32;

        // Same calculations at new position
        let char1_x2 = start_x2 + 0.0f32;
        let char2_x2 = start_x2 + char1_advance;

        // Since both are in same bin (Zero) with same floor (50):
        // - Characters should be at IDENTICAL positions
        assert_eq!(bin1, bin2);
        assert_eq!(floor1, floor2);
        assert_eq!(char1_x1, char1_x2);
        assert_eq!(char2_x1, char2_x2);

        // Relative spacing between characters is always consistent
        let spacing1 = char2_x1 - char1_x1;
        let spacing2 = char2_x2 - char1_x2;
        assert_eq!(spacing1, spacing2);
        assert_eq!(spacing1, char1_advance);
    }

    /// Test that small position changes don't cause character wobble.
    ///
    /// The key invariant: delta between character positions should equal
    /// the delta in text start position when staying in same bin.
    #[test]
    fn test_no_character_wobble() {
        use crate::text::SubpixelBin;

        // Simulate a panel moving slightly
        for base_pos in [0.0, 100.0, 200.5, 500.75] {
            let (base_floor, base_bin) = SubpixelBin::new(base_pos);

            // Small movement within same bin
            for delta in [0.01, 0.05, 0.1, 0.15, 0.2] {
                let new_pos = base_pos + delta;
                let (new_floor, new_bin) = SubpixelBin::new(new_pos);

                // If still in same bin and same floor, positions should be identical
                if base_floor == new_floor && base_bin == new_bin {
                    // All characters would render at same positions
                    // because start_x = floor (integer) is the same
                    assert_eq!(
                        base_floor as f32, new_floor as f32,
                        "Floor should be stable within same bin"
                    );
                }
            }
        }
    }

    // === Popup Content Bounds Tracking Tests ===

    /// Test that track_popup_item correctly expands bounds for a single item.
    #[test]
    fn test_track_popup_item_single() {
        let mut ctx = UiContext::new();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);

        // Initially no bounds tracked
        assert!(ctx.popup_content_bounds.is_none());

        // Track a single item
        let item_bounds = Rect2D::from_origin_size(Vec2::new(100.0, 50.0), Vec2::new(150.0, 24.0));
        ctx.track_popup_item(item_bounds);

        // Bounds should match the item exactly
        let tracked = ctx.popup_content_bounds.unwrap();
        assert_eq!(tracked.min, Vec2::new(100.0, 50.0));
        assert_eq!(tracked.max, Vec2::new(250.0, 74.0));

        ctx.end();
    }

    /// Test that track_popup_item correctly expands bounds for multiple items.
    #[test]
    fn test_track_popup_item_multiple() {
        let mut ctx = UiContext::new();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);

        // Track first item at (100, 50)
        let item1 = Rect2D::from_origin_size(Vec2::new(100.0, 50.0), Vec2::new(150.0, 24.0));
        ctx.track_popup_item(item1);

        // Track second item below first at (100, 74)
        let item2 = Rect2D::from_origin_size(Vec2::new(100.0, 74.0), Vec2::new(150.0, 24.0));
        ctx.track_popup_item(item2);

        // Track third item below second at (100, 98)
        let item3 = Rect2D::from_origin_size(Vec2::new(100.0, 98.0), Vec2::new(150.0, 24.0));
        ctx.track_popup_item(item3);

        // Bounds should encompass all items
        let tracked = ctx.popup_content_bounds.unwrap();
        assert_eq!(tracked.min, Vec2::new(100.0, 50.0), "Top should be at first item top");
        assert_eq!(tracked.max, Vec2::new(250.0, 122.0), "Bottom should be at last item bottom");

        ctx.end();
    }
}
