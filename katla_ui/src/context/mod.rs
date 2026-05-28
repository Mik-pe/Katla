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

use std::cell::RefCell;
use std::rc::Rc;

#[cfg(test)]
use katla_math::Color;
use katla_math::{Rect2D, Vec2};

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
    /// Font system for text rendering (shared via Rc<RefCell<>>).
    pub(crate) fonts: Rc<RefCell<FontSystem>>,
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
    /// Label of a text input that should receive focus when next encountered.
    pending_focus_label: Option<String>,
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
    /// Highest z-index from the previous frame. Used to block hover for
    /// widgets that are drawn before higher-z content has re-registered.
    /// This prevents e.g. a popup's hover-z from being lost on the first
    /// frame after begin() clears hover_z_index.
    prev_hover_z_index: u32,
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
    /// Per-widget text input state (cursor, selection).
    pub(crate) text_input_states: std::collections::HashMap<WidgetId, TextInputState>,
    /// Clipboard provider for copy/cut/paste.
    clipboard: Option<Box<dyn ClipboardProvider>>,
    /// Focusable widgets registered during this frame's layout pass.
    /// Each entry is (widget_id, bounds), collected in layout order for Tab navigation.
    pub(crate) focusable_widgets: Vec<(WidgetId, Rect2D)>,
    /// Tooltips deferred via `Response::tooltip()`, rendered in `end()`.
    pub(crate) pending_tooltips: Vec<(u64, String)>,
    /// Panel regions registered during layout for focus tracking.
    pub(crate) panel_regions: Vec<(u64, Rect2D)>,
    /// Currently focused panel ID, determined by mouse click hit-testing.
    focused_panel_id: Option<u64>,
    /// Whether the declarative view tree consumed input this frame.
    declarative_input_consumed: bool,
    /// Temporary typed data slots for declarative Custom draw functions.
    /// Set before `ViewTree::frame()` and read during `Custom` draw dispatch.
    scratch_data: std::collections::HashMap<std::any::TypeId, Box<dyn std::any::Any>>,
}

impl UiContext {
    /// Create a new UI context with the default dark theme.
    pub fn new() -> Self {
        Self {
            draw_list: DrawList::new(),
            input: UiInputState::new(),
            style: UiStyle::dark(),
            fonts: Rc::new(RefCell::new(FontSystem::new())),
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
            pending_focus_label: None,
            in_frame: false,
            cursor: Vec2::new(0.0, 0.0),
            row_height: 0.0,
            layout_stack: Vec::new(),
            popup_id: None,
            menu_bar_close_id: None,
            popup_position: None,
            popup_bounds: None,
            popup_opened_this_frame: false,
            z_index: z_index::DEFAULT,
            z_stack: Vec::new(),
            hover_z_index: z_index::DEFAULT,
            prev_hover_z_index: z_index::DEFAULT,
            popup_content_bounds: None,
            popup_cursor: Vec2::new(0.0, 0.0),
            popup_width: 0.0,
            scroll_area_bounds: None,
            scroll_area_content_bounds: None,
            scroll_area_state: None,
            scroll_area_show_scrollbar: false,
            time: 0.0,
            last_input_time: 0.0,
            text_input_states: std::collections::HashMap::new(),
            clipboard: None,
            focusable_widgets: Vec::new(),
            pending_tooltips: Vec::new(),
            panel_regions: Vec::new(),
            focused_panel_id: None,
            declarative_input_consumed: false,
            scratch_data: std::collections::HashMap::new(),
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
    pub fn fonts(&self) -> std::cell::Ref<'_, FontSystem> {
        self.fonts.borrow()
    }

    /// Access the font system mutably.
    pub fn fonts_mut(&self) -> std::cell::RefMut<'_, FontSystem> {
        self.fonts.borrow_mut()
    }

    /// Set the clipboard provider for copy/cut/paste operations.
    pub fn set_clipboard_provider(&mut self, provider: Box<dyn ClipboardProvider>) {
        self.clipboard = Some(provider);
    }

    /// Copy text to the clipboard, if a clipboard provider is set.
    pub fn copy_to_clipboard(&mut self, text: &str) {
        if let Some(cb) = &mut self.clipboard {
            cb.set(text);
        }
    }

    /// Register a panel region for focus tracking.
    pub fn register_panel(&mut self, id: u64, bounds: Rect2D) {
        self.panel_regions.push((id, bounds));
    }

    /// Get the currently focused panel ID, if any.
    pub fn focused_panel(&self) -> Option<u64> {
        self.focused_panel_id
    }

    /// Request focus for a text input by label.
    ///
    /// The label must match the label passed to the widget's constructor
    /// (e.g. the first argument of `TextInput::new`). The next text input
    /// with a matching label will receive focus automatically.
    pub fn request_focus(&mut self, label: &str) {
        self.pending_focus_label = Some(label.to_string());
    }

    /// Whether the declarative view tree consumed input this frame.
    pub fn is_input_consumed_by_declarative(&self) -> bool {
        self.declarative_input_consumed
    }

    /// Set whether the declarative view tree consumed input this frame.
    pub fn set_declarative_input_consumed(&mut self, consumed: bool) {
        self.declarative_input_consumed = consumed;
    }

    /// Store a typed value in the scratch data slot, available to declarative
    /// `Custom` draw functions during this frame.
    pub fn set_scratch<T: Clone + 'static>(&mut self, value: T) {
        self.scratch_data
            .insert(std::any::TypeId::of::<T>(), Box::new(value));
    }

    /// Retrieve a typed value from the scratch data slot.
    pub fn get_scratch<T: Clone + 'static>(&self) -> Option<&T> {
        self.scratch_data
            .get(&std::any::TypeId::of::<T>())
            .and_then(|v| v.downcast_ref::<T>())
    }

    /// Retrieve a typed mutable reference from the scratch data slot.
    pub fn get_scratch_mut<T: Clone + 'static>(&mut self) -> Option<&mut T> {
        self.scratch_data
            .get_mut(&std::any::TypeId::of::<T>())
            .and_then(|v| v.downcast_mut::<T>())
    }

    /// Clear all scratch data slots (typically at the start of each frame).
    pub fn clear_scratch(&mut self) {
        self.scratch_data.clear();
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

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
