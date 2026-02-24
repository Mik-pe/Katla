//! ScrollArea widget for scrollable content.

use katla_math::{Rect2D, Vec2};

use crate::input::mouse_button;

use super::super::UiContext;

/// Scroll area state tracked between frames.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScrollAreaState {
    /// Current scroll offset.
    pub scroll_offset: f32,
    /// Content height from last frame.
    pub content_height: f32,
    /// Whether to stick to bottom when content grows.
    pub stick_to_bottom: bool,
}

/// Scroll area builder.
pub struct ScrollArea<'a> {
    id: &'a str,
    state: &'a mut ScrollAreaState,
    /// Maximum height for the scroll area.
    max_height: f32,
    /// Whether to show vertical scrollbar.
    show_scrollbar: bool,
}

impl<'a> ScrollArea<'a> {
    /// Create a new scroll area.
    pub fn new(id: &'a str, state: &'a mut ScrollAreaState) -> Self {
        Self {
            id,
            state,
            max_height: f32::MAX,
            show_scrollbar: true,
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
    pub fn stick_to_bottom(self, stick: bool) -> Self {
        self.state.stick_to_bottom = stick;
        self
    }
}

impl UiContext {
    /// Begin a scrollable area.
    ///
    /// Returns the content bounds to draw content into.
    /// Call `end_scroll_area()` after drawing content.
    ///
    /// # Example
    /// ```ignore
    /// let mut scroll_state = ScrollAreaState::default();
    /// let content_bounds = ui.begin_scroll_area(
    ///     ScrollArea::new("my_scroll", &mut scroll_state).max_height(200.0),
    ///     bounds
    /// );
    /// // Draw content at content_bounds.min + scroll offset
    /// ui.end_scroll_area();
    /// ```
    pub fn begin_scroll_area(&mut self, config: ScrollArea, bounds: Rect2D) -> Rect2D {
        let ScrollArea {
            id,
            state,
            max_height,
            show_scrollbar,
        } = config;

        // Calculate actual height (limited by max_height)
        let actual_height = bounds.height().min(max_height);

        // Calculate content width (leave room for scrollbar if needed)
        let scrollbar_width = if show_scrollbar { 10.0 } else { 0.0 };
        let content_width = bounds.width() - scrollbar_width;

        // Content area
        let content_bounds =
            Rect2D::from_origin_size(bounds.min, Vec2::new(content_width, actual_height));

        // Handle mouse wheel scrolling
        if self.is_hovered(content_bounds) {
            let scroll_delta = self.input.scroll_delta.y() * 30.0;
            if scroll_delta != 0.0 {
                let max_scroll = (state.content_height - actual_height).max(0.0);
                state.scroll_offset = (state.scroll_offset - scroll_delta).clamp(0.0, max_scroll);
            }
        }

        // Scrollbar dragging
        let scrollbar_bounds = Rect2D::from_origin_size(
            Vec2::new(bounds.max.x() - scrollbar_width, bounds.min.y()),
            Vec2::new(scrollbar_width, actual_height),
        );

        let scrollbar_id = self.generate_id(&format!("{}_scrollbar", id));
        if self.active_id == Some(scrollbar_id) {
            if self.input.mouse_down[mouse_button::LEFT] {
                let max_scroll = (state.content_height - actual_height).max(0.0);
                let track_height = actual_height;
                let handle_height =
                    (actual_height / state.content_height).clamp(20.0, track_height);
                let track_usable = track_height - handle_height;

                let mouse_y = self.input.mouse_pos.y() - bounds.min.y() - handle_height * 0.5;
                state.scroll_offset = (mouse_y / track_usable * max_scroll).clamp(0.0, max_scroll);
            } else {
                self.active_id = None;
            }
        } else if self.is_hovered(scrollbar_bounds) && self.input.mouse_pressed[mouse_button::LEFT]
        {
            self.active_id = Some(scrollbar_id);
        }

        // Push clip for content
        self.push_clip(content_bounds);

        // Store for end_scroll_area
        self.scroll_area_bounds = Some(bounds);
        self.scroll_area_content_bounds = Some(content_bounds);
        self.scroll_area_state = Some(ScrollAreaState { ..*state });
        self.scroll_area_show_scrollbar = show_scrollbar;

        // Return content bounds with scroll offset applied (caller draws at offset)
        content_bounds
    }

    /// End a scrollable area.
    ///
    /// Takes the actual content height and renders scrollbar if needed.
    pub fn end_scroll_area(&mut self, content_height: f32) {
        let bounds = self.scroll_area_bounds.unwrap();
        let _content_bounds = self.scroll_area_content_bounds.unwrap();
        let show_scrollbar = self.scroll_area_show_scrollbar;
        let mut state = self.scroll_area_state.take().unwrap();

        // Update content height
        let prev_content_height = state.content_height;
        state.content_height = content_height;

        // Handle stick_to_bottom
        if state.stick_to_bottom && content_height > prev_content_height {
            let max_scroll = (content_height - bounds.height()).max(0.0);
            state.scroll_offset = max_scroll;
        }

        // Clamp scroll offset
        let max_scroll = (content_height - bounds.height()).max(0.0);
        state.scroll_offset = state.scroll_offset.clamp(0.0, max_scroll);

        // Pop content clip
        self.pop_clip();

        // Draw scrollbar if needed
        if show_scrollbar && content_height > bounds.height() {
            let scrollbar_width = 10.0;
            let track_height = bounds.height();
            let handle_height =
                (bounds.height() / content_height * track_height).clamp(20.0, track_height);
            let track_usable = track_height - handle_height;
            let max_scroll = (content_height - bounds.height()).max(0.0);

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

            // Handle
            let handle_color = if self.active_id.is_some() || self.is_hovered(handle_bounds) {
                self.style.scrollbar_handle_hovered
            } else {
                self.style.scrollbar_handle
            };
            self.draw_rect(handle_bounds, handle_color);
        }

        // Clear stored state
        self.scroll_area_bounds = None;
        self.scroll_area_content_bounds = None;

        // Copy state back
        if let Some(stored) = self.scroll_area_state.as_mut() {
            *stored = state;
        }
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
