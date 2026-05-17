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
        let padding = 6.0;
        let line_spacing = 2.0;
        let font_size = self.style.font_size;

        let lines: Vec<&str> = text.split('\n').collect();
        let line_count = lines.len();

        let mut max_width = 0.0f32;
        let mut line_widths = Vec::with_capacity(line_count);
        for line in &lines {
            let w = self.measure_text(line, font_size).x();
            max_width = max_width.max(w);
            line_widths.push(w);
        }

        let single_line_height = self.measure_text("Ay", font_size).y();
        let total_text_height = if line_count == 1 {
            single_line_height
        } else {
            line_count as f32 * single_line_height + (line_count - 1) as f32 * line_spacing
        };

        let tip_size = Vec2::new(max_width + padding * 2.0, total_text_height + padding * 2.0);

        let mut tip_pos = self.input.mouse_pos + Vec2::new(10.0, 10.0);

        if tip_pos.x() + tip_size.x() > self.screen_size.x() {
            tip_pos = Vec2::new(tip_pos.x() - tip_size.x() - 20.0, tip_pos.y());
        }
        if tip_pos.y() + tip_size.y() > self.screen_size.y() {
            tip_pos = Vec2::new(tip_pos.x(), tip_pos.y() - tip_size.y() - 20.0);
        }

        let bounds = Rect2D::from_origin_size(tip_pos, tip_size);

        self.draw_rounded_rect(bounds, self.style.popup_bg, 4.0);
        self.draw_rounded_selection_border(bounds, self.style.popup_border, 1.0, 4.0);

        let text_x = tip_pos.x() + padding;
        let mut text_y = tip_pos.y() + padding;
        for line in &lines {
            self.draw_text(
                line,
                Vec2::new(text_x, text_y),
                self.style.text_color,
                font_size,
            );
            text_y += single_line_height + line_spacing;
        }
    }

    /// Draw a status bar label at the current cursor position and advance the cursor.
    pub fn status_label(&mut self, text: &str, color: Color) {
        let text_size = self.measure_text(text, self.style.font_size);
        let pos = self.cursor();
        self.draw_text(text, pos, color, self.style.font_size);
        self.set_cursor(Vec2::new(
            pos.x() + text_size.x() + self.style.item_spacing,
            pos.y(),
        ));
    }

    /// Draw a vertical separator line for status bars at the current cursor position.
    pub fn status_separator(&mut self) {
        let pos = self.cursor();
        let height = self.style.font_size;
        let x = pos.x() + self.style.item_spacing * 0.5;
        self.draw_line(
            Vec2::new(x, pos.y()),
            Vec2::new(x, pos.y() + height),
            self.style.separator,
            1.0,
        );
        self.set_cursor(Vec2::new(pos.x() + self.style.item_spacing, pos.y()));
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
}
