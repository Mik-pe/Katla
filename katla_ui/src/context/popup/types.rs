//! Popup configuration types.

use katla_math::{Rect2D, Vec2};

/// Popup positioning mode.
#[derive(Debug, Clone, Copy)]
pub enum PopupPosition {
    /// Position at current cursor (context menu style).
    AtCursor,
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
#[derive(Debug, Clone, Copy)]
pub struct Popup<'a> {
    pub(crate) id: &'a str,
    pub(crate) position: PopupPosition,
    pub(crate) style: PopupStyle,
    pub(crate) close_behavior: CloseBehavior,
}

impl<'a> Popup<'a> {
    /// Create a new popup configuration with the given ID.
    ///
    /// Default configuration:
    /// - Position: AtCursor
    /// - Style: Menu
    /// - Close behavior: ClickOutside
    pub fn new(id: &'a str) -> Self {
        Self {
            id,
            position: PopupPosition::AtCursor,
            style: PopupStyle::Menu,
            close_behavior: CloseBehavior::ClickOutside,
        }
    }

    /// Position popup at the current cursor position.
    pub fn at_cursor(mut self) -> Self {
        self.position = PopupPosition::AtCursor;
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

    /// Use modal style (dark overlay, centered, explicit close only).
    pub fn modal(mut self) -> Self {
        self.style = PopupStyle::Modal;
        self.close_behavior = CloseBehavior::ExplicitOnly;
        self
    }
}
