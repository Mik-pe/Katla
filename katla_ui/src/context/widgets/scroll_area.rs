//! Internal scroll area widget implementation.
//!
//! This module contains the rendering and interaction logic for scrollable containers.
//! This is a private implementation detail.

use katla_math::{Rect2D, Vec2};

use crate::input::mouse_button;

use super::super::UiContext;

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

/// Scroll area options (builder pattern).
pub struct ScrollArea<'a> {
    id: &'a str,
    /// Maximum height for the scroll area.
    max_height: f32,
    /// Whether to show vertical scrollbar.
    show_scrollbar: bool,
    /// Whether to stick to bottom when content grows.
    stick_to_bottom: bool,
}

impl<'a> ScrollArea<'a> {
    /// Create a new scroll area configuration.
    pub fn new(id: &'a str) -> Self {
        Self {
            id,
            max_height: f32::MAX,
            show_scrollbar: true,
            stick_to_bottom: false,
        }
    }

    /// Set maximum height.
    pub fn max_height(mut self, height: f32) -> Self {
        self.max_height = height;
        self
    }

    /// Enable/disable scrollbar.
    pub fn show_scrollbar(mut self, show: bool) -> Self {
        self.show_scrollbar = show;
        self
    }

    /// Keep scrolled to bottom when content grows.
    pub fn stick_to_bottom(mut self, stick: bool) -> Self {
        self.stick_to_bottom = stick;
        self
    }
}

impl UiContext {
    /// Scrollable area with closure-based content.
    ///
    /// The closure should draw the scrollable content and return the total content height.
    ///
    /// # Example
    /// ```ignore
    /// let mut scroll_state = ScrollAreaState::default();
    /// scroll_state = ui.scroll_area(
    ///     ScrollArea::new("my_scroll").max_height(200.0),
    ///     scroll_state,
    ///     bounds,
    ///     |ui| {
    ///         // Draw content at offset positions
    ///         let content_height = 500.0; // Calculate from your content
    ///         content_height
    ///     }
    /// );
    /// ```
    pub fn scroll_area<F>(
        &mut self,
        config: ScrollArea,
        state: ScrollAreaState,
        bounds: Rect2D,
        content: F,
    ) -> ScrollAreaState
    where
        F: FnOnce(&mut Self) -> f32,
    {
        let ScrollArea {
            id,
            max_height,
            show_scrollbar,
            stick_to_bottom,
        } = config;

        // Create mutable state from the passed value
        let mut state = state;
        state.stick_to_bottom = stick_to_bottom;

        // Calculate actual height (limited by max_height)
        let actual_height = bounds.height().min(max_height);

        // Calculate content width (leave room for scrollbar if needed)
        let scrollbar_width = if show_scrollbar {
            self.style.scrollbar_width
        } else {
            0.0
        };
        let content_width = bounds.width() - scrollbar_width;

        // Content area
        let content_bounds =
            Rect2D::from_origin_size(bounds.min, Vec2::new(content_width, actual_height));

        let scrollbar_bounds = Rect2D::from_origin_size(
            Vec2::new(bounds.max.x() - scrollbar_width, bounds.min.y()),
            Vec2::new(scrollbar_width, actual_height),
        );

        let scrollbar_id = {
            self.push_id(id);
            let sid = self.generate_id("\x00scrollbar");
            self.pop_id();
            sid
        };

        let is_active_scrollbar = self.active_id == Some(scrollbar_id);
        let mouse_in_area = self.input.is_hovered(bounds) && self.z_index >= self.hover_z_index;

        // Handle mouse wheel scrolling (works even when dragging scrollbar)
        if mouse_in_area && !self.input.scroll_consumed && !is_active_scrollbar {
            let scroll_delta = self.input.scroll_delta.y() * 30.0;
            if scroll_delta != 0.0 {
                let max_scroll = (state.content_height - actual_height).max(0.0);
                state.scroll_offset = (state.scroll_offset - scroll_delta).clamp(0.0, max_scroll);
                self.input.scroll_consumed = true;
            }
        }

        // Scrollbar dragging
        if is_active_scrollbar {
            if self.input.mouse_down[mouse_button::LEFT] {
                let max_scroll = (state.content_height - actual_height).max(0.0);
                if max_scroll > 0.0 {
                    let track_height = actual_height;
                    let handle_height =
                        (actual_height / state.content_height).clamp(20.0, track_height);
                    let track_usable = (track_height - handle_height).max(1.0);

                    let mouse_y = self.input.mouse_pos.y() - bounds.min.y() - handle_height * 0.5;
                    state.scroll_offset =
                        (mouse_y / track_usable * max_scroll).clamp(0.0, max_scroll);
                }
            } else {
                self.active_id = None;
            }
        } else if mouse_in_area
            && self.input.is_hovered(scrollbar_bounds)
            && self.input.mouse_pressed[mouse_button::LEFT]
        {
            self.active_id = Some(scrollbar_id);
        }

        // Push clip for content
        self.push_clip(content_bounds);

        // Store for helper methods (scroll_offset, scroll_to_y)
        self.scroll_area_bounds = Some(bounds);
        self.scroll_area_content_bounds = Some(content_bounds);
        self.scroll_area_state = Some(state);
        self.scroll_area_show_scrollbar = show_scrollbar;

        // Run content closure
        let content_height = content(self);

        // Get the state back (it was modified by the closure via scroll_offset, scroll_to_y)
        let mut state = self
            .scroll_area_state
            .expect("scroll area state must be set before use");

        // Update content height
        let prev_content_height = state.content_height;
        state.content_height = content_height;

        // Handle stick_to_bottom
        if state.stick_to_bottom && content_height > prev_content_height && state.at_bottom {
            let max_scroll = (content_height - bounds.height()).max(0.0);
            state.scroll_offset = max_scroll;
        }

        // Clamp scroll offset
        let max_scroll = (content_height - bounds.height()).max(0.0);
        state.scroll_offset = state.scroll_offset.clamp(0.0, max_scroll);

        if state.stick_to_bottom {
            state.at_bottom = max_scroll == 0.0 || state.scroll_offset >= max_scroll - 20.0;
        }

        // Pop content clip
        self.pop_clip();

        // Draw scrollbar if needed
        if show_scrollbar && content_height > actual_height {
            let scrollbar_width = self.style.scrollbar_width;
            let track_height = actual_height;
            let handle_height =
                (actual_height / content_height * track_height).clamp(20.0, track_height);
            let track_usable = (track_height - handle_height).max(1.0);
            let max_scroll = (content_height - actual_height).max(0.0);

            let handle_y = if max_scroll > 0.0 {
                bounds.min.y() + (state.scroll_offset / max_scroll) * track_usable
            } else {
                bounds.min.y()
            };

            let scrollbar_bounds = Rect2D::from_origin_size(
                Vec2::new(bounds.max.x() - scrollbar_width, bounds.min.y()),
                Vec2::new(scrollbar_width, track_height),
            );

            let handle_bounds = Rect2D::from_origin_size(
                Vec2::new(bounds.max.x() - scrollbar_width, handle_y),
                Vec2::new(scrollbar_width, handle_height),
            );

            // Track
            self.draw_rect(scrollbar_bounds, self.style.scrollbar_track);

            // Handle (rounded, with hover highlight)
            let handle_color =
                if self.active_id == Some(scrollbar_id) || self.input.is_hovered(handle_bounds) {
                    self.style.scrollbar_handle_hovered
                } else {
                    self.style.scrollbar_handle
                };
            let thumb_rounding = self.style.button_rounding.min(scrollbar_width * 0.5);
            self.draw_rounded_rect(handle_bounds, handle_color, thumb_rounding);
        }

        // Clear stored state
        self.scroll_area_bounds = None;
        self.scroll_area_content_bounds = None;
        self.scroll_area_state = None;

        // Return the updated state
        state
    }

    /// Get current scroll offset for a scroll area.
    pub fn scroll_offset(&self) -> f32 {
        self.scroll_area_state
            .as_ref()
            .map(|s| s.scroll_offset)
            .unwrap_or(0.0)
    }

    /// Scroll to a specific Y position within content.
    pub fn scroll_to_y(&mut self, y: f32) {
        if let Some(state) = self.scroll_area_state.as_mut() {
            let content_bounds = self
                .scroll_area_content_bounds
                .unwrap_or_else(|| Rect2D::from_size(Vec2::new(0.0, 0.0)));
            let visible_height = content_bounds.height();

            // Scroll so y is visible
            if y < state.scroll_offset {
                state.scroll_offset = y;
            } else if y > state.scroll_offset + visible_height {
                state.scroll_offset = y - visible_height;
            }
        }
    }
}
