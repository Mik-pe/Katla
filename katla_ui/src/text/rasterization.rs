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

            let mut alpha = vec![0u8; width * height];

            for y in 0..height {
                for x in 0..width {
                    let idx = y * width + x;
                    let coverage_f = image.data[idx] as f32 / 255.0;
                    alpha[idx] = (coverage_to_alpha(coverage_f) * 255.0) as u8;
                }
            }

            Some((
                alpha,
                width,
                height,
                image.placement.left,
                image.placement.top,
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

    /// Rasterize a shaped glyph by cosmic-text CacheKey and add to the atlas.
    ///
    /// This is used for rendering glyphs from cosmic-text layout runs, where
    /// the glyph is identified by its shaped glyph ID and font rather than by
    /// character. Font fallback is handled automatically via the CacheKey's
    /// embedded fontdb::ID.
    ///
    /// Returns the cached glyph info if successful.
    pub fn get_or_rasterize_shaped(
        &mut self,
        cache_key: cosmic_text::CacheKey,
        scale_factor: f32,
    ) -> Option<CachedGlyph> {
        if let Some(cached) = self.shaped_cache.get(&cache_key) {
            return Some(*cached);
        }

        let physical_size = f32::from_bits(cache_key.font_size_bits);

        let cosmic_id = cache_key.font_id;
        let glyph_id = cache_key.glyph_id;

        let font = self
            .cosmic
            .font_system_mut()
            .get_font(cosmic_id, cache_key.font_weight)?;

        let swash_font = font.as_swash();

        let subpixel_x = cache_key.x_bin.as_float();
        let subpixel_y = cache_key.y_bin.as_float();

        let pixels = self.glyph_pool.acquire(|cx| {
            let mut scaler = cx.builder(swash_font).size(physical_size).build();

            let offset = swash::zeno::Vector::new(subpixel_x, subpixel_y);
            let image = Render::new(&[Source::Outline])
                .format(Format::Alpha)
                .offset(offset)
                .render(&mut scaler, glyph_id)?;

            let width = image.placement.width as usize;
            let height = image.placement.height as usize;

            if width == 0 || height == 0 {
                return Some(RasterizedPixels {
                    data: Vec::new(),
                    width: 0,
                    height: 0,
                    left: image.placement.left,
                    top: image.placement.top,
                });
            }

            let mut alpha = vec![0u8; width * height];

            for y in 0..height {
                for x in 0..width {
                    let idx = y * width + x;
                    let coverage_f = image.data[idx] as f32 / 255.0;
                    alpha[idx] = (coverage_to_alpha(coverage_f) * 255.0) as u8;
                }
            }

            Some(RasterizedPixels {
                data: alpha,
                width,
                height,
                left: image.placement.left,
                top: image.placement.top,
            })
        });

        let pixels = match pixels {
            Some(p) => p,
            None => {
                let cached = CachedGlyph {
                    uv_rect: Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0)),
                    size: Vec2::new(0.0, 0.0),
                    offset_x: 0.0,
                    top_offset: 0.0,
                    ascender: 0.0,
                    advance: 0.0,
                };
                self.shaped_cache.insert(cache_key, cached);
                return Some(cached);
            }
        };

        if pixels.width == 0 || pixels.height == 0 {
            let cached = CachedGlyph {
                uv_rect: Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0)),
                size: Vec2::new(0.0, 0.0),
                offset_x: pixels.left as f32 / scale_factor,
                top_offset: pixels.top as f32 / scale_factor,
                ascender: 0.0,
                advance: 0.0,
            };
            self.shaped_cache.insert(cache_key, cached);
            return Some(cached);
        }

        let rasterized = RasterizedGlyph {
            c: '\0',
            pixels: pixels.data,
            width: pixels.width,
            height: pixels.height,
            offset_x: pixels.left as f32 / scale_factor,
            top_offset: pixels.top as f32 / scale_factor,
            ascender: 0.0,
            advance: 0.0,
        };

        let cached = self.place_in_atlas(&rasterized, scale_factor)?;

        self.shaped_cache.insert(cache_key, cached);

        Some(cached)
    }
}

struct RasterizedPixels {
    data: Vec<u8>,
    width: usize,
    height: usize,
    left: i32,
    top: i32,
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

        let mut pixels = vec![0u8; width * height];

        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                let coverage_f = image.data[idx] as f32 / 255.0;
                let alpha = coverage_f.powf(1.0 / 1.45);
                pixels[idx] = (alpha * 255.0) as u8;
            }
        }

        let glyph = RasterizedGlyph {
            c,
            pixels,
            width,
            height,
            offset_x: image.placement.left as f32,
            top_offset: image.placement.top as f32,
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

    /// Count non-zero pixels in the glyph bitmap.
    fn nonzero_pixel_count(glyph: &RasterizedGlyph) -> usize {
        glyph.pixels.iter().filter(|&&p| p > 0).count()
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

        let nonzero = nonzero_pixel_count(&glyph);
        eprintln!("Non-zero pixels: {}", nonzero);

        assert!(glyph.width > 0, "Glyph should have non-zero width");
        assert!(glyph.height > 0, "Glyph should have non-zero height");
        assert!(nonzero > 0, "Glyph should have non-zero pixels");
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

        let nonzero = nonzero_pixel_count(&glyph);
        eprintln!("Non-zero pixels: {}", nonzero);

        assert!(glyph.width > 0, "Glyph should have non-zero width");
        assert!(glyph.height > 0, "Glyph should have non-zero height");
        assert!(nonzero > 0, "Glyph should have non-zero pixels");
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

        let nonzero = nonzero_pixel_count(&glyph);
        eprintln!("Non-zero pixels: {}", nonzero);

        assert!(glyph.width > 0, "Glyph should have non-zero width");
        assert!(glyph.height > 0, "Glyph should have non-zero height");
        assert!(nonzero > 0, "Glyph should have non-zero pixels");
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

            let nonzero = nonzero_pixel_count(&glyph);
            eprintln!("Non-zero pixels: {}", nonzero);

            assert!(
                glyph.width > 0,
                "Subpixel {}: glyph should have non-zero width",
                bin_name
            );
            assert!(
                glyph.height > 0,
                "Subpixel {}: glyph should have non-zero height",
                bin_name
            );
            assert!(
                nonzero > 0,
                "Subpixel {}: glyph should have non-zero pixels",
                bin_name
            );
        }
    }
}
