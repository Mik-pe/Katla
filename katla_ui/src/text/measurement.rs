use super::*;

use katla_math::Vec2;
use skrifa::{
    MetadataProvider,
    instance::{LocationRef, Size},
    raw::TableProvider,
};

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
        let font = self.get_font(font_id)?;
        let physical_size = size * scale_factor;
        let metrics = font.metrics(Size::new(physical_size), LocationRef::default());

        Some((
            metrics.ascent / scale_factor,
            (-metrics.descent) / scale_factor,
            metrics.leading / scale_factor,
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
        let font = match self.get_font(font_id) {
            Some(f) => f,
            None => return 0.0,
        };

        let glyph_left = match font.charmap().map(left) {
            Some(id) => id,
            None => return 0.0,
        };
        let glyph_right = match font.charmap().map(right) {
            Some(id) => id,
            None => return 0.0,
        };

        let kern_table = match font.kern() {
            Ok(kern) => kern,
            Err(_) => return 0.0,
        };

        let mut total_kerning = 0i32;
        for subtable in kern_table.subtables().flatten() {
            if !subtable.is_horizontal() || subtable.is_cross_stream() {
                continue;
            }
            if let Ok(kind) = subtable.kind() {
                let value = match kind {
                    skrifa::raw::tables::kern::SubtableKind::Format0(t) => {
                        t.kerning(glyph_left, glyph_right)
                    }
                    skrifa::raw::tables::kern::SubtableKind::Format2(t) => {
                        t.kerning(glyph_left, glyph_right)
                    }
                    _ => None,
                };
                if let Some(v) = value {
                    total_kerning += v;
                }
            }
        }

        if total_kerning == 0 {
            return 0.0;
        }

        let physical_size = size * scale_factor;
        let units_per_em = match font.head() {
            Ok(head) => head.units_per_em(),
            Err(_) => return 0.0,
        };
        if units_per_em == 0 {
            return 0.0;
        }

        let scale = physical_size / units_per_em as f32;
        (total_kerning as f32 * scale) / scale_factor
    }

    /// Measure text dimensions using cosmic-text for proper shaping.
    ///
    /// Returns dimensions in logical pixels.
    /// This method uses cosmic-text Buffer for shaping, which handles kerning,
    /// ligatures, BiDi, CJK line breaking, and font fallback.
    #[inline]
    pub fn measure_text(
        &mut self,
        font_id: FontId,
        text: &str,
        size: f32,
        scale_factor: f32,
    ) -> Vec2 {
        self.measure_text_shaped(font_id, text, size, scale_factor)
    }

    /// Measure text dimensions using legacy char-by-char approach.
    ///
    /// This is kept as a fallback for when cosmic-text is not available
    /// (e.g., when no font family name is registered).
    #[allow(dead_code)]
    pub fn measure_text_legacy(
        &self,
        font_id: FontId,
        text: &str,
        size: f32,
        scale_factor: f32,
    ) -> Vec2 {
        let font = match self.get_font(font_id) {
            Some(f) => f,
            None => return Vec2::new(0.0, 0.0),
        };

        let size_key = FontSizeKey::from_f32(size);
        let scale_key = ScaleFactorKey::from_f32(scale_factor);

        let physical_size = size * scale_factor;
        let metrics = font.metrics(Size::new(physical_size), LocationRef::default());
        let line_height = (metrics.ascent - metrics.descent + metrics.leading) / scale_factor;

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
                let glyph_id = font.charmap().map(c);
                let advance = glyph_id
                    .and_then(|g| {
                        font.glyph_metrics(Size::new(physical_size), LocationRef::default())
                            .advance_width(g)
                    })
                    .unwrap_or(0.0);
                line_width += advance / scale_factor;
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
            for bin in [SubpixelBin::Zero, SubpixelBin::One, SubpixelBin::Two] {
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
