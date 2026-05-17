//! Drawing primitives for UI rendering.
//!
//! Low-level drawing functions for rectangles, text, images, lines, and icons.

use crate::types::TextureId;
use katla_math::{Color, Rect2D, Vec2};

use crate::text::{FontId, SubpixelBin};

use super::UiContext;
use super::z_index;

/// Compute the top-left position for centering content of a given size within bounds.
#[inline]
pub(crate) fn center_in_bounds(bounds: Rect2D, content_size: Vec2) -> Vec2 {
    Vec2::new(
        bounds.center().x() - content_size.x() * 0.5,
        bounds.center().y() - content_size.y() * 0.5,
    )
}

impl UiContext {
    // -------------------------------------------------------------------------
    // Low-level Primitives
    // -------------------------------------------------------------------------

    /// Draw a solid-color rectangle.
    pub fn draw_rect(&mut self, bounds: Rect2D, color: Color) {
        self.draw_list.set_clip(self.clip_rect());
        self.draw_list.add_rect(bounds, color);

        if self.z_index > z_index::DEFAULT {
            self.register_hover_layer(self.z_index, bounds);
        }
    }

    pub fn draw_rounded_rect(&mut self, bounds: Rect2D, color: Color, radius: f32) {
        self.draw_list.set_clip(self.clip_rect());
        self.draw_list.add_rounded_rect_aa(bounds, color, radius);

        if self.z_index > z_index::DEFAULT {
            self.register_hover_layer(self.z_index, bounds);
        }
    }

    /// Draw a gradient rectangle with per-corner colors (TL, TR, BR, BL).
    pub fn draw_gradient_rect(
        &mut self,
        bounds: Rect2D,
        tl: Color,
        tr: Color,
        br: Color,
        bl: Color,
    ) {
        self.draw_list.set_clip(self.clip_rect());
        self.draw_list.add_gradient_rect(bounds, tl, tr, br, bl);

        if self.z_index > z_index::DEFAULT {
            self.register_hover_layer(self.z_index, bounds);
        }
    }

    /// Draw a filled circle.
    pub fn draw_circle(&mut self, center: Vec2, radius: f32, color: Color) {
        self.draw_list.set_clip(self.clip_rect());
        self.draw_list.add_circle_auto(center, radius, color);

        if self.z_index > z_index::DEFAULT {
            let bounds = Rect2D::from_center_size(center, Vec2::new(radius * 2.0, radius * 2.0));
            self.register_hover_layer(self.z_index, bounds);
        }
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

    /// Draw only a rounded selection border (no fill).
    ///
    /// Draws a stroke along the rounded rect path instead of 4 sharp rectangles.
    pub fn draw_rounded_selection_border(
        &mut self,
        bounds: Rect2D,
        color: Color,
        width: f32,
        radius: f32,
    ) {
        self.draw_list.set_clip(self.clip_rect());
        self.draw_list
            .add_rounded_rect_stroke_aa(bounds, color, radius, width);
    }

    /// Draw a textured image with explicit texture ID.
    ///
    /// # Arguments
    /// * `bounds` - Screen position and size
    /// * `uv_min` - Top-left UV coordinate
    /// * `uv_max` - Bottom-right UV coordinate
    /// * `color` - Tint color (use Color::WHITE for no tint)
    /// * `texture` - Texture ID (mapped to handle by katla_app)
    pub fn draw_image(
        &mut self,
        bounds: Rect2D,
        uv_min: Vec2,
        uv_max: Vec2,
        color: Color,
        texture: TextureId,
    ) {
        self.draw_list.set_clip(self.clip_rect());
        self.draw_list
            .add_image(bounds, uv_min, uv_max, color, texture);
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
        let font_atlas = self.fonts.borrow().atlas_id();

        let (floor_x, subpixel_bin) = SubpixelBin::new(position.x());
        let start_x = floor_x as f32;

        let line_height = self.line_height(size);
        let font_ascent = self.font_ascent(size);
        let baseline_y_start = (position.y() + font_ascent).round();

        struct GlyphEntry {
            bounds: Rect2D,
            uv_rect: Rect2D,
            is_placeholder: bool,
        }

        let mut glyphs = Vec::new();
        let mut cursor_offset = 0.0f32;
        let mut current_baseline = baseline_y_start;

        {
            let mut fonts = self.fonts.borrow_mut();
            for c in text.chars() {
                if c == '\n' {
                    cursor_offset = 0.0;
                    current_baseline += line_height;
                    continue;
                }

                let glyph = fonts.get_or_rasterize(
                    self.current_font,
                    c,
                    size,
                    self.scale_factor,
                    subpixel_bin,
                );
                if let Some(glyph) = glyph {
                    if glyph.size.x() == 0.0 || glyph.size.y() == 0.0 {
                        cursor_offset += glyph.advance;
                        continue;
                    }

                    let pos_x = start_x + cursor_offset + glyph.offset_x;
                    let pos_y = current_baseline - glyph.top_offset;

                    glyphs.push(GlyphEntry {
                        bounds: Rect2D::from_origin_size(Vec2::new(pos_x, pos_y), glyph.size),
                        uv_rect: glyph.uv_rect,
                        is_placeholder: false,
                    });
                    cursor_offset += glyph.advance;
                } else {
                    let placeholder_size = Vec2::new(size * 0.6, size);
                    glyphs.push(GlyphEntry {
                        bounds: Rect2D::from_origin_size(
                            Vec2::new(start_x + cursor_offset, current_baseline - font_ascent),
                            placeholder_size,
                        ),
                        uv_rect: Rect2D::default(),
                        is_placeholder: true,
                    });
                    cursor_offset += placeholder_size.x();
                }
            }
        }

        self.draw_list.set_clip(self.clip_rect());
        for entry in &glyphs {
            if entry.is_placeholder {
                self.draw_rect_border(entry.bounds, Color::TRANSPARENT, color, 1.0);
            } else {
                self.draw_list
                    .add_textured_rect(entry.bounds, entry.uv_rect, color, font_atlas);
            }
        }
    }

    /// Measure text dimensions in logical pixels.
    #[inline]
    pub fn measure_text(&self, text: &str, size: f32) -> Vec2 {
        self.fonts
            .borrow()
            .measure_text(self.current_font, text, size, self.scale_factor)
    }

    /// Get the font ascent (baseline to font top) in logical pixels.
    ///
    /// This is needed for proper text positioning.
    #[inline]
    pub(crate) fn font_ascent(&self, size: f32) -> f32 {
        self.fonts
            .borrow()
            .get_font_metrics(self.current_font, size, self.scale_factor)
            .map(|(ascent, _, _)| ascent)
            .unwrap_or(size * 0.75) // Fallback heuristic
    }

    /// Get the line height for a font size (ascent - descent + small gap).
    ///
    /// This is used for multiline text spacing.
    #[inline]
    pub(crate) fn line_height(&self, size: f32) -> f32 {
        self.fonts
            .borrow()
            .get_font_metrics(self.current_font, size, self.scale_factor)
            .map(|(ascent, descent, line_gap)| ascent - descent + line_gap)
            .unwrap_or(size * 1.2) // Fallback heuristic
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
        // Get the font atlas texture handle
        let font_atlas = self.fonts.borrow().atlas_id();

        // Get text font metrics
        let text_ascent = self
            .fonts
            .borrow()
            .get_font_metrics(ref_font, size, self.scale_factor)
            .map(|(a, _, _)| a)
            .unwrap_or(size * 0.75);

        // Get icon's actual rendered size
        let icon_glyph = self.fonts.borrow_mut().get_or_rasterize(
            FontId::ICON,
            icon,
            size,
            self.scale_factor,
            SubpixelBin::Zero,
        );

        if let Some(glyph) = icon_glyph
            && glyph.size.x() > 0.0
            && glyph.size.y() > 0.0
        {
            let line_bounds =
                katla_math::Rect2D::from_origin_size(position, Vec2::new(0.0, text_ascent));
            let centered = center_in_bounds(line_bounds, glyph.size);
            let glyph_pos = Vec2::new(
                (position.x() + glyph.offset_x).round(),
                centered.y().round(),
            );
            let bounds = katla_math::Rect2D::from_origin_size(glyph_pos, glyph.size);
            self.draw_list.set_clip(self.clip_rect());
            self.draw_list
                .add_textured_rect(bounds, glyph.uv_rect, color, font_atlas);
        }
    }

    /// Draw an icon centered within bounds.
    ///
    /// This is useful for icon buttons where you want the icon centered.
    pub fn draw_icon_centered(&mut self, icon: char, bounds: Rect2D, size: f32, color: Color) {
        let prev_font = self.current_font;
        self.current_font = FontId::ICON;

        let font_atlas = self.fonts.borrow().atlas_id();
        let (_, subpixel_bin) = SubpixelBin::new(bounds.center().x());

        let mut buf = [0u8; 4];
        let icon_str = icon.encode_utf8(&mut buf);

        let glyph = self.fonts.borrow_mut().get_or_rasterize(
            FontId::ICON,
            icon,
            size,
            self.scale_factor,
            subpixel_bin,
        );
        if let Some(glyph) = glyph {
            if glyph.size.x() > 0.0 && glyph.size.y() > 0.0 {
                let draw_pos = center_in_bounds(bounds, glyph.size);

                let glyph_bounds = Rect2D::from_origin_size(draw_pos, glyph.size);
                self.draw_list.set_clip(self.clip_rect());
                self.draw_list
                    .add_textured_rect(glyph_bounds, glyph.uv_rect, color, font_atlas);
            }
        } else {
            self.draw_text(
                icon_str,
                Vec2::new(bounds.min.x(), bounds.min.y()),
                color,
                size,
            );
        }

        self.current_font = prev_font;
    }

    pub fn set_font(&mut self, font_id: FontId) {
        self.current_font = font_id;
    }
}
