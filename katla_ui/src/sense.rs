//! Interaction sensing for widgets.
//!
//! `Sense` describes what kind of interaction a widget should sense.
//! This allows explicit control over widget behavior.

use std::fmt;

bitflags::bitflags! {
    /// What kind of interaction should a widget sense?
    ///
    /// By default, widgets sense clicks. Use `Sense::drag()` for sliders,
    /// or `Sense::click_and_drag()` for widgets that respond to both.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Sense: u8 {
        /// Sense clicks (button released while hovered).
        const CLICK = 1 << 0;
        /// Sense drags (mouse movement while held).
        const DRAG = 1 << 1;
        /// Widget can receive keyboard focus.
        const FOCUSABLE = 1 << 2;
    }
}

impl Sense {
    /// Sense clicks and be focusable (default for buttons).
    pub fn click() -> Self {
        Self::CLICK | Self::FOCUSABLE
    }

    /// Sense drags and be focusable (for sliders).
    pub fn drag() -> Self {
        Self::DRAG | Self::FOCUSABLE
    }

    /// Sense both clicks and drags (for e.g. draggable list items).
    pub fn click_and_drag() -> Self {
        Self::CLICK | Self::DRAG | Self::FOCUSABLE
    }

    /// No interaction - just show content.
    pub fn nothing() -> Self {
        Self::empty()
    }

    /// Is this widget focusable?
    pub fn focusable(&self) -> bool {
        self.contains(Self::FOCUSABLE)
    }

    /// Does this widget sense clicks?
    pub fn clicks(&self) -> bool {
        self.contains(Self::CLICK)
    }

    /// Does this widget sense drags?
    pub fn drags(&self) -> bool {
        self.contains(Self::DRAG)
    }

    /// Does this widget sense any interaction?
    pub fn interactive(&self) -> bool {
        !self.is_empty()
    }
}

impl Default for Sense {
    fn default() -> Self {
        Self::click()
    }
}

impl fmt::Display for Sense {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            return write!(f, "Sense::nothing()");
        }
        let mut parts = Vec::new();
        if self.contains(Self::CLICK) {
            parts.push("click");
        }
        if self.contains(Self::DRAG) {
            parts.push("drag");
        }
        if self.contains(Self::FOCUSABLE) {
            parts.push("focusable");
        }
        write!(f, "Sense::{}", parts.join("+"))
    }
}
