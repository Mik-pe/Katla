//! Internal utility widget implementations.
//!
//! This module contains rendering logic for utility widgets like tooltips.
//! These are private implementation details.

use crate::types::TextureId;
use katla_math::{Color, Rect2D, Vec2};

use super::super::UiContext;

impl UiContext {
    /// Draw a tooltip at the current mouse position.
    pub fn tooltip(&mut self, text: &str) {
        let padding = 4.0;
        let text_size = self.measure_text(text, self.style.font_size);
        let tip_size = Vec2::new(text_size.x() + padding * 2.0, text_size.y() + padding * 2.0);

        let mut tip_pos = self.input.mouse_pos + Vec2::new(10.0, 10.0);

        if tip_pos.x() + tip_size.x() > self.screen_size.x() {
            tip_pos = Vec2::new(tip_pos.x() - tip_size.x() - 20.0, tip_pos.y());
        }
        if tip_pos.y() + tip_size.y() > self.screen_size.y() {
            tip_pos = Vec2::new(tip_pos.x(), tip_pos.y() - tip_size.y() - 20.0);
        }

        let bounds = Rect2D::from_origin_size(tip_pos, tip_size);

        self.draw_rect(bounds, self.style.window_bg);
        self.draw_rect_border(bounds, Color::TRANSPARENT, self.style.border, 1.0);
        self.draw_text(
            text,
            Vec2::new(tip_pos.x() + padding, tip_pos.y() + padding),
            self.style.text_color,
            self.style.font_size,
        );
    }

    /// Draw an image.
    pub fn image(
        &mut self,
        texture: TextureId,
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

    pub(crate) fn image_bordered(
        &mut self,
        texture: TextureId,
        bounds: Rect2D,
        uv: Option<Rect2D>,
        tint: Option<Color>,
        border_color: Color,
    ) {
        self.image(texture, bounds, uv, tint);
        self.draw_rect_border(bounds, Color::TRANSPARENT, border_color, 1.0);
    }
}

/// Image widget for displaying textures.
pub struct Image {
    texture: TextureId,
    bounds: Rect2D,
    uv: Option<Rect2D>,
    tint: Option<Color>,
}

impl Image {
    pub fn new(texture: TextureId) -> Self {
        Self {
            texture,
            bounds: Rect2D::default(),
            uv: None,
            tint: None,
        }
    }

    pub fn bounds(mut self, bounds: Rect2D) -> Self {
        self.bounds = bounds;
        self
    }

    pub fn uv(mut self, uv: Rect2D) -> Self {
        self.uv = Some(uv);
        self
    }

    pub fn tint(mut self, color: Color) -> Self {
        self.tint = Some(color);
        self
    }
}
