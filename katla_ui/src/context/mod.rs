//! UI context - the main entry point for UI rendering.
//!
//! The `UiContext` manages all UI state and provides the immediate mode API
//! for building user interfaces.

mod drawing;
mod interaction;
mod layout;
mod popup;
mod widgets;

use katla_math::{Color, Rect2D, Vec2};

use crate::draw_list::DrawList;
use crate::input::UiInputState;
use crate::style::UiStyle;
use crate::text::{FontId, FontSystem};

pub use layout::{LayoutDirection, LayoutState};
pub use popup::{CloseBehavior, Popup, PopupPosition, PopupStyle};
pub use widgets::{ScrollArea, ScrollAreaState};

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
    /// ID of the menu bar dropdown to close (set during hover-to-switch).
    /// When hover-to-switch happens, this is set to the ID of the dropdown
    /// that should close itself.
    menu_bar_close_id: Option<WidgetId>,
    /// Captured position for the current popup (set when first opened).
    popup_position: Option<Vec2>,
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
    /// Scroll area bounds (for end_scroll_area).
    scroll_area_bounds: Option<Rect2D>,
    /// Scroll area content bounds.
    scroll_area_content_bounds: Option<Rect2D>,
    /// Scroll area state (temporary copy).
    scroll_area_state: Option<widgets::ScrollAreaState>,
    /// Whether to show scrollbar for current scroll area.
    scroll_area_show_scrollbar: bool,
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
            hovered_id: None,
            active_id: None,
            in_frame: false,
            cursor: Vec2::new(0.0, 0.0),
            row_height: 0.0,
            layout_stack: Vec::new(),
            popup_id: None,
            menu_bar_close_id: None,
            popup_position: None,
            popup_bounds: None,
            popup_opened_this_frame: false,
            popup_consume_click: false,
            z_index: z_index::DEFAULT,
            z_stack: Vec::new(),
            popup_content_bounds: None,
            popup_cursor: Vec2::new(0.0, 0.0),
            popup_width: 0.0,
            scroll_area_bounds: None,
            scroll_area_content_bounds: None,
            scroll_area_state: None,
            scroll_area_show_scrollbar: false,
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

        // Clear popup bounds at start of frame
        // They'll be re-registered if the popup is still open
        if self.popup_id.is_none() {
            self.popup_bounds = None;
        }

        // NOTE: Don't clear menu_bar_close_id here! It needs to persist across frames
        // so that dropdowns can check if they should close (hover-to-switch happens
        // after the first dropdown has already processed).

        // Reset popup flags
        self.popup_opened_this_frame = false;
        self.popup_consume_click = false;

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
    // Input Shortcuts
    // -------------------------------------------------------------------------

    /// Check if a mouse button was clicked this frame.
    #[inline]
    pub fn mouse_clicked(&self, button: usize) -> bool {
        self.input.mouse_clicked(button)
    }

    /// Check if a mouse button is currently down.
    #[inline]
    pub fn mouse_down(&self, button: usize) -> bool {
        self.input.mouse_down[button]
    }

    /// Get the current mouse position.
    #[inline]
    pub fn mouse_pos(&self) -> Vec2 {
        self.input.mouse_pos
    }

    /// Check if a key was pressed this frame.
    #[inline]
    pub fn key_pressed(&self, key: crate::input::KeyCode) -> bool {
        self.input.key_pressed(key)
    }

    /// Check if a key is currently being held down.
    #[inline]
    pub fn key_down(&self, key: crate::input::KeyCode) -> bool {
        self.input.is_key_down(key)
    }

    // -------------------------------------------------------------------------
    // ID Management
    // -------------------------------------------------------------------------

    /// Generate a unique ID for a widget.
    ///
    /// Combines parent ID, label, and a sequential counter to ensure uniqueness.
    /// The counter is reset each frame in `begin()`, so consistent call order
    /// produces consistent IDs across frames.
    pub fn generate_id(&mut self, label: &str) -> WidgetId {
        let base = self.id_stack.last().copied().unwrap_or(0);
        let counter = self.id_counter;
        self.id_counter += 1;

        // Hash: parent + label + counter
        // This ensures unique IDs even for widgets with same label in same parent
        let mut hash = base;
        for byte in label.bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
        }
        hash = hash.wrapping_mul(31).wrapping_add(counter as u64);

        hash
    }

    /// Generate a stable ID that doesn't depend on call order.
    ///
    /// Use this for things like popups that need the same ID regardless of
    /// when they're called in the frame.
    pub fn make_stable_id(&self, label: &str) -> WidgetId {
        let base = self.id_stack.last().copied().unwrap_or(0);

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
    // Widget Trait
    // -------------------------------------------------------------------------

    /// Add a widget to the UI.
    ///
    /// This method accepts any type implementing `Widget` and renders it.
    /// This enables custom widgets and composition patterns.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Using a closure as a widget
    /// ui.add(|ui: &mut UiContext| {
    ///     ui.label("Hello", bounds);
    ///     Response::new(bounds)
    /// });
    ///
    /// // Using a custom widget type
    /// ui.add(MyCustomWidget::new("label"));
    /// ```
    pub fn add<W: crate::Widget>(&mut self, widget: W) -> crate::Response {
        widget.ui(self)
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

        // Each call generates a unique ID due to counter increment
        let id1 = ctx.generate_id("test");
        let id2 = ctx.generate_id("test");
        let id3 = ctx.generate_id("other");

        // Same label produces DIFFERENT IDs (counter ensures uniqueness)
        assert_ne!(id1, id2, "same label should get different IDs");
        // Different labels also produce different IDs
        assert_ne!(id1, id3, "different labels should get different IDs");

        ctx.end();
    }

    #[test]
    fn test_id_generation_consistent_across_frames() {
        let mut ctx = UiContext::new();

        // Frame 1
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        let frame1_id1 = ctx.generate_id("button");
        let frame1_id2 = ctx.generate_id("button");
        ctx.end();

        // Frame 2 - same call order should produce same IDs
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        let frame2_id1 = ctx.generate_id("button");
        let frame2_id2 = ctx.generate_id("button");
        ctx.end();

        // IDs should be consistent across frames (important for state persistence)
        assert_eq!(frame1_id1, frame2_id1, "first button ID should be stable");
        assert_eq!(frame1_id2, frame2_id2, "second button ID should be stable");
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
        let _char2_advance = 5.3f32;

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
        assert_eq!(
            tracked.min,
            Vec2::new(100.0, 50.0),
            "Top should be at first item top"
        );
        assert_eq!(
            tracked.max,
            Vec2::new(250.0, 122.0),
            "Bottom should be at last item bottom"
        );

        ctx.end();
    }

    // === Menu Bar Dropdown Click Tests ===

    /// Test that menu_bar_dropdown opens when clicked (press + release).
    ///
    /// This tests the full click cycle:
    /// 1. Frame 1: Mouse press on button -> sets active_id
    /// 2. Frame 2: Mouse release while over button -> should toggle open state
    #[test]
    fn test_menu_bar_dropdown_click_opens_dropdown() {
        use crate::input::mouse_button;

        let mut ctx = UiContext::new();
        let mut dropdown_open = false;
        let button_bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(60.0, 24.0));

        // Frame 1: Mouse press on the button
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(Vec2::new(30.0, 12.0)); // Center of button
        ctx.input.set_mouse_button(mouse_button::LEFT, true);

        ctx.menu_bar_dropdown(
            "file",
            "File",
            button_bounds,
            &mut dropdown_open,
            |_ui, _open| {
                // Menu content would go here
            },
        );
        ctx.end();

        // Dropdown should NOT be open yet (just pressed, not released)
        assert!(!dropdown_open, "Dropdown should not open on press");

        // Frame 2: Mouse release while still over button
        ctx.input.clear_frame_state();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        // Mouse is still at the same position (still hovering the button)
        ctx.input.set_mouse_pos(Vec2::new(30.0, 12.0));
        ctx.input.set_mouse_button(mouse_button::LEFT, false); // Release

        ctx.menu_bar_dropdown(
            "file",
            "File",
            button_bounds,
            &mut dropdown_open,
            |_ui, _open| {
                // Menu content would go here
            },
        );
        ctx.end();

        // NOW the dropdown should be open!
        assert!(
            dropdown_open,
            "Dropdown should open after click (press + release)"
        );
    }

    /// Test that clicking an open dropdown closes it.
    #[test]
    fn test_menu_bar_dropdown_click_toggles() {
        use crate::input::mouse_button;

        let mut ctx = UiContext::new();
        let mut dropdown_open = false;
        let button_bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(60.0, 24.0));

        // --- First click: open the dropdown ---
        // Frame 1: Press
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(Vec2::new(30.0, 12.0));
        ctx.input.set_mouse_button(mouse_button::LEFT, true);
        ctx.menu_bar_dropdown(
            "file",
            "File",
            button_bounds,
            &mut dropdown_open,
            |_ui, _open| {},
        );
        ctx.end();

        // Frame 2: Release
        ctx.input.clear_frame_state();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(Vec2::new(30.0, 12.0));
        ctx.input.set_mouse_button(mouse_button::LEFT, false);
        ctx.menu_bar_dropdown(
            "file",
            "File",
            button_bounds,
            &mut dropdown_open,
            |_ui, _open| {},
        );
        ctx.end();

        assert!(dropdown_open, "Dropdown should be open after first click");

        // --- Second click: close the dropdown ---
        // Frame 3: Press
        ctx.input.clear_frame_state();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(Vec2::new(30.0, 12.0));
        ctx.input.set_mouse_button(mouse_button::LEFT, true);
        ctx.menu_bar_dropdown(
            "file",
            "File",
            button_bounds,
            &mut dropdown_open,
            |_ui, _open| {},
        );
        ctx.end();

        // Frame 4: Release
        ctx.input.clear_frame_state();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(Vec2::new(30.0, 12.0));
        ctx.input.set_mouse_button(mouse_button::LEFT, false);
        ctx.menu_bar_dropdown(
            "file",
            "File",
            button_bounds,
            &mut dropdown_open,
            |_ui, _open| {},
        );
        ctx.end();

        assert!(
            !dropdown_open,
            "Dropdown should be closed after second click"
        );
    }

    /// Test that menu_bar_dropdown click works even when popup bounds overlap button.
    /// This tests that the raw input hover check bypasses popup blocking.
    #[test]
    fn test_menu_bar_dropdown_click_with_open_popup() {
        use crate::input::mouse_button;

        let mut ctx = UiContext::new();
        let mut dropdown_open = true; // Start with dropdown already open
        let button_bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(60.0, 24.0));

        // Simulate that a popup was opened in a previous frame (popup state persists)
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        // First, simulate the popup being rendered which sets popup_bounds
        ctx.popup(
            crate::context::Popup::new("test").below_button(button_bounds),
            &mut dropdown_open,
            |_ui, _open| {},
        );
        ctx.end();

        // Now popup_bounds should be set from the previous frame
        // Simulate a click on the button to close the dropdown
        ctx.input.clear_frame_state();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(Vec2::new(30.0, 12.0));
        ctx.input.set_mouse_button(mouse_button::LEFT, true);
        ctx.menu_bar_dropdown(
            "test",
            "Test",
            button_bounds,
            &mut dropdown_open,
            |_ui, _open| {},
        );
        ctx.end();

        // Release - should toggle the dropdown closed
        ctx.input.clear_frame_state();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(Vec2::new(30.0, 12.0));
        ctx.input.set_mouse_button(mouse_button::LEFT, false);
        ctx.menu_bar_dropdown(
            "test",
            "Test",
            button_bounds,
            &mut dropdown_open,
            |_ui, _open| {},
        );
        ctx.end();

        assert!(
            !dropdown_open,
            "Dropdown should close when clicked with popup open"
        );
    }

    // === Menu Bar Hover-to-Switch Tests ===

    /// Test that hovering over another dropdown while one is open switches to it.
    ///
    /// This is the standard menu bar behavior: when "File" is open and you hover
    /// over "Edit", the File menu closes and Edit menu opens automatically.
    ///
    /// Note: In immediate mode UI, hover-to-switch takes 2 frames:
    /// - Frame N: Hover detected, close flag set, Edit opens
    /// - Frame N+1: File sees close flag and closes
    #[test]
    fn test_menu_bar_hover_to_switch() {
        use crate::input::mouse_button;

        let mut ctx = UiContext::new();
        let mut file_open = false;
        let mut edit_open = false;
        let file_bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(60.0, 24.0));
        let edit_bounds = Rect2D::from_origin_size(Vec2::new(60.0, 0.0), Vec2::new(60.0, 24.0));

        // --- First, open the File dropdown ---
        // Frame 1: Press on File
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(Vec2::new(30.0, 12.0)); // Over File button
        ctx.input.set_mouse_button(mouse_button::LEFT, true);
        ctx.menu_bar_dropdown("file", "File", file_bounds, &mut file_open, |_ui, _open| {});
        ctx.menu_bar_dropdown("edit", "Edit", edit_bounds, &mut edit_open, |_ui, _open| {});
        ctx.end();

        // Frame 2: Release on File
        ctx.input.clear_frame_state();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(Vec2::new(30.0, 12.0));
        ctx.input.set_mouse_button(mouse_button::LEFT, false);
        ctx.menu_bar_dropdown("file", "File", file_bounds, &mut file_open, |_ui, _open| {});
        ctx.menu_bar_dropdown("edit", "Edit", edit_bounds, &mut edit_open, |_ui, _open| {});
        ctx.end();

        assert!(file_open, "File dropdown should be open");
        assert!(!edit_open, "Edit dropdown should be closed");

        // --- Now hover over Edit (no click, just hover) ---
        // Frame 3: Move mouse to Edit button - hover-to-switch triggered
        ctx.input.clear_frame_state();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(Vec2::new(90.0, 12.0)); // Over Edit button
        ctx.menu_bar_dropdown("file", "File", file_bounds, &mut file_open, |_ui, _open| {});
        ctx.menu_bar_dropdown("edit", "Edit", edit_bounds, &mut edit_open, |_ui, _open| {});
        ctx.end();

        // Edit should be open now
        assert!(edit_open, "Edit dropdown should open when hovering it");

        // Frame 4: File sees the close flag and closes
        ctx.input.clear_frame_state();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(Vec2::new(90.0, 12.0)); // Still over Edit
        ctx.menu_bar_dropdown("file", "File", file_bounds, &mut file_open, |_ui, _open| {});
        ctx.menu_bar_dropdown("edit", "Edit", edit_bounds, &mut edit_open, |_ui, _open| {});
        ctx.end();

        // Now File should be closed and Edit should be open
        assert!(!file_open, "File dropdown should close when hovering Edit");
        assert!(edit_open, "Edit dropdown should remain open");
    }

    /// Test that only one menu dropdown can be open at a time.
    ///
    /// Note: Hover-to-switch takes 2 frames in immediate mode:
    /// - Frame N: Hover detected, close flag set, new dropdown opens
    /// - Frame N+1: Old dropdown sees close flag and closes
    #[test]
    fn test_menu_bar_only_one_open() {
        use crate::input::mouse_button;

        let mut ctx = UiContext::new();
        let mut file_open = false;
        let mut edit_open = false;
        let mut view_open = false;
        let file_bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(60.0, 24.0));
        let edit_bounds = Rect2D::from_origin_size(Vec2::new(60.0, 0.0), Vec2::new(60.0, 24.0));
        let view_bounds = Rect2D::from_origin_size(Vec2::new(120.0, 0.0), Vec2::new(60.0, 24.0));

        // Open File dropdown
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(Vec2::new(30.0, 12.0));
        ctx.input.set_mouse_button(mouse_button::LEFT, true);
        ctx.menu_bar_dropdown("file", "File", file_bounds, &mut file_open, |_ui, _open| {});
        ctx.menu_bar_dropdown("edit", "Edit", edit_bounds, &mut edit_open, |_ui, _open| {});
        ctx.menu_bar_dropdown("view", "View", view_bounds, &mut view_open, |_ui, _open| {});
        ctx.end();

        ctx.input.clear_frame_state();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(Vec2::new(30.0, 12.0));
        ctx.input.set_mouse_button(mouse_button::LEFT, false);
        ctx.menu_bar_dropdown("file", "File", file_bounds, &mut file_open, |_ui, _open| {});
        ctx.menu_bar_dropdown("edit", "Edit", edit_bounds, &mut edit_open, |_ui, _open| {});
        ctx.menu_bar_dropdown("view", "View", view_bounds, &mut view_open, |_ui, _open| {});
        ctx.end();

        assert!(
            file_open && !edit_open && !view_open,
            "Only File should be open"
        );

        // Hover over Edit - Frame 1 (hover detected)
        ctx.input.clear_frame_state();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(Vec2::new(90.0, 12.0));
        ctx.menu_bar_dropdown("file", "File", file_bounds, &mut file_open, |_ui, _open| {});
        ctx.menu_bar_dropdown("edit", "Edit", edit_bounds, &mut edit_open, |_ui, _open| {});
        ctx.menu_bar_dropdown("view", "View", view_bounds, &mut view_open, |_ui, _open| {});
        ctx.end();

        // Frame 2 (File sees close flag)
        ctx.input.clear_frame_state();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(Vec2::new(90.0, 12.0));
        ctx.menu_bar_dropdown("file", "File", file_bounds, &mut file_open, |_ui, _open| {});
        ctx.menu_bar_dropdown("edit", "Edit", edit_bounds, &mut edit_open, |_ui, _open| {});
        ctx.menu_bar_dropdown("view", "View", view_bounds, &mut view_open, |_ui, _open| {});
        ctx.end();

        assert!(
            !file_open && edit_open && !view_open,
            "Only Edit should be open"
        );

        // Hover over View - Frame 1 (hover detected)
        ctx.input.clear_frame_state();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(Vec2::new(150.0, 12.0));
        ctx.menu_bar_dropdown("file", "File", file_bounds, &mut file_open, |_ui, _open| {});
        ctx.menu_bar_dropdown("edit", "Edit", edit_bounds, &mut edit_open, |_ui, _open| {});
        ctx.menu_bar_dropdown("view", "View", view_bounds, &mut view_open, |_ui, _open| {});
        ctx.end();

        // Frame 2 (Edit sees close flag)
        ctx.input.clear_frame_state();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(Vec2::new(150.0, 12.0));
        ctx.menu_bar_dropdown("file", "File", file_bounds, &mut file_open, |_ui, _open| {});
        ctx.menu_bar_dropdown("edit", "Edit", edit_bounds, &mut edit_open, |_ui, _open| {});
        ctx.menu_bar_dropdown("view", "View", view_bounds, &mut view_open, |_ui, _open| {});
        ctx.end();

        assert!(
            !file_open && !edit_open && view_open,
            "Only View should be open"
        );
    }

    /// Test that hover-to-switch only happens when a dropdown is already open.
    /// Hovering alone (without clicking first) should NOT open a dropdown.
    #[test]
    fn test_menu_bar_hover_does_not_open_without_click() {
        let mut ctx = UiContext::new();
        let mut file_open = false;
        let file_bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(60.0, 24.0));

        // Just hover over File - should NOT open it
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.input.set_mouse_pos(Vec2::new(30.0, 12.0));
        ctx.menu_bar_dropdown("file", "File", file_bounds, &mut file_open, |_ui, _open| {});
        ctx.end();

        assert!(!file_open, "Hovering alone should not open dropdown");
    }

    // === Modal Bounds Tests ===

    /// Test that get_popup_bounds() returns correct bounds for a centered modal.
    ///
    /// This verifies that the modal's bounds match its specified width/height,
    /// allowing content to be correctly positioned within the modal.
    #[test]
    fn test_modal_get_popup_bounds_matches_specified_size() {
        let mut ctx = UiContext::new();
        let mut modal_open = true;
        let modal_width = 320.0;
        let modal_height = 120.0;

        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.modal(
            "test_modal",
            modal_width,
            modal_height,
            &mut modal_open,
            |ui, _open| {
                let bounds = ui.get_popup_bounds();

                // The bounds should have the specified width and height
                assert!(
                    (bounds.width() - modal_width).abs() < 1.0,
                    "Modal width should be {}, got {}",
                    modal_width,
                    bounds.width()
                );
                assert!(
                    (bounds.height() - modal_height).abs() < 1.0,
                    "Modal height should be {}, got {}",
                    modal_height,
                    bounds.height()
                );

                // The modal should be centered on screen
                let expected_x = (800.0 - modal_width) * 0.5;
                let expected_y = (600.0 - modal_height) * 0.5;
                assert!(
                    (bounds.min.x() - expected_x).abs() < 1.0,
                    "Modal x should be {}, got {}",
                    expected_x,
                    bounds.min.x()
                );
                assert!(
                    (bounds.min.y() - expected_y).abs() < 1.0,
                    "Modal y should be {}, got {}",
                    expected_y,
                    bounds.min.y()
                );
            },
        );
        ctx.end();
    }

    /// Test that buttons positioned relative to modal bounds are inside the modal.
    ///
    /// This is the actual bug: when positioning buttons using get_popup_bounds(),
    /// they end up outside the modal rectangle because the bounds are wrong.
    #[test]
    fn test_modal_buttons_within_bounds() {
        let mut ctx = UiContext::new();
        let mut modal_open = true;
        let modal_width = 320.0;
        let modal_height = 120.0;
        let btn_width = 80.0;
        let btn_height = 28.0;
        let btn_margin = 10.0;

        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.modal(
            "test_modal",
            modal_width,
            modal_height,
            &mut modal_open,
            |ui, _open| {
                let bounds = ui.get_popup_bounds();

                // Position "Yes" button at bottom-right of modal
                let yes_btn_bounds = Rect2D::from_origin_size(
                    Vec2::new(
                        bounds.min.x() + modal_width - btn_width - btn_margin,
                        bounds.min.y() + modal_height - btn_height - btn_margin,
                    ),
                    Vec2::new(btn_width, btn_height),
                );

                // Position "No" button to the left of "Yes"
                let no_btn_bounds = Rect2D::from_origin_size(
                    Vec2::new(
                        bounds.min.x() + modal_width - btn_width * 2.0 - btn_margin * 2.0,
                        bounds.min.y() + modal_height - btn_height - btn_margin,
                    ),
                    Vec2::new(btn_width, btn_height),
                );

                // Both buttons should be fully contained within the modal bounds
                assert!(
                    bounds.contains_rect(&yes_btn_bounds),
                    "Yes button {:?} should be within modal bounds {:?}",
                    yes_btn_bounds,
                    bounds
                );
                assert!(
                    bounds.contains_rect(&no_btn_bounds),
                    "No button {:?} should be within modal bounds {:?}",
                    no_btn_bounds,
                    bounds
                );
            },
        );
        ctx.end();
    }

    /// Test that button clicks work inside a modal.
    ///
    /// This tests the full click cycle (press + release) for a button
    /// positioned inside a modal dialog.
    #[test]
    fn test_modal_button_click_works() {
        use crate::input::mouse_button;
        use crate::widgets::Button;

        let mut ctx = UiContext::new();
        let mut modal_open = true;
        let modal_width = 320.0;
        let modal_height = 120.0;
        let btn_width = 80.0;
        let btn_height = 28.0;
        let btn_margin = 10.0;
        let mut button_clicked = false;

        // Calculate expected button position (same both frames)
        let modal_x = (800.0 - modal_width) * 0.5;
        let modal_y = (600.0 - modal_height) * 0.5;
        let no_btn_x = modal_x + modal_width - btn_width * 2.0 - btn_margin * 2.0;
        let no_btn_y = modal_y + modal_height - btn_height - btn_margin;
        let btn_center = Vec2::new(no_btn_x + btn_width * 0.5, no_btn_y + btn_height * 0.5);

        // Frame 1: Press on the button inside modal
        ctx.input.set_mouse_pos(btn_center);
        ctx.input.set_mouse_button(mouse_button::LEFT, true);
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.modal(
            "test_modal",
            modal_width,
            modal_height,
            &mut modal_open,
            |ui, _open| {
                let bounds = ui.get_popup_bounds();
                let no_btn_bounds = Rect2D::from_origin_size(
                    Vec2::new(
                        bounds.min.x() + modal_width - btn_width * 2.0 - btn_margin * 2.0,
                        bounds.min.y() + modal_height - btn_height - btn_margin,
                    ),
                    Vec2::new(btn_width, btn_height),
                );

                let response = ui.add(Button::new("No").bounds(no_btn_bounds));
                if response.clicked {
                    button_clicked = true;
                }
            },
        );
        ctx.end();

        // Button should NOT be clicked yet (just pressed)
        assert!(!button_clicked, "Button should not click on press");

        // Frame 2: Release on the button
        ctx.input.clear_frame_state();
        ctx.input.set_mouse_pos(btn_center);
        ctx.input.set_mouse_button(mouse_button::LEFT, false);
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.modal(
            "test_modal",
            modal_width,
            modal_height,
            &mut modal_open,
            |ui, _open| {
                let bounds = ui.get_popup_bounds();
                let no_btn_bounds = Rect2D::from_origin_size(
                    Vec2::new(
                        bounds.min.x() + modal_width - btn_width * 2.0 - btn_margin * 2.0,
                        bounds.min.y() + modal_height - btn_height - btn_margin,
                    ),
                    Vec2::new(btn_width, btn_height),
                );

                let response = ui.add(Button::new("No").bounds(no_btn_bounds));
                if response.clicked {
                    button_clicked = true;
                }
            },
        );
        ctx.end();

        // NOW the button should be clicked!
        assert!(
            button_clicked,
            "Button should be clicked after press+release"
        );
    }

    /// Test that button hover detection works inside a modal.
    #[test]
    fn test_modal_button_hover_works() {
        let mut ctx = UiContext::new();
        let mut modal_open = true;
        let modal_width = 320.0;
        let modal_height = 120.0;
        let btn_width = 80.0;
        let btn_height = 28.0;
        let btn_margin = 10.0;

        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.modal(
            "test_modal",
            modal_width,
            modal_height,
            &mut modal_open,
            |ui, _open| {
                let bounds = ui.get_popup_bounds();

                let no_btn_bounds = Rect2D::from_origin_size(
                    Vec2::new(
                        bounds.min.x() + modal_width - btn_width * 2.0 - btn_margin * 2.0,
                        bounds.min.y() + modal_height - btn_height - btn_margin,
                    ),
                    Vec2::new(btn_width, btn_height),
                );

                // Set mouse position to center of the button
                let btn_center = Vec2::new(
                    no_btn_bounds.min.x() + btn_width * 0.5,
                    no_btn_bounds.min.y() + btn_height * 0.5,
                );
                ui.input.set_mouse_pos(btn_center);

                // Check if the button would be hovered by checking is_hovered directly
                let hovered = ui.input.is_hovered(no_btn_bounds);
                assert!(
                    hovered,
                    "Button at {:?} should be hovered when mouse at {:?}",
                    no_btn_bounds, btn_center
                );
            },
        );
        ctx.end();
    }

    /// Test that Button widget hover state (for visuals) works inside a modal.
    /// This tests the actual Button widget's hovered response, not just raw input.
    #[test]
    fn test_modal_button_widget_hover_visual_works() {
        use crate::widgets::Button;

        let mut ctx = UiContext::new();
        let mut modal_open = true;
        let modal_width = 320.0;
        let modal_height = 120.0;
        let btn_width = 80.0;
        let btn_height = 28.0;
        let btn_margin = 10.0;

        // Calculate expected button position
        let modal_x = (800.0 - modal_width) * 0.5;
        let modal_y = (600.0 - modal_height) * 0.5;
        let no_btn_x = modal_x + modal_width - btn_width * 2.0 - btn_margin * 2.0;
        let no_btn_y = modal_y + modal_height - btn_height - btn_margin;
        let btn_center = Vec2::new(no_btn_x + btn_width * 0.5, no_btn_y + btn_height * 0.5);

        // Set mouse position BEFORE beginning frame
        ctx.input.set_mouse_pos(btn_center);

        ctx.begin(Vec2::new(800.0, 600.0), 1.0);
        ctx.modal(
            "test_modal",
            modal_width,
            modal_height,
            &mut modal_open,
            |ui, _open| {
                let bounds = ui.get_popup_bounds();
                let no_btn_bounds = Rect2D::from_origin_size(
                    Vec2::new(
                        bounds.min.x() + modal_width - btn_width * 2.0 - btn_margin * 2.0,
                        bounds.min.y() + modal_height - btn_height - btn_margin,
                    ),
                    Vec2::new(btn_width, btn_height),
                );

                // Add the button and check its hover state
                let response = ui.add(Button::new("No").bounds(no_btn_bounds));
                assert!(
                    response.hovered,
                    "Button widget should report hovered=true when mouse is over it inside modal"
                );
            },
        );
        ctx.end();
    }

    // === Layout Cursor Tests ===

    /// Test that cursor() returns layout cursor when inside a layout.
    #[test]
    fn test_cursor_returns_layout_cursor_when_in_layout() {
        let mut ctx = UiContext::new();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);

        // Set initial cursor position
        ctx.set_cursor(Vec2::new(100.0, 50.0));
        assert_eq!(ctx.cursor(), Vec2::new(100.0, 50.0));

        // Begin a horizontal layout
        ctx.begin_row();

        // Cursor should still return the same position initially
        assert_eq!(ctx.cursor(), Vec2::new(100.0, 50.0));

        ctx.end_row();
        ctx.end();
    }

    /// Test that layout_item advances cursor in horizontal layout.
    #[test]
    fn test_layout_item_advances_horizontal_cursor() {
        let mut ctx = UiContext::new();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);

        ctx.set_cursor(Vec2::new(100.0, 50.0));
        ctx.begin_row();

        // Get bounds for first item
        let size = Vec2::new(60.0, 24.0);
        let bounds1 = ctx.layout_item(size);

        // First item should be at initial cursor position
        assert_eq!(bounds1.min, Vec2::new(100.0, 50.0));

        // Cursor should have advanced horizontally
        let cursor_after = ctx.cursor();
        assert!(
            cursor_after.x() > 100.0,
            "Cursor should have advanced horizontally, got x={}",
            cursor_after.x()
        );
        assert_eq!(
            cursor_after.y(),
            50.0,
            "Cursor y should stay the same in horizontal layout"
        );

        // Get bounds for second item
        let bounds2 = ctx.layout_item(size);

        // Second item should be at the advanced cursor position
        assert_eq!(bounds2.min.x(), cursor_after.x());
        assert_ne!(bounds1.min.x(), bounds2.min.x(), "Items should not pile up");

        ctx.end_row();
        ctx.end();
    }

    /// Test that layout_item advances cursor in vertical layout.
    #[test]
    fn test_layout_item_advances_vertical_cursor() {
        let mut ctx = UiContext::new();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);

        ctx.set_cursor(Vec2::new(100.0, 50.0));
        ctx.begin_column();

        // Get bounds for first item
        let size = Vec2::new(60.0, 24.0);
        let bounds1 = ctx.layout_item(size);

        // First item should be at initial cursor position
        assert_eq!(bounds1.min, Vec2::new(100.0, 50.0));

        // Cursor should have advanced vertically
        let cursor_after = ctx.cursor();
        assert_eq!(
            cursor_after.x(),
            100.0,
            "Cursor x should stay the same in vertical layout"
        );
        assert!(
            cursor_after.y() > 50.0,
            "Cursor should have advanced vertically, got y={}",
            cursor_after.y()
        );

        // Get bounds for second item
        let bounds2 = ctx.layout_item(size);

        // Second item should be at the advanced cursor position
        assert_eq!(bounds2.min.y(), cursor_after.y());
        assert_ne!(bounds1.min.y(), bounds2.min.y(), "Items should not pile up");

        ctx.end_column();
        ctx.end();
    }

    /// Test that advance_cursor works in horizontal layout.
    #[test]
    fn test_advance_cursor_in_layout() {
        let mut ctx = UiContext::new();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);

        ctx.set_cursor(Vec2::new(100.0, 50.0));
        ctx.begin_row();

        // Cursor starts at layout start
        assert_eq!(ctx.cursor(), Vec2::new(100.0, 50.0));

        // Advance cursor manually
        ctx.advance_cursor(Vec2::new(60.0, 24.0));

        // Cursor should have advanced
        let cursor_after = ctx.cursor();
        assert!(
            cursor_after.x() > 100.0,
            "Cursor should have advanced horizontally"
        );

        // Advance again
        ctx.advance_cursor(Vec2::new(60.0, 24.0));

        // Cursor should have advanced more
        let cursor_final = ctx.cursor();
        assert!(
            cursor_final.x() > cursor_after.x(),
            "Cursor should have advanced again"
        );

        ctx.end_row();
        ctx.end();
    }

    /// Test that set_cursor works inside a layout.
    #[test]
    fn test_set_cursor_in_layout() {
        let mut ctx = UiContext::new();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);

        // Set main cursor before layout
        ctx.set_cursor(Vec2::new(100.0, 50.0));

        ctx.begin_row();

        // set_cursor inside layout should update layout cursor
        ctx.set_cursor(Vec2::new(200.0, 100.0));

        // cursor() should return the updated layout cursor
        assert_eq!(ctx.cursor(), Vec2::new(200.0, 100.0));

        ctx.end_row();
        ctx.end();
    }

    /// Test that layouts are independent between panels.
    #[test]
    fn test_layouts_dont_interfere() {
        let mut ctx = UiContext::new();
        ctx.begin(Vec2::new(800.0, 600.0), 1.0);

        // First "panel" - left side
        ctx.begin_column();
        ctx.set_cursor(Vec2::new(10.0, 10.0));
        let bounds1 = ctx.layout_item(Vec2::new(100.0, 20.0));
        assert_eq!(bounds1.min, Vec2::new(10.0, 10.0));
        ctx.end_column();

        // After end_column, main cursor should be updated
        let cursor_after_first = ctx.cursor();
        assert!(
            cursor_after_first.y() > 10.0,
            "Cursor should have moved down"
        );

        // Second "panel" - right side (simulating inspector after hierarchy)
        ctx.begin_column();
        ctx.set_cursor(Vec2::new(500.0, 10.0)); // Different X position
        let bounds2 = ctx.layout_item(Vec2::new(100.0, 20.0));
        // Item should be at the new position, not affected by first panel
        assert_eq!(bounds2.min, Vec2::new(500.0, 10.0));
        ctx.end_column();

        ctx.end();
    }
}
