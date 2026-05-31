//! UI context - the main entry point for UI rendering.
//!
//! The `UiContext` manages all UI state and provides the immediate mode API
//! for building user interfaces.

mod clipping;
mod drawing;
mod frame;
mod id;
mod input;
mod widgets;
pub mod z_index;

use std::cell::RefCell;
use std::rc::Rc;

use katla_math::{Rect2D, Vec2};

use crate::draw_list::DrawList;
use crate::input::UiInputState;
use crate::style::UiStyle;
use crate::text::{FontId, FontSystem};
use crate::widget::ClipboardProvider;

pub use widgets::ScrollAreaState;

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
    /// Current time in seconds (for cursor blink animation).
    pub(crate) time: f64,
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
            z_index: z_index::DEFAULT,
            z_stack: Vec::new(),
            hover_z_index: z_index::DEFAULT,
            prev_hover_z_index: z_index::DEFAULT,
            time: 0.0,
            clipboard: None,
            focusable_widgets: Vec::new(),
            pending_tooltips: Vec::new(),
            panel_regions: Vec::new(),
            focused_panel_id: None,
            declarative_input_consumed: false,
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
    /// The label must match the label passed to the widget's constructor.
    /// The next widget with a matching label will receive focus automatically.
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
