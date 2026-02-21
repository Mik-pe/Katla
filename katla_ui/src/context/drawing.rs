//! Drawing primitives for UI rendering.
//!
//! Low-level drawing functions for rectangles, text, images, lines, and icons.

use katla_math::{Color, Rect2D, Vec2};

use crate::draw_list::TextureId;
use crate::text::{FontId, SubpixelBin};
use crate::FontSize;

use super::UiContext;

impl UiContext {
    // -------------------------------------------------------------------------
    // Low-level Primitives
    // -------------------------------------------------------------------------

    /// Draw a solid-color rectangle.
    pub fn draw_rect(&mut self, bounds: Rect2D, color: Color) {
        self.draw_list.set_clip(self.clip_rect());
        self.draw_list.add_rect(bounds, color);
    }

    /// Draw a rectangle with a border.
    pub fn draw_rect_border(
        &mut self,
        bounds: Rect2D,
        fill: Color,
        border: Color,
        border_width: f32,
    ) {
        self.draw_rect(bounds, fill);
        self.draw_selection_border(bounds, border, border_width);
    }

    /// Draw only a selection border (no fill).
    ///
    /// Useful for highlighting already-drawn content like selected items.
    /// Internally draws 4 rectangles, but these are batched into a single
    /// draw command by the draw list (same texture/color/z-index/clip).
    pub fn draw_selection_border(&mut self, bounds: Rect2D, color: Color, width: f32) {
        // Top
        self.draw_rect(
            Rect2D::from_origin_size(bounds.min, Vec2::new(bounds.width(), width)),
            color,
        );
        // Bottom
        self.draw_rect(
            Rect2D::from_origin_size(
                Vec2::new(bounds.min.x(), bounds.max.y() - width),
                Vec2::new(bounds.width(), width),
            ),
            color,
        );
        // Left
        self.draw_rect(
            Rect2D::from_origin_size(bounds.min, Vec2::new(width, bounds.height())),
            color,
        );
        // Right
        self.draw_rect(
            Rect2D::from_origin_size(
                Vec2::new(bounds.max.x() - width, bounds.min.y()),
                Vec2::new(width, bounds.height()),
            ),
            color,
        );
    }

    /// Draw a textured image with explicit texture.
    ///
    /// # Arguments
    /// * `bounds` - Screen position and size
    /// * `uv_min` - Top-left UV coordinate
    /// * `uv_max` - Bottom-right UV coordinate
    /// * `color` - Tint color (use Color::WHITE for no tint)
    /// * `texture` - Texture to sample from (TextureId::FONT_ATLAS, VIEWPORT, or custom)
    pub fn draw_image(&mut self, bounds: Rect2D, uv_min: Vec2, uv_max: Vec2, color: Color, texture: TextureId) {
        self.draw_list.set_clip(self.clip_rect());
        self.draw_list.add_image(bounds, uv_min, uv_max, color, texture);
    }

    /// Draw a line.
    pub fn draw_line(&mut self, start: Vec2, end: Vec2, color: Color, thickness: f32) {
        self.draw_list.set_clip(self.clip_rect());
        self.draw_list.add_line(start, end, color, thickness);
    }

    /// Draw text using the font system.
    ///
    /// Text is rendered as textured quads from the font atlas.
    /// If no font is loaded, draws placeholder rectangles.
    ///
    /// `position` is the TOP-LEFT of the text bounding box.
    /// This is the most intuitive API for UI work.
    ///
    /// Supports multiline text with `\n` characters.
    pub fn draw_text(&mut self, text: &str, position: Vec2, color: Color, size: f32) {
        // Calculate subpixel bin from TEXT START position.
        // All characters share the same bin so the text moves as a unit.
        let (floor_x, subpixel_bin) = SubpixelBin::new(position.x());
        let start_x = floor_x as f32;

        // Get line height for multiline support
        let line_height = self.line_height(size);

        // Round Y position to integer for crisp vertical alignment
        let mut baseline_y = (position.y() + self.font_ascent(size)).round();

        // Cursor tracks offset relative to start_x
        let mut cursor_offset = 0.0f32;

        for c in text.chars() {
            // Handle newlines - move to next line
            if c == '\n' {
                cursor_offset = 0.0;
                baseline_y += line_height;
                continue;
            }

            // Get glyph with the shared subpixel bin
            if let Some(glyph) = self.fonts.get_or_rasterize(
                self.current_font,
                c,
                size,
                self.scale_factor,
                subpixel_bin,
            ) {
                // Skip empty glyphs (spaces)
                if glyph.size.x() == 0.0 || glyph.size.y() == 0.0 {
                    cursor_offset += glyph.advance;
                    continue;
                }

                // Position is RELATIVE to the integer start position
                let pos_x = start_x + cursor_offset + glyph.offset_x;
                let pos_y = baseline_y - glyph.top_offset;

                let bounds = Rect2D::from_origin_size(Vec2::new(pos_x, pos_y), glyph.size);

                // Draw glyph as textured quad
                self.draw_list.set_clip(self.clip_rect());
                self.draw_list.add_textured_rect(
                    bounds,
                    glyph.uv_rect,
                    color,
                    TextureId::FONT_ATLAS,
                );

                cursor_offset += glyph.advance;
            } else {
                // No glyph available - draw placeholder
                let placeholder_size = Vec2::new(size * 0.6, size);
                let bounds = Rect2D::from_origin_size(
                    Vec2::new(start_x + cursor_offset, baseline_y - self.font_ascent(size)),
                    placeholder_size,
                );
                self.draw_rect_border(bounds, Color::TRANSPARENT, color, 1.0);
                cursor_offset += placeholder_size.x();
            }
        }
    }

    /// Measure text dimensions in logical pixels.
    pub fn measure_text(&self, text: &str, size: f32) -> Vec2 {
        self.fonts
            .measure_text(self.current_font, text, size, self.scale_factor)
    }

    /// Measure text dimensions using a predefined font size.
    pub fn measure_text_sized(&self, text: &str, size: FontSize) -> Vec2 {
        self.measure_text(text, size.to_pixels())
    }

    /// Get the font ascent (baseline to font top) in logical pixels.
    ///
    /// This is needed for proper text positioning.
    pub fn font_ascent(&self, size: f32) -> f32 {
        self.fonts
            .get_font_metrics(self.current_font, size, self.scale_factor)
            .map(|(ascent, _, _)| ascent)
            .unwrap_or(size * 0.75) // Fallback heuristic
    }

    /// Get the font ascent using a predefined font size.
    pub fn font_ascent_sized(&self, size: FontSize) -> f32 {
        self.font_ascent(size.to_pixels())
    }

    /// Get the line height for a font size (ascent - descent + small gap).
    ///
    /// This is used for multiline text spacing.
    pub fn line_height(&self, size: f32) -> f32 {
        self.fonts
            .get_font_metrics(self.current_font, size, self.scale_factor)
            .map(|(ascent, descent, line_gap)| ascent - descent + line_gap)
            .unwrap_or(size * 1.2) // Fallback heuristic
    }

    /// Draw text using a predefined font size.
    pub fn draw_text_sized(
        &mut self,
        text: &str,
        position: Vec2,
        color: Color,
        size: FontSize,
    ) {
        self.draw_text(text, position, color, size.to_pixels())
    }

    /// Draw an icon from an icon font (like ForkAwesome).
    ///
    /// This is a convenience method that temporarily switches to the icon font,
    /// renders the icon, and restores the previous font.
    ///
    /// # Arguments
    /// * `icon` - The icon character (use constants from `icons::ForkAwesome`)
    /// * `position` - Top-left position of the icon
    /// * `size` - Font size in pixels
    /// * `color` - RGBA color
    ///
    /// # Example
    /// ```ignore
    /// use katla_ui::{FontId, icons::ForkAwesome};
    ///
    /// ui.draw_icon(ForkAwesome::CUBE, pos, 16.0, [1.0, 1.0, 1.0, 1.0]);
    /// ```
    pub fn draw_icon(&mut self, icon: char, position: Vec2, size: f32, color: Color) {
        let prev_font = self.current_font;
        self.current_font = FontId::ICON;

        let mut buf = [0u8; 4];
        let icon_str = icon.encode_utf8(&mut buf);
        self.draw_text(icon_str, position, color, size);

        self.current_font = prev_font;
    }

    /// Draw an icon aligned with adjacent text.
    ///
    /// This method uses the reference font's ascent for baseline positioning,
    /// ensuring icons align properly with text rendered in that font.
    /// Use this when drawing icons alongside regular text.
    ///
    /// # Arguments
    /// * `icon` - The icon character (use constants from `icons::ForkAwesome`)
    /// * `position` - Top-left position (same as you'd use for adjacent text)
    /// * `size` - Font size in pixels
    /// * `color` - RGBA color
    /// * `ref_font` - Reference font to use for baseline alignment (usually FontId::DEFAULT)
    pub fn draw_icon_aligned(
        &mut self,
        icon: char,
        position: Vec2,
        size: f32,
        color: Color,
        ref_font: FontId,
    ) {
        // Get text font metrics
        let text_ascent = self
            .fonts
            .get_font_metrics(ref_font, size, self.scale_factor)
            .map(|(a, _, _)| a)
            .unwrap_or(size * 0.75);

        // Get icon's actual rendered size
        let icon_glyph = self.fonts.get_or_rasterize(
            FontId::ICON,
            icon,
            size,
            self.scale_factor,
            SubpixelBin::Zero,
        );

        if let Some(glyph) = icon_glyph {
            if glyph.size.x() > 0.0 && glyph.size.y() > 0.0 {
                // Text centerline: position.y + text_ascent/2
                // Icon center should match: icon_top + icon_height/2 = text_center
                let text_center_y = position.y() + text_ascent * 0.5;
                let icon_top_y = text_center_y - glyph.size.y() * 0.5;

                let glyph_pos =
                    Vec2::new((position.x() + glyph.offset_x).round(), icon_top_y.round());
                let bounds = katla_math::Rect2D::from_origin_size(glyph_pos, glyph.size);
                self.draw_list.set_clip(self.clip_rect());
                self.draw_list.add_textured_rect(
                    bounds,
                    glyph.uv_rect,
                    color,
                    TextureId::FONT_ATLAS,
                );
            }
        }
    }

    /// Draw an icon centered within bounds.
    ///
    /// This is useful for icon buttons where you want the icon centered.
    pub fn draw_icon_centered(&mut self, icon: char, bounds: Rect2D, size: f32, color: Color) {
        // Get icon metrics to center it
        let prev_font = self.current_font;
        self.current_font = FontId::ICON;

        let mut buf = [0u8; 4];
        let icon_str = icon.encode_utf8(&mut buf);
        let icon_size = self.measure_text(icon_str, size);

        let pos = Vec2::new(
            bounds.center().x() - icon_size.x() * 0.5,
            bounds.center().y() - icon_size.y() * 0.5,
        );

        self.draw_text(icon_str, pos, color, size);
        self.current_font = prev_font;
    }

    /// Measure an icon's dimensions.
    pub fn measure_icon(&mut self, icon: char, size: f32) -> Vec2 {
        let prev_font = self.current_font;
        self.current_font = FontId::ICON;

        let mut buf = [0u8; 4];
        let icon_str = icon.encode_utf8(&mut buf);
        let dims = self.measure_text(icon_str, size);

        self.current_font = prev_font;
        dims
    }

    /// Set the current font for text rendering.
    pub fn set_font(&mut self, font_id: FontId) {
        self.current_font = font_id;
    }

    /// Get the current font ID.
    pub fn current_font(&self) -> FontId {
        self.current_font
    }

    // -------------------------------------------------------------------------
    // Utility Drawing Methods
    // -------------------------------------------------------------------------

    /// Draw an icon followed by text at the specified position.
    ///
    /// Returns the x position after the text (for chaining).
    pub fn draw_icon_label(&mut self, icon: char, text: &str, position: Vec2, icon_size: f32, text_size: f32, color: Color) -> f32 {
        let icon_y = position.y();
        self.draw_icon(icon, position, icon_size, color);
        let text_x = position.x() + icon_size + 4.0;
        self.draw_text(text, Vec2::new(text_x, icon_y), color, text_size);
        text_x + self.measure_text(text, text_size).x()
    }

    /// Draw an icon with text centered horizontally within bounds.
    ///
    /// Returns the y position after the content (for chaining vertically).
    pub fn draw_icon_text_centered(&mut self, icon: char, text: &str, bounds: Rect2D, icon_size: f32, font_size: f32, color: Color) -> f32 {
        let text_measure = self.measure_text(text, font_size);
        let total_width = icon_size + 4.0 + text_measure.x();
        let start_x = bounds.center().x() - total_width * 0.5;
        let text_y = bounds.center().y() - text_measure.y() * 0.5;

        self.draw_icon(icon, Vec2::new(start_x, text_y), icon_size, color);
        self.draw_text(text, Vec2::new(start_x + icon_size + 4.0, text_y), color, font_size);

        text_y + text_measure.y()
    }
}
