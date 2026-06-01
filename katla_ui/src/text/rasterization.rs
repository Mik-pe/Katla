use super::*;

use katla_math::{Rect2D, Vec2};
use skrifa::{
    MetadataProvider,
    instance::{LocationRef, Size},
};
use swash::scale::{Render, Source};
use swash::zeno::{Format, Vector};

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

        let font_data = self.fonts.get(&font_id)?;
        let font = skrifa::FontRef::new(font_data).ok()?;

        let physical_size = logical_size * scale_factor;
        let size = Size::new(physical_size);
        let location = LocationRef::default();

        let glyph_id = match font.charmap().map(c) {
            Some(id) => id,
            None => {
                let metrics = font.metrics(size, location);
                let cached = CachedGlyph {
                    uv_rect: Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0)),
                    size: Vec2::new(0.0, 0.0),
                    offset_x: 0.0,
                    top_offset: 0.0,
                    ascender: metrics.ascent / scale_factor,
                    advance: 0.0,
                };
                self.glyph_cache
                    .insert((font_id, c, size_key, scale_key, subpixel_bin), cached);
                return Some(cached);
            }
        };

        let metrics = font.metrics(size, location);
        let ascender = metrics.ascent;

        let advance = font
            .glyph_metrics(size, location)
            .advance_width(glyph_id)
            .unwrap_or(0.0);

        if font.outline_glyphs().get(glyph_id).is_none() {
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

        let subpixel_offset = subpixel_bin.as_offset() * scale_factor;
        let swash_glyph_id = glyph_id.to_u32() as swash::GlyphId;

        let pixels = self.glyph_pool.acquire(|cx| {
            let swash_font = swash::FontDataRef::new(font_data).and_then(|fd| fd.get(0))?;

            let mut scaler = cx.builder(swash_font).size(physical_size).build();

            let offset = Vector::new(subpixel_offset, 0.0);
            let image = Render::new(&[Source::Outline])
                .format(Format::Alpha)
                .offset(offset)
                .render(&mut scaler, swash_glyph_id)?;

            let width = image.placement.width as usize;
            let height = image.placement.height as usize;

            if width == 0 || height == 0 {
                return Some((
                    vec![0u8; 0],
                    0,
                    0,
                    image.placement.left,
                    image.placement.top,
                ));
            }

            // Add 1px padding on all sides to prevent edge clipping
            let padded_width = width + 2;
            let padded_height = height + 2;
            let mut alpha = vec![0u8; padded_width * padded_height];

            for y in 0..height {
                for x in 0..width {
                    let src_idx = y * width + x;
                    let dst_idx = (y + 1) * padded_width + (x + 1);
                    let coverage_f = image.data[src_idx] as f32 / 255.0;
                    alpha[dst_idx] = (coverage_to_alpha(coverage_f) * 255.0) as u8;
                }
            }

            Some((
                alpha,
                padded_width,
                padded_height,
                image.placement.left - 1,
                image.placement.top + 1,
            ))
        });

        let (pixels, glyph_width, glyph_height, placement_left, placement_top) = match pixels {
            Some(p) => p,
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

        if glyph_width == 0 || glyph_height == 0 {
            let cached = CachedGlyph {
                uv_rect: Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0)),
                size: Vec2::new(0.0, 0.0),
                offset_x: placement_left as f32 / scale_factor,
                top_offset: placement_top as f32 / scale_factor,
                ascender: ascender / scale_factor,
                advance: advance / scale_factor,
            };
            self.glyph_cache
                .insert((font_id, c, size_key, scale_key, subpixel_bin), cached);
            return Some(cached);
        }

        let offset_x = placement_left as f32 / scale_factor;
        let top_offset = placement_top as f32 / scale_factor;

        let rasterized = RasterizedGlyph {
            c,
            pixels,
            width: glyph_width,
            height: glyph_height,
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

#[cfg(test)]
mod tests {
    use super::RasterizedGlyph;
    use swash::scale::{Render, ScaleContext, Source};
    use swash::zeno::{Format, Vector};

    /// Load the bundled Roboto font. Panics if not found.
    fn load_roboto() -> Vec<u8> {
        let candidates = [
            "resources/fonts/roboto-regular.ttf",
            "../resources/fonts/roboto-regular.ttf",
            "../../resources/fonts/roboto-regular.ttf",
        ];
        for path in &candidates {
            if let Ok(data) = std::fs::read(path) {
                return data;
            }
        }
        panic!("Could not find roboto-regular.ttf from any candidate path");
    }

    /// Rasterize a single glyph and return the raw RasterizedGlyph with its pixels.
    fn rasterize_glyph_raw(
        font_data: &[u8],
        c: char,
        size_px: f32,
        subpixel_offset: f64,
    ) -> Option<RasterizedGlyph> {
        let font = swash::FontDataRef::new(font_data)?.get(0)?;
        let glyph_id = font.charmap().map(c);
        if glyph_id == 0 {
            return None;
        }

        let mut cx = ScaleContext::new();
        let mut scaler = cx.builder(font).size(size_px).build();
        let offset = Vector::new(subpixel_offset as f32, 0.0);

        let image = Render::new(&[Source::Outline])
            .format(Format::Alpha)
            .offset(offset)
            .render(&mut scaler, glyph_id)?;

        let width = image.placement.width as usize;
        let height = image.placement.height as usize;

        if width == 0 || height == 0 {
            return None;
        }

        // Add 1px padding on all sides
        let padded_width = width + 2;
        let padded_height = height + 2;
        let mut pixels = vec![0u8; padded_width * padded_height];

        for y in 0..height {
            for x in 0..width {
                let src_idx = y * width + x;
                let dst_idx = (y + 1) * padded_width + (x + 1);
                let coverage_f = image.data[src_idx] as f32 / 255.0;
                let alpha = coverage_f.powf(1.0 / 1.45);
                pixels[dst_idx] = (alpha * 255.0) as u8;
            }
        }

        let glyph = RasterizedGlyph {
            c,
            pixels,
            width: padded_width,
            height: padded_height,
            offset_x: (image.placement.left - 1) as f32,
            top_offset: (image.placement.top + 1) as f32,
            ascender: 0.0,
            advance: 0.0,
        };

        Some(glyph)
    }

    /// Render the glyph pixels as a text grid for debugging.
    fn render_pixel_grid(glyph: &RasterizedGlyph, threshold: u8) -> String {
        let mut output = String::new();
        for y in 0..glyph.height {
            for x in 0..glyph.width {
                let alpha = glyph.pixels[y * glyph.width + x];
                let ch = if alpha == 0 {
                    '.'
                } else if alpha < threshold {
                    '~'
                } else if alpha < 128 {
                    '+'
                } else if alpha < 220 {
                    '#'
                } else {
                    '@'
                };
                output.push(ch);
            }
            output.push('\n');
        }
        output
    }

    /// Check if any edge pixels have non-zero alpha (potential clipping indicator).
    fn edge_coverage(glyph: &RasterizedGlyph, threshold: u8) -> EdgeCoverage {
        let mut top = 0;
        let mut bottom = 0;
        let mut left = 0;
        let mut right = 0;

        for x in 0..glyph.width {
            if glyph.pixels[x] >= threshold {
                top += 1;
            }
            let last_row = (glyph.height - 1) * glyph.width + x;
            if glyph.pixels[last_row] >= threshold {
                bottom += 1;
            }
        }
        for y in 0..glyph.height {
            if glyph.pixels[y * glyph.width] >= threshold {
                left += 1;
            }
            let last_col = y * glyph.width + (glyph.width - 1);
            if glyph.pixels[last_col] >= threshold {
                right += 1;
            }
        }

        EdgeCoverage {
            top,
            bottom,
            left,
            right,
        }
    }

    struct EdgeCoverage {
        top: usize,
        bottom: usize,
        left: usize,
        right: usize,
    }

    #[test]
    fn test_glyph_e_no_clipping() {
        let font_data = load_roboto();
        let size = 32.0;

        let glyph =
            rasterize_glyph_raw(&font_data, 'E', size, 0.0).expect("Failed to rasterize 'E'");

        eprintln!("\n=== Glyph 'E' at {}px ===", size);
        eprintln!("Bitmap: {}x{}", glyph.width, glyph.height);
        eprintln!("{}", render_pixel_grid(&glyph, 10));

        let edges = edge_coverage(&glyph, 10);
        eprintln!(
            "Edge pixels: top={} bottom={} left={} right={}",
            edges.top, edges.bottom, edges.left, edges.right
        );

        assert_eq!(edges.top, 0, "Top edge should be empty with padding");
        assert_eq!(edges.bottom, 0, "Bottom edge should be empty with padding");
        assert_eq!(edges.left, 0, "Left edge should be empty with padding");
        assert_eq!(edges.right, 0, "Right edge should be empty with padding");
    }

    #[test]
    fn test_glyph_m_no_clipping() {
        let font_data = load_roboto();
        let size = 32.0;

        let glyph =
            rasterize_glyph_raw(&font_data, 'M', size, 0.0).expect("Failed to rasterize 'M'");

        eprintln!("\n=== Glyph 'M' at {}px ===", size);
        eprintln!("Bitmap: {}x{}", glyph.width, glyph.height);
        eprintln!("{}", render_pixel_grid(&glyph, 10));

        let edges = edge_coverage(&glyph, 10);
        eprintln!(
            "Edge pixels: top={} bottom={} left={} right={}",
            edges.top, edges.bottom, edges.left, edges.right
        );

        assert_eq!(edges.top, 0, "Top edge should be empty");
        assert_eq!(edges.bottom, 0, "Bottom edge should be empty");
        assert_eq!(edges.left, 0, "Left edge should be empty");
        assert_eq!(edges.right, 0, "Right edge should be empty");
    }

    #[test]
    fn test_glyph_o_no_clipping() {
        let font_data = load_roboto();
        let size = 32.0;

        let glyph =
            rasterize_glyph_raw(&font_data, 'O', size, 0.0).expect("Failed to rasterize 'O'");

        eprintln!("\n=== Glyph 'O' at {}px ===", size);
        eprintln!("Bitmap: {}x{}", glyph.width, glyph.height);
        eprintln!("{}", render_pixel_grid(&glyph, 10));

        let edges = edge_coverage(&glyph, 10);
        eprintln!(
            "Edge pixels: top={} bottom={} left={} right={}",
            edges.top, edges.bottom, edges.left, edges.right
        );

        assert_eq!(edges.top, 0, "Top edge should be empty");
        assert_eq!(edges.bottom, 0, "Bottom edge should be empty");
        assert_eq!(edges.left, 0, "Left edge should be empty");
        assert_eq!(edges.right, 0, "Right edge should be empty");
    }

    #[test]
    fn test_glyph_e_subpixel_no_clipping() {
        let font_data = load_roboto();
        let size = 32.0;

        for (bin_name, offset) in [("zero", 0.0), ("one", 1.0 / 3.0), ("two", 2.0 / 3.0)] {
            let glyph = rasterize_glyph_raw(&font_data, 'E', size, offset)
                .unwrap_or_else(|| panic!("Failed to rasterize 'E' with subpixel {}", bin_name));

            eprintln!(
                "\n=== Glyph 'E' subpixel {} (offset={}) ===",
                bin_name, offset
            );
            eprintln!("Bitmap: {}x{}", glyph.width, glyph.height);

            let edges = edge_coverage(&glyph, 10);
            eprintln!(
                "Edge pixels: top={} bottom={} left={} right={}",
                edges.top, edges.bottom, edges.left, edges.right
            );

            assert_eq!(
                edges.top, 0,
                "Subpixel {}: top edge should be empty",
                bin_name
            );
            assert_eq!(
                edges.bottom, 0,
                "Subpixel {}: bottom edge should be empty",
                bin_name
            );
            assert_eq!(
                edges.left, 0,
                "Subpixel {}: left edge should be empty",
                bin_name
            );
            assert_eq!(
                edges.right, 0,
                "Subpixel {}: right edge should be empty",
                bin_name
            );
        }
    }
}
