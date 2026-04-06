use super::*;

use skrifa::{
    MetadataProvider,
    instance::{LocationRef, Size},
    outline::{DrawSettings, OutlinePen},
};
use vello_cpu::kurbo::{Affine, BezPath, Point};
use vello_cpu::{Pixmap, RenderContext};

use katla_math::{Rect2D, Vec2};

/// Adapter that converts skrifa outline drawing commands into a kurbo BezPath.
///
/// Flips Y coordinates at the pen level (font Y-up to screen Y-down) so that
/// kurbo path bounds are already in screen space. This avoids needing a separate
/// Y-flip transform on the render context.
struct VelloPen {
    path: BezPath,
    x_offset: f64,
}

impl VelloPen {
    fn new(x_offset: f64) -> Self {
        Self {
            path: BezPath::new(),
            x_offset,
        }
    }
}

impl OutlinePen for VelloPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path
            .move_to(Point::new(x as f64 + self.x_offset, -y as f64));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.path
            .line_to(Point::new(x as f64 + self.x_offset, -y as f64));
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.path.quad_to(
            Point::new(cx as f64 + self.x_offset, -cy as f64),
            Point::new(x as f64 + self.x_offset, -y as f64),
        );
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.path.curve_to(
            Point::new(cx0 as f64 + self.x_offset, -cy0 as f64),
            Point::new(cx1 as f64 + self.x_offset, -cy1 as f64),
            Point::new(x as f64 + self.x_offset, -y as f64),
        );
    }
    fn close(&mut self) {
        self.path.close_path();
    }
}

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

        let font = self.get_font(font_id)?;

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

        let outline_glyph = match font.outline_glyphs().get(glyph_id) {
            Some(g) => g,
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

        let settings = DrawSettings::unhinted(size, location);
        let subpixel_offset = subpixel_bin.as_offset() * scale_factor;

        // Build the outline path with Y-flipped coords and subpixel offset.
        // Y is negated in the pen so bounds are already in screen space (Y-down).
        let mut pen = VelloPen::new(subpixel_offset as f64);

        if outline_glyph.draw(settings, &mut pen).is_err() {
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

        let path = pen.path;

        // Get screen-space bounds from the already-flipped path.
        // expand() adds a small margin for curve overshoot, and we add 1px padding
        // on all sides so antialiased edges don't get clipped at bitmap boundaries.
        let bounds = path.control_box().expand();
        let padded = vello_cpu::kurbo::Rect::new(
            bounds.x0 - 1.0,
            bounds.y0 - 1.0,
            bounds.x1 + 1.0,
            bounds.y1 + 1.0,
        );
        let glyph_width = padded.width().ceil() as usize;
        let glyph_height = padded.height().ceil() as usize;

        // Bounds are in screen coords (Y-down). padded.y0 is the top edge (most negative),
        // padded.y1 is the bottom edge.
        let offset_x = padded.x0 as f32 / scale_factor;
        let top_offset = (-padded.y0 as f32) / scale_factor;

        if glyph_width == 0 || glyph_height == 0 {
            let cached = CachedGlyph {
                uv_rect: Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0)),
                size: Vec2::new(0.0, 0.0),
                offset_x,
                top_offset,
                ascender: ascender / scale_factor,
                advance: advance / scale_factor,
            };
            self.glyph_cache
                .insert((font_id, c, size_key, scale_key, subpixel_bin), cached);
            return Some(cached);
        }

        let glyph_width_u16 = glyph_width as u16;
        let glyph_height_u16 = glyph_height as u16;

        // Simple translate to map the path into the bitmap origin.
        let transform = Affine::translate((-padded.x0, -padded.y0));

        let mut ctx = RenderContext::new(glyph_width_u16, glyph_height_u16);
        ctx.set_paint(vello_cpu::peniko::color::palette::css::WHITE);
        ctx.set_transform(transform);
        ctx.fill_path(&path);
        ctx.flush();

        let mut pixmap = Pixmap::new(glyph_width_u16, glyph_height_u16);
        ctx.render_to_pixmap(&mut pixmap);

        // Extract alpha channel from premultiplied RGBA.
        // For white text on transparent, R=G=B=A in premultiplied, so the alpha byte
        // at offset 3 gives us the coverage directly.
        let pixel_data = pixmap.data_as_u8_slice();
        let mut pixels = vec![0u8; glyph_width * glyph_height];
        for i in 0..glyph_width * glyph_height {
            let alpha_raw = pixel_data[i * 4 + 3];
            let coverage = alpha_raw as f32 / 255.0;
            let alpha = coverage_to_alpha(coverage);
            pixels[i] = (alpha * 255.0) as u8;
        }

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
    use vello_cpu::kurbo::Affine;
    use vello_cpu::{Pixmap, RenderContext};

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
    ///
    /// Uses the same logic as production code (VelloPen + padded bounds).
    fn rasterize_glyph_raw(
        font_data: &[u8],
        c: char,
        size_px: f32,
        subpixel_offset: f64,
    ) -> Option<(RasterizedGlyph, vello_cpu::kurbo::Rect)> {
        use skrifa::instance::Size;
        use skrifa::{MetadataProvider, instance::LocationRef, outline::DrawSettings};

        let font = skrifa::FontRef::new(font_data).ok()?;
        let size = Size::new(size_px);
        let location = LocationRef::default();

        let glyph_id = font.charmap().map(c)?;
        let outline_glyph = font.outline_glyphs().get(glyph_id)?;
        let settings = DrawSettings::unhinted(size, location);

        let mut pen = super::VelloPen::new(subpixel_offset);
        outline_glyph.draw(settings, &mut pen).ok()?;
        let path = pen.path;

        // Same as production: expand() + 1px padding on all sides
        let bounds = path.control_box().expand();
        let padded = vello_cpu::kurbo::Rect::new(
            bounds.x0 - 1.0,
            bounds.y0 - 1.0,
            bounds.x1 + 1.0,
            bounds.y1 + 1.0,
        );
        let glyph_width = padded.width().ceil() as usize;
        let glyph_height = padded.height().ceil() as usize;

        if glyph_width == 0 || glyph_height == 0 {
            return None;
        }

        let w = glyph_width as u16;
        let h = glyph_height as u16;
        let transform = Affine::translate((-padded.x0, -padded.y0));

        let mut ctx = RenderContext::new(w, h);
        ctx.set_paint(vello_cpu::peniko::color::palette::css::WHITE);
        ctx.set_transform(transform);
        ctx.fill_path(&path);
        ctx.flush();

        let mut pixmap = Pixmap::new(w, h);
        ctx.render_to_pixmap(&mut pixmap);

        let pixel_data = pixmap.data_as_u8_slice();
        let mut pixels = vec![0u8; glyph_width * glyph_height];
        for i in 0..glyph_width * glyph_height {
            let alpha_raw = pixel_data[i * 4 + 3];
            let coverage = alpha_raw as f32 / 255.0;
            let alpha = coverage.powf(1.0 / 1.45);
            pixels[i] = (alpha * 255.0) as u8;
        }

        let glyph = RasterizedGlyph {
            c,
            pixels,
            width: glyph_width,
            height: glyph_height,
            offset_x: padded.x0 as f32,
            top_offset: -padded.y0 as f32,
            ascender: 0.0,
            advance: 0.0,
        };

        Some((glyph, padded))
    }

    /// Render the glyph pixels as a text grid for debugging.
    /// Each pixel is represented by a character based on alpha value.
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

        let (glyph, bounds) =
            rasterize_glyph_raw(&font_data, 'E', size, 0.0).expect("Failed to rasterize 'E'");

        eprintln!("\n=== Glyph 'E' at {}px ===", size);
        eprintln!("Bitmap: {}x{}", glyph.width, glyph.height);
        eprintln!(
            "Bounds: x0={:.2} y0={:.2} x1={:.2} y1={:.2}",
            bounds.x0, bounds.y0, bounds.x1, bounds.y1
        );
        eprintln!("{}", render_pixel_grid(&glyph, 10));

        let edges = edge_coverage(&glyph, 10);
        eprintln!(
            "Edge pixels: top={} bottom={} left={} right={}",
            edges.top, edges.bottom, edges.left, edges.right
        );

        // With 1px padding, no edge should have non-zero pixels
        assert_eq!(edges.top, 0, "Top edge should be empty with padding");
        assert_eq!(edges.bottom, 0, "Bottom edge should be empty with padding");
        assert_eq!(edges.left, 0, "Left edge should be empty with padding");
        assert_eq!(edges.right, 0, "Right edge should be empty with padding");
    }

    #[test]
    fn test_glyph_m_no_clipping() {
        let font_data = load_roboto();
        let size = 32.0;

        let (glyph, bounds) =
            rasterize_glyph_raw(&font_data, 'M', size, 0.0).expect("Failed to rasterize 'M'");

        eprintln!("\n=== Glyph 'M' at {}px ===", size);
        eprintln!("Bitmap: {}x{}", glyph.width, glyph.height);
        eprintln!(
            "Bounds: x0={:.2} y0={:.2} x1={:.2} y1={:.2}",
            bounds.x0, bounds.y0, bounds.x1, bounds.y1
        );
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

        let (glyph, bounds) =
            rasterize_glyph_raw(&font_data, 'O', size, 0.0).expect("Failed to rasterize 'O'");

        eprintln!("\n=== Glyph 'O' at {}px ===", size);
        eprintln!("Bitmap: {}x{}", glyph.width, glyph.height);
        eprintln!(
            "Bounds: x0={:.2} y0={:.2} x1={:.2} y1={:.2}",
            bounds.x0, bounds.y0, bounds.x1, bounds.y1
        );
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

    /// Test that subpixel offsets don't cause clipping at any edge.
    #[test]
    fn test_glyph_e_subpixel_no_clipping() {
        let font_data = load_roboto();
        let size = 32.0;

        for (bin_name, offset) in [("zero", 0.0), ("one", 0.25), ("two", 0.5), ("three", 0.75)] {
            let (glyph, bounds) = rasterize_glyph_raw(&font_data, 'E', size, offset)
                .unwrap_or_else(|| panic!("Failed to rasterize 'E' with subpixel {}", bin_name));

            eprintln!(
                "\n=== Glyph 'E' subpixel {} (offset={}) ===",
                bin_name, offset
            );
            eprintln!("Bitmap: {}x{}", glyph.width, glyph.height);
            eprintln!("Bounds: x0={:.2} x1={:.2}", bounds.x0, bounds.x1);

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
