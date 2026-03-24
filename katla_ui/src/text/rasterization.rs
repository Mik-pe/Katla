use super::*;

use ab_glyph::{Font, Glyph, PxScale, ScaleFont};
use katla_math::{Rect2D, Vec2};

/// A glyph ready for placement in the atlas.
#[derive(Debug, Clone)]
pub(super) struct RasterizedGlyph {
    /// The character.
    pub c: char,
    /// Pixel data (8-bit alpha only).
    pub pixels: Vec<u8>,
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// Horizontal offset (left side bearing).
    pub offset_x: f32,
    /// Distance from baseline to top of glyph bitmap in screen coords.
    /// Positive value - how far up from baseline the top edge is.
    pub top_offset: f32,
    /// Font ascender for consistent top alignment.
    pub ascender: f32,
    /// Horizontal advance.
    pub advance: f32,
}

impl super::FontSystem {
    /// Rasterize a glyph and add to the atlas.
    ///
    /// Returns the cached glyph info if successful.
    ///
    /// # Arguments
    /// * `font_id` - The font to use
    /// * `c` - The character to rasterize
    /// * `logical_size` - Font size in logical pixels
    /// * `scale_factor` - DPI scale factor (physical pixels per logical pixel)
    /// * `subpixel_bin` - Subpixel position bin for crisp fractional positioning
    pub fn get_or_rasterize(
        &mut self,
        font_id: FontId,
        c: char,
        logical_size: f32,
        scale_factor: f32,
        subpixel_bin: SubpixelBin,
    ) -> Option<CachedGlyph> {
        let size_key = FontSizeKey::from_f32(logical_size);
        let scale_key = ScaleFactorKey::from_f32(scale_factor);

        if let Some(cached) = self
            .glyph_cache
            .get(&(font_id, c, size_key, scale_key, subpixel_bin))
        {
            return Some(*cached);
        }

        let font = self.fonts.get(&font_id)?;

        let physical_size = logical_size * scale_factor;

        let scaled_font = font.as_scaled(PxScale::from(physical_size));

        let glyph_id = font.glyph_id(c);

        let ascender = scaled_font.ascent();

        let advance = scaled_font.h_advance(glyph_id);

        let subpixel_offset = subpixel_bin.as_offset() * scale_factor;

        let glyph = Glyph {
            id: glyph_id,
            scale: PxScale::from(physical_size),
            position: ab_glyph::point(subpixel_offset, 0.0),
        };

        let outlined = match font.outline_glyph(glyph) {
            Some(o) => o,
            None => {
                let cached = CachedGlyph {
                    uv_rect: Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0)),
                    size: Vec2::new(0.0, 0.0),
                    offset_x: 0.0,
                    top_offset: 0.0,
                    ascender: ascender / scale_factor,
                    advance: advance / scale_factor,
                };
                self.glyph_cache
                    .insert((font_id, c, size_key, scale_key, subpixel_bin), cached);
                return Some(cached);
            }
        };

        let bounds = outlined.px_bounds();

        let glyph_for_metrics = Glyph {
            id: glyph_id,
            scale: PxScale::from(physical_size),
            position: ab_glyph::point(0.0, 0.0),
        };
        let metrics_bounds = font
            .outline_glyph(glyph_for_metrics)
            .map(|g| g.px_bounds())
            .unwrap_or(bounds);

        let offset_x = metrics_bounds.min.x / scale_factor;
        let top_offset = -metrics_bounds.min.y / scale_factor;

        let width = metrics_bounds.width().ceil() as usize;
        let height = metrics_bounds.height().ceil() as usize;

        if width == 0 || height == 0 {
            let cached = CachedGlyph {
                uv_rect: Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0)),
                size: Vec2::new(0.0, 0.0),
                offset_x: 0.0,
                top_offset: 0.0,
                ascender: ascender / scale_factor,
                advance: advance / scale_factor,
            };
            self.glyph_cache
                .insert((font_id, c, size_key, scale_key, subpixel_bin), cached);
            return Some(cached);
        }

        let mut pixels = vec![0u8; width * height];
        outlined.draw(|x, y, coverage| {
            let px = x as usize;
            let py = y as usize;
            if px < width && py < height {
                let alpha = coverage_to_alpha(coverage);
                pixels[py * width + px] = (alpha * 255.0) as u8;
            }
        });

        let rasterized = RasterizedGlyph {
            c,
            pixels,
            width,
            height,
            offset_x,
            top_offset,
            ascender: ascender / scale_factor,
            advance: advance / scale_factor,
        };

        let cached = self.place_in_atlas(&rasterized, scale_factor)?;

        self.glyph_cache
            .insert((font_id, c, size_key, scale_key, subpixel_bin), cached);

        Some(cached)
    }
}
