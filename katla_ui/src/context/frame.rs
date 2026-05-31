use katla_math::Vec2;

use super::{UiContext, z_index};

impl UiContext {
    /// Set the current time in seconds (for animations like cursor blink).
    pub fn set_time(&mut self, time: f64) {
        self.time = time;
    }

    /// Set the font scale multiplier for accessibility.
    ///
    /// Use 1.0 for default (100%), 1.25 for 125%, 2.0 for 200%, etc.
    pub fn set_font_scale(&mut self, scale: f32) {
        self.font_scale = scale.clamp(0.5, 3.0);
    }

    /// Convert a FontSize to scaled pixels using current font_scale.
    pub(crate) fn scaled_font_size(&self, size: crate::style::FontSize) -> f32 {
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
        self.prev_hover_z_index = self.hover_z_index;
        self.hover_z_index = z_index::DEFAULT;
        self.id_counter = 0;
        self.hovered_id = None;
        self.cursor = Vec2::new(0.0, 0.0);

        self.clip_stack.clear();
        self.clip_stack
            .push(katla_math::Rect2D::from_size(screen_size));
        self.pending_tooltips.clear();
        self.panel_regions.clear();
        self.input.prev_active_id = self.active_id;
        self.focusable_widgets.clear();
        self.declarative_input_consumed = false;
        self.pending_focus_label = None;
    }

    /// End the frame and get the draw list.
    ///
    /// After calling this, render the draw list using `UiRenderer`.
    pub fn end(&mut self) -> &crate::draw_list::DrawList {
        debug_assert!(self.in_frame, "end() called without begin()");

        // Handle Tab/Shift+Tab keyboard navigation
        self.handle_tab_navigation();

        // Draw focus ring on the currently focused widget
        if let Some(focused) = self.focused_id
            && let Some((_, bounds)) = self.focusable_widgets.iter().find(|(id, _)| *id == focused)
        {
            self.draw_list
                .set_clip(katla_math::Rect2D::from_size(self.screen_size));
            self.draw_list
                .add_rect(bounds.inflate(2.0), self.style.focus_ring_color);
        }

        // Render deferred tooltips
        let tooltips = std::mem::take(&mut self.pending_tooltips);
        for (_, text) in tooltips {
            self.tooltip(&text);
        }

        self.draw_list.finalize();
        self.in_frame = false;

        if self.input.mouse_pressed[crate::input::mouse_button::LEFT]
            || self.input.mouse_pressed[crate::input::mouse_button::RIGHT]
            || self.input.mouse_pressed[crate::input::mouse_button::MIDDLE]
        {
            let mouse = self.input.mouse_pos;
            self.focused_panel_id = self
                .panel_regions
                .iter()
                .rev()
                .find(|(_, bounds)| bounds.contains(mouse))
                .map(|(id, _)| *id);
        }

        if self.input.mouse_released[crate::input::mouse_button::LEFT] {
            self.active_id = None;

            // Unfocus text input when clicking outside any focused widget
            if self.focused_id.is_some() && self.hovered_id != self.focused_id {
                self.focused_id = None;
            }
        }

        &self.draw_list
    }

    fn handle_tab_navigation(&mut self) {
        let shift = self.input.is_key_down(crate::input::KeyCode::Shift);
        if !self.input.key_pressed(crate::input::KeyCode::Tab) {
            return;
        }
        if self.focusable_widgets.is_empty() {
            return;
        }

        // If a text input is focused and Ctrl is held, don't tab-navigate
        // (Ctrl+Tab may have different meaning)
        if self.input.is_key_down(crate::input::KeyCode::Control) {
            return;
        }

        let count = self.focusable_widgets.len();
        let current_index = self
            .focused_id
            .and_then(|fid| self.focusable_widgets.iter().position(|(id, _)| *id == fid))
            .unwrap_or(count); // "past the end" = no selection

        let next_index = if shift {
            if current_index == 0 || current_index == count {
                count - 1
            } else {
                current_index - 1
            }
        } else {
            if current_index >= count - 1 {
                0
            } else {
                current_index + 1
            }
        };

        self.focused_id = Some(self.focusable_widgets[next_index].0);
    }
}
