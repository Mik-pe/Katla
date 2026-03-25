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

        if self.popup_id.is_none() {
            self.popup_bounds = None;
        }

        self.popup_opened_this_frame = false;
        self.popup_consume_click = false;

        self.clip_stack.clear();
        self.clip_stack
            .push(katla_math::Rect2D::from_size(screen_size));
    }

    /// End the frame and get the draw list.
    ///
    /// After calling this, render the draw list using `UiRenderer`.
    pub fn end(&mut self) -> &crate::draw_list::DrawList {
        debug_assert!(self.in_frame, "end() called without begin()");

        self.draw_list.finalize();
        self.in_frame = false;

        if self.input.mouse_released[crate::input::mouse_button::LEFT] {
            self.active_id = None;
        }

        &self.draw_list
    }
}
