//! Drawing primitives for UI rendering.
//!
//! Low-level drawing functions for rectangles, text, images, lines, and icons.

use crate::types::TextureId;
use katla_math::{Color, Rect2D, Vec2};

use crate::text::FontId;

use super::UiContext;
use super::z_index;

impl UiContext {
    // -------------------------------------------------------------------------
    // Low-level Primitives
    // -------------------------------------------------------------------------

    /// Draw a solid-color rectangle.
    pub(crate) fn draw_rect(&mut self, bounds: Rect2D, color: Color) {
        self.draw_list.set_clip(self.clip_rect());
        self.draw_list.add_rect(bounds, color);

        if self.z_index > z_index::DEFAULT {
            self.register_hover_layer(self.z_index, bounds);
        }
    }

    pub(crate) fn draw_rounded_rect(&mut self, bounds: Rect2D, color: Color, radius: f32) {
        self.draw_list.set_clip(self.clip_rect());
        self.draw_list.add_rounded_rect_aa(bounds, color, radius);

        if self.z_index > z_index::DEFAULT {
            self.register_hover_layer(self.z_index, bounds);
        }
    }

    /// Draw a filled circle.
    pub(crate) fn draw_circle(&mut self, center: Vec2, radius: f32, color: Color) {
        self.draw_list.set_clip(self.clip_rect());
        self.draw_list.add_circle_auto(center, radius, color);

        if self.z_index > z_index::DEFAULT {
            let bounds = Rect2D::from_center_size(center, Vec2::new(radius * 2.0, radius * 2.0));
            self.register_hover_layer(self.z_index, bounds);
        }
    }

    /// Draw a rectangle with a border.
    pub(crate) fn draw_rect_border(
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
    pub(crate) fn draw_selection_border(&mut self, bounds: Rect2D, color: Color, width: f32) {
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
    pub(crate) fn draw_rounded_selection_border(
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
    pub(crate) fn draw_image(
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
    pub(crate) fn draw_line(&mut self, start: Vec2, end: Vec2, color: Color, thickness: f32) {
        self.draw_list.set_clip(self.clip_rect());
        self.draw_list.add_line(start, end, color, thickness);
    }

    /// Draw text using the font system with cosmic-text shaping.
    ///
    /// Text is rendered as textured quads from the font atlas, using cosmic-text
    /// for proper text shaping (kerning, ligatures, BiDi, CJK, word wrapping).
    /// If no font is loaded, draws placeholder rectangles.
    ///
    /// `position` is the TOP-LEFT of the text bounding box.
    pub(crate) fn draw_text(&mut self, text: &str, position: Vec2, color: Color, size: f32) {
        if text.is_empty() {
            return;
        }

        let font_atlas = self.fonts.borrow().atlas_id();

        struct GlyphEntry {
            bounds: Rect2D,
            uv_rect: Rect2D,
            is_placeholder: bool,
        }

        let mut glyphs = Vec::new();
        let scale = self.scale_factor;

        {
            let mut fonts = self.fonts.borrow_mut();

            let shaped = fonts.shape_text(self.current_font, text, size, scale, None);

            match shaped {
                Some(shaped) => {
                    for run in shaped.buffer.layout_runs() {
                        for glyph in run.glyphs.iter() {
                            let physical = glyph.physical((0.0, 0.0), scale);

                            let cached = fonts.get_or_rasterize_shaped(physical.cache_key, scale);

                            if let Some(cached) = cached {
                                if cached.size.x() == 0.0 || cached.size.y() == 0.0 {
                                    continue;
                                }

                                // All positions in logical pixels for the shader.
                                // physical.x is in physical pixels (from glyph.physical with
                                // scale), convert back to logical. Cached offsets are already
                                // logical (divided by scale_factor during rasterization).
                                let pos_x =
                                    position.x() + physical.x as f32 / scale + cached.offset_x;
                                let pos_y = position.y() + run.line_y + glyph.y
                                    - glyph.font_size * glyph.y_offset
                                    - cached.top_offset;

                                glyphs.push(GlyphEntry {
                                    bounds: Rect2D::from_origin_size(
                                        Vec2::new(pos_x, pos_y),
                                        cached.size,
                                    ),
                                    uv_rect: cached.uv_rect,
                                    is_placeholder: false,
                                });
                            } else {
                                let placeholder_size = Vec2::new(size * 0.6, size);
                                glyphs.push(GlyphEntry {
                                    bounds: Rect2D::from_origin_size(
                                        Vec2::new(
                                            position.x() + physical.x as f32 / scale,
                                            position.y() + run.line_y,
                                        ),
                                        placeholder_size,
                                    ),
                                    uv_rect: Rect2D::default(),
                                    is_placeholder: true,
                                });
                            }
                        }
                    }
                }
                None => {
                    let placeholder_size = Vec2::new(size * 0.6, size);
                    glyphs.push(GlyphEntry {
                        bounds: Rect2D::from_origin_size(position, placeholder_size),
                        uv_rect: Rect2D::default(),
                        is_placeholder: true,
                    });
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
    pub(crate) fn measure_text(&self, text: &str, size: f32) -> Vec2 {
        self.fonts
            .borrow_mut()
            .measure_text(self.current_font, text, size, self.scale_factor)
    }

    /// Measure the size of an icon character using the icon font.
    #[inline]
    pub(crate) fn measure_icon(&self, icon: char, size: f32) -> Vec2 {
        let mut buf = [0u8; 4];
        let icon_str = icon.encode_utf8(&mut buf);
        self.fonts
            .borrow_mut()
            .measure_text(crate::FontId::ICON, icon_str, size, self.scale_factor)
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
    pub(crate) fn draw_icon(&mut self, icon: char, position: Vec2, size: f32, color: Color) {
        let prev_font = self.current_font;
        self.current_font = FontId::ICON;

        let mut buf = [0u8; 4];
        let icon_str = icon.encode_utf8(&mut buf);
        self.draw_text(icon_str, position, color, size);

        self.current_font = prev_font;
    }

    pub fn set_font(&mut self, font_id: FontId) {
        self.current_font = font_id;
    }
}
