//! Utility widgets: progress bar, tooltip, color rect, image.

use katla_math::{Color, Rect2D, Vec2};

use super::super::UiContext;

impl UiContext {
    /// Draw a progress bar.
    pub fn progress_bar(&mut self, progress: f32, bounds: Rect2D, overlay: Option<&str>) {
        let progress_clamped = progress.clamp(0.0, 1.0);

        // Background
        self.draw_rect(bounds, self.style.slider_track);

        // Fill
        if progress_clamped > 0.0 {
            let fill_width = bounds.width() * progress_clamped;
            let fill_bounds =
                Rect2D::from_origin_size(bounds.min, Vec2::new(fill_width, bounds.height()));
            self.draw_rect(fill_bounds, self.style.slider_grab);
        }

        // Overlay text
        if let Some(text) = overlay {
            let text_size = self.measure_text(text, self.style.font_size);
            let text_pos = Vec2::new(
                bounds.center().x() - text_size.x() * 0.5,
                bounds.center().y() - text_size.y() * 0.5,
            );
            self.draw_text(text, text_pos, self.style.text_color, self.style.font_size);
        }
    }

    /// Draw a tooltip at the current mouse position.
    pub fn tooltip(&mut self, text: &str) {
        let padding = 4.0;
        let text_size = self.measure_text(text, self.style.font_size);
        let tip_size = Vec2::new(text_size.x() + padding * 2.0, text_size.y() + padding * 2.0);

        // Position near mouse
        let mut tip_pos = self.input.mouse_pos + Vec2::new(10.0, 10.0);

        // Keep on screen
        if tip_pos.x() + tip_size.x() > self.screen_size.x() {
            tip_pos = Vec2::new(tip_pos.x() - tip_size.x() - 20.0, tip_pos.y());
        }
        if tip_pos.y() + tip_size.y() > self.screen_size.y() {
            tip_pos = Vec2::new(tip_pos.x(), tip_pos.y() - tip_size.y() - 20.0);
        }

        let bounds = Rect2D::from_origin_size(tip_pos, tip_size);

        // Draw tooltip
        self.draw_rect(bounds, self.style.window_bg);
        self.draw_rect_border(bounds, Color::TRANSPARENT, self.style.border, 1.0);
        self.draw_text(
            text,
            Vec2::new(tip_pos.x() + padding, tip_pos.y() + padding),
            self.style.text_color,
            self.style.font_size,
        );
    }

    /// Draw a color preview rectangle.
    pub fn color_rect(&mut self, color: Color, bounds: Rect2D) {
        self.draw_rect(bounds, color);
        self.draw_rect_border(bounds, Color::TRANSPARENT, self.style.border, 1.0);
    }

    /// Draw an image/texture in the given bounds.
    ///
    /// The texture is stretched to fill the bounds.
    /// Use UV rect to display a portion of the texture.
    pub fn image(
        &mut self,
        texture: crate::TextureId,
        bounds: Rect2D,
        uv: Option<Rect2D>,
        tint: Option<Color>,
    ) {
        let uv_rect = uv.unwrap_or(Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0)));
        let color = tint.unwrap_or(Color::WHITE);
        self.draw_list.set_clip(self.clip_rect());
        self.draw_list
            .add_textured_rect(bounds, uv_rect, color, texture);
    }

    /// Draw an image with a border (useful for viewport frames).
    pub fn image_bordered(
        &mut self,
        texture: crate::TextureId,
        bounds: Rect2D,
        uv: Option<Rect2D>,
        tint: Option<Color>,
        border_color: Color,
    ) {
        self.image(texture, bounds, uv, tint);
        self.draw_rect_border(bounds, Color::TRANSPARENT, border_color, 1.0);
    }
}
