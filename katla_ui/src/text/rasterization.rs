use super::*;

use skrifa::{
    instance::{LocationRef, Size},
    outline::{pen::ControlBoundsPen, DrawSettings, OutlinePen},
    MetadataProvider,
};
use vello_cpu::kurbo::{Affine, BezPath, Point};
use vello_cpu::{Pixmap, RenderContext};

use katla_math::{Rect2D, Vec2};

/// Adapter that converts skrifa outline drawing commands into a kurbo BezPath.
///
/// The outline coordinates from skrifa (with unhinted DrawSettings) are in
/// scaled pixel space with Y pointing up. This pen records the path as-is;
/// Y-flipping is handled via the vello_cpu transform.
struct KurboPen(BezPath);

impl OutlinePen for KurboPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(Point::new(x as f64, y as f64));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(Point::new(x as f64, y as f64));
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.0.quad_to(Point::new(cx as f64, cy as f64), Point::new(x as f64, y as f64));
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.curve_to(
            Point::new(cx0 as f64, cy0 as f64),
            Point::new(cx1 as f64, cy1 as f64),
            Point::new(x as f64, y as f64),
        );
    }
    fn close(&mut self) {
        self.0.close_path();
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

        // Compute bounds using ControlBoundsPen
        let mut bounds_pen = ControlBoundsPen::default();
        if outline_glyph
            .draw(DrawSettings::unhinted(size, location), &mut bounds_pen)
            .is_err()
        {
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

        let (x_min, y_min, x_max, y_max) = match bounds_pen.bounding_box() {
            Some(bb) => (bb.x_min, bb.y_min, bb.x_max, bb.y_max),
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

        // Bounds are in font coordinate space (Y up). Convert to pixel dimensions.
        // x_min can be negative (e.g. overshoot), y_min is the bottom, y_max is the top.
        let glyph_width = (x_max - x_min).ceil() as usize;
        let glyph_height = (y_max - y_min).ceil() as usize;

        // offset_x is the left side bearing in logical pixels
        let offset_x = x_min / scale_factor;
        // top_offset: distance from baseline to top of glyph bitmap in screen coords (y-down).
        // In font coords, y_max is the top edge. In screen coords, the top of the bitmap
        // is at y_max pixels above the baseline, which is y_max / scale_factor.
        let top_offset = y_max / scale_factor;

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

        let subpixel_offset = subpixel_bin.as_offset() * scale_factor;

        // Build the outline path and rasterize with vello_cpu.
        // The skrifa outline is in pixel coords with Y up. vello_cpu uses Y down.
        // We apply a Y-flip transform: translate by height, then scale Y by -1.
        let glyph_width_u16 = glyph_width as u16;
        let glyph_height_u16 = glyph_height as u16;

        let path = BezPath::new();
        let mut kurbo_pen = KurboPen(path);

        if outline_glyph.draw(settings, &mut kurbo_pen).is_err() {
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

        let path = kurbo_pen.0;

        // Transform: translate to account for bounds offset and subpixel positioning,
        // then flip Y axis (font Y-up -> screen Y-down).
        // The glyph origin is at (x_min, y_min) in font coords.
        // We need to shift so that the bottom-left of the glyph maps to (0, height)
        // and the top-left maps to (0, 0) in screen coords.
        let tx = -x_min as f64 + subpixel_offset as f64;
        let ty = y_max as f64; // translate to align top of glyph at y=0 after flip
        let transform = Affine::translate((tx, ty)) * Affine::scale_non_uniform(1.0, -1.0);

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
