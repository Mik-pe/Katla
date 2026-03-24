use super::*;

use ab_glyph::{Font, PxScale, ScaleFont};
use katla_math::Vec2;

impl super::FontSystem {
    /// Get font metrics for a given size.
    ///
    /// Returns (ascent, descent, line_gap) in logical pixels.
    pub fn get_font_metrics(
        &self,
        font_id: FontId,
        size: f32,
        scale_factor: f32,
    ) -> Option<(f32, f32, f32)> {
        let font = self.fonts.get(&font_id)?;
        let physical_size = size * scale_factor;
        let scaled_font = font.as_scaled(PxScale::from(physical_size));

        Some((
            scaled_font.ascent() / scale_factor,
            scaled_font.descent() / scale_factor,
            scaled_font.line_gap() / scale_factor,
        ))
    }

    /// Get kerning between two characters.
    ///
    /// Returns the kerning adjustment in logical pixels.
    pub fn get_kerning(
        &self,
        font_id: FontId,
        left: char,
        right: char,
        size: f32,
        scale_factor: f32,
    ) -> f32 {
        let Some(font) = self.fonts.get(&font_id) else {
            return 0.0;
        };

        let left_id = font.glyph_id(left);
        let right_id = font.glyph_id(right);
        let unscaled_kern = font.kern_unscaled(left_id, right_id);

        let physical_size = size * scale_factor;
        let scaled_kern = unscaled_kern * physical_size / font.units_per_em().unwrap_or(1.0);
        scaled_kern / scale_factor
    }

    /// Measure text dimensions without rendering.
    ///
    /// Returns dimensions in logical pixels.
    /// This method includes kerning between character pairs.
    pub fn measure_text(&self, font_id: FontId, text: &str, size: f32, scale_factor: f32) -> Vec2 {
        let font = match self.fonts.get(&font_id) {
            Some(f) => f,
            None => return Vec2::new(0.0, 0.0),
        };

        let size_key = FontSizeKey::from_f32(size);
        let scale_key = ScaleFactorKey::from_f32(scale_factor);

        let physical_size = size * scale_factor;
        let scaled_font = font.as_scaled(PxScale::from(physical_size));

        let line_height = scaled_font.height() / scale_factor;

        let mut line_width = 0.0f32;
        let mut max_width = 0.0f32;
        let mut line_count = 1u32;
        let mut prev_char: Option<char> = None;

        for c in text.chars() {
            if c == '\n' {
                max_width = max_width.max(line_width);
                line_count += 1;
                line_width = 0.0;
                prev_char = None;
                continue;
            }

            if let Some(prev) = prev_char {
                line_width += self.get_kerning(font_id, prev, c, size, scale_factor);
            }

            if let Some(cached) =
                self.glyph_cache
                    .get(&(font_id, c, size_key, scale_key, SubpixelBin::Zero))
            {
                line_width += cached.advance;
            } else {
                let glyph_id = font.glyph_id(c);
                line_width += scaled_font.h_advance(glyph_id) / scale_factor;
            }

            prev_char = Some(c);
        }

        max_width = max_width.max(line_width);

        let total_height = line_count as f32 * line_height;

        Vec2::new(max_width, total_height)
    }

    /// Pre-cache common ASCII characters for a font.
    ///
    /// This pre-caches all 4 subpixel bins for each character for optimal
    /// rendering performance at any position.
    pub fn precache_ascii(&mut self, font_id: FontId, size: f32, scale_factor: f32) {
        for c in ' '..='~' {
            for bin in [
                SubpixelBin::Zero,
                SubpixelBin::One,
                SubpixelBin::Two,
                SubpixelBin::Three,
            ] {
                self.get_or_rasterize(font_id, c, size, scale_factor, bin);
            }
        }
    }

    /// Pre-cache common icons for an icon font.
    ///
    /// Only caches SubpixelBin::Zero for icons (usually positioned at integer coords).
    pub fn precache_icons(
        &mut self,
        font_id: FontId,
        size: f32,
        scale_factor: f32,
        icons: &[char],
    ) {
        for &icon in icons {
            self.get_or_rasterize(font_id, icon, size, scale_factor, SubpixelBin::Zero);
        }
    }
}
