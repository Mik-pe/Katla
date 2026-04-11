//! UI context - the main entry point for UI rendering.
//!
//! The `UiContext` manages all UI state and provides the immediate mode API
//! for building user interfaces.

mod clipping;
mod drawing;
mod frame;
mod helpers;
mod id;
mod input;
pub(crate) mod interaction;
mod layout;
use layout::LayoutState;
mod popup;
mod widgets;
pub mod z_index;

use katla_math::{Color, Rect2D, Vec2};

use crate::draw_list::DrawList;
use crate::input::UiInputState;
use crate::style::UiStyle;
use crate::text::{FontId, FontSystem};
use crate::widget::ClipboardProvider;

pub use popup::{CloseBehavior, Popup, PopupPosition, PopupStyle};
pub use widgets::{ScrollArea, ScrollAreaState};

/// ID type for UI elements.
pub type WidgetId = u64;

/// Per-widget state tracked for text input fields across frames.
#[derive(Debug, Clone)]
pub struct TextInputState {
    /// Byte offset of the cursor within the text.
    pub cursor: usize,
    /// Byte offset of the selection anchor (the other end of the selection).
    /// When equal to `cursor`, there is no selection.
    pub selection_anchor: usize,
    /// Horizontal scroll offset so the cursor stays visible.
    pub scroll_offset: f32,
}

impl Default for TextInputState {
    fn default() -> Self {
        Self {
            cursor: 0,
            selection_anchor: 0,
            scroll_offset: 0.0,
        }
    }
}

impl TextInputState {
    /// Create a new state with cursor and anchor at the end of the text.
    pub fn at_end(text: &str) -> Self {
        let len = text.len();
        Self {
            cursor: len,
            selection_anchor: len,
            scroll_offset: 0.0,
        }
    }

    /// Returns the byte range of the current selection.
    /// The range is sorted so start <= end.
    pub fn selection_range(&self) -> (usize, usize) {
        let start = self.cursor.min(self.selection_anchor);
        let end = self.cursor.max(self.selection_anchor);
        (start, end)
    }

    /// Whether there is an active selection.
    pub fn has_selection(&self) -> bool {
        self.cursor != self.selection_anchor
    }

    /// Reset both cursor and anchor to 0.
    pub fn clear(&mut self) {
        self.cursor = 0;
        self.selection_anchor = 0;
    }
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

/// RAII guard for window clip management.
///
/// Automatically pops the clip when dropped.
///
/// # Example
/// ```ignore
/// {
///     let win = ui.begin_window_guard("my_window", Some("Title"), bounds);
///     // draw window content using win.state
/// } // auto-pops clip
/// ```
pub struct WindowGuard<'a> {
    /// Window state for accessing cursor/bounds info.
    pub state: WindowState,
    ctx: &'a mut UiContext,
}

impl Drop for WindowGuard<'_> {
    fn drop(&mut self) {
        self.ctx.pop_clip();
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
    pub(crate) input: UiInputState,
    /// Style configuration.
    pub(crate) style: UiStyle,
    /// Font system for text rendering.
    pub(crate) fonts: FontSystem,
    /// Currently active font.
    pub(crate) current_font: FontId,
    /// Current screen size (logical pixels).
    pub(crate) screen_size: Vec2,
    /// DPI scale factor (physical pixels per logical pixel).
    pub(crate) scale_factor: f32,
    /// Font scale multiplier for accessibility (1.0 = 100%).
    pub(super) font_scale: f32,
    /// Stack of clipping rectangles.
    pub(super) clip_stack: Vec<Rect2D>,
    /// Stack of widget IDs for nesting.
    pub(super) id_stack: Vec<WidgetId>,
    /// Counter for generating unique IDs.
    pub(super) id_counter: u32,
    /// Currently hovered widget.
    pub(crate) hovered_id: Option<WidgetId>,
    /// Currently active (pressed) widget.
    pub(crate) active_id: Option<WidgetId>,
    /// Currently focused widget (for text input).
    pub(crate) focused_id: Option<WidgetId>,
    /// Whether we're inside a begin()/end() pair.
    pub(super) in_frame: bool,
    /// Layout cursor for automatic positioning.
    pub(crate) cursor: Vec2,
    /// Current row height for layout.
    pub(super) row_height: f32,
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
    /// Current time in seconds (for cursor blink animation).
    pub(crate) time: f64,
    /// Time of last keyboard input (for cursor blink grace period).
    pub(crate) last_input_time: f64,
    /// Current Z-index for rendering (higher = on top).
    pub(super) z_index: u32,
    /// Z-index stack for nested containers.
    pub(super) z_stack: Vec<u32>,
    /// Highest z-index that the mouse is currently hovering over.
    /// Updated automatically by `draw_rect`. Widgets check against this to
    /// prevent interaction when covered by higher-z content.
    pub(crate) hover_z_index: u32,
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
    /// Scratch buffer for graph widget point computation (avoids per-frame allocation).
    scratch_points: Vec<Vec2>,
    /// Per-widget text input state (cursor, selection).
    pub(crate) text_input_states: std::collections::HashMap<WidgetId, TextInputState>,
    /// Clipboard provider for copy/cut/paste.
    clipboard: Option<Box<dyn ClipboardProvider>>,
    /// Focusable widgets registered during this frame's layout pass.
    /// Each entry is (widget_id, bounds), collected in layout order for Tab navigation.
    pub(crate) focusable_widgets: Vec<(WidgetId, Rect2D)>,
    /// Tooltips deferred via `Response::tooltip()`, rendered in `end()`.
    pub(crate) pending_tooltips: Vec<(u64, String)>,
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
            focused_id: None,
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
            hover_z_index: z_index::DEFAULT,
            popup_content_bounds: None,
            popup_cursor: Vec2::new(0.0, 0.0),
            popup_width: 0.0,
            scroll_area_bounds: None,
            scroll_area_content_bounds: None,
            scroll_area_state: None,
            scroll_area_show_scrollbar: false,
            scratch_points: Vec::new(),
            time: 0.0,
            last_input_time: 0.0,
            text_input_states: std::collections::HashMap::new(),
            clipboard: None,
            focusable_widgets: Vec::new(),
            pending_tooltips: Vec::new(),
        }
    }

    /// Create a new UI context with a specific style.
    pub fn with_style(style: UiStyle) -> Self {
        Self {
            style,
            ..Self::new()
        }
    }

    // -------------------------------------------------------------------------
    // Field Accessors
    // -------------------------------------------------------------------------

    /// Access the input state.
    pub fn input(&self) -> &UiInputState {
        &self.input
    }

    /// Access the input state mutably.
    pub fn input_mut(&mut self) -> &mut UiInputState {
        &mut self.input
    }

    /// Access the style configuration.
    pub fn style(&self) -> &UiStyle {
        &self.style
    }

    /// Access the style configuration mutably.
    pub fn style_mut(&mut self) -> &mut UiStyle {
        &mut self.style
    }

    /// Access the font system.
    pub fn fonts(&self) -> &FontSystem {
        &self.fonts
    }

    /// Access the font system mutably.
    pub fn fonts_mut(&mut self) -> &mut FontSystem {
        &mut self.fonts
    }

    /// Set the clipboard provider for copy/cut/paste operations.
    pub fn set_clipboard_provider(&mut self, provider: Box<dyn ClipboardProvider>) {
        self.clipboard = Some(provider);
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
        let response = widget.ui(self);
        self.advance_cursor(katla_math::Vec2::new(
            response.bounds.width(),
            response.bounds.height(),
        ));
        response
    }

    pub fn add_overlay<W: crate::Widget>(&mut self, widget: W) -> crate::Response {
        widget.ui(self)
    }

    // -------------------------------------------------------------------------
    // Z-Index Management
    // -------------------------------------------------------------------------

    /// Defer a tooltip for rendering during `end()`.
    pub fn defer_tooltip(&mut self, text: impl Into<String>) {
        self.pending_tooltips.push((0, text.into()));
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
#[path = "tests.rs"]
mod tests;
