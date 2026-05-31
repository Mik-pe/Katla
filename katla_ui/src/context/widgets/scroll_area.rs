//! Scroll area state type.
//!
//! `ScrollAreaState` is the persisted state for scrollable containers,
//! used by the declarative system and by immediate-mode callers.

/// Scroll area state tracked between frames.
#[derive(Debug, Clone, Copy)]
pub struct ScrollAreaState {
    /// Current scroll offset.
    pub scroll_offset: f32,
    /// Content height from last frame.
    pub content_height: f32,
    /// Whether to stick to bottom when content grows.
    pub stick_to_bottom: bool,
    /// Whether the view was near the bottom before content grew.
    pub at_bottom: bool,
}

impl Default for ScrollAreaState {
    fn default() -> Self {
        Self {
            scroll_offset: 0.0,
            content_height: 0.0,
            stick_to_bottom: false,
            at_bottom: true,
        }
    }
}
