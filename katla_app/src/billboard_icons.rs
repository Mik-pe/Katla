//! Billboard icon texture generation.
//!
//! Rasterizes ForkAwesome icon glyphs into RGBA pixel buffers
//! for use as billboard textures in the editor viewport.

use katla_icons::ForkAwesome;
use skrifa::{
    MetadataProvider,
    instance::{LocationRef, Size},
    outline::{DrawSettings, OutlinePen},
};
use vello_cpu::kurbo::{Affine, BezPath, Point, Shape};
use vello_cpu::{Pixmap, RenderContext};

use crate::components::BillboardIcon;

/// Rasterized icon data ready for GPU upload.
pub struct RasterizedIcon {
    /// RGBA pixel data (width * height * 4 bytes).
    pub pixels: Vec<u8>,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Pen adapter that converts skrifa outline drawing commands into a kurbo BezPath.
///
/// Flips Y coordinates (font Y-up to screen Y-down) and applies a horizontal offset
/// to center the glyph within the target bitmap.
struct OutlineToPath {
    path: BezPath,
    x_offset: f64,
}

impl OutlineToPath {
    fn new(x_offset: f64) -> Self {
        Self {
            path: BezPath::new(),
            x_offset,
        }
    }
}

impl OutlinePen for OutlineToPath {
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

/// Map a [`BillboardIcon`] variant to its ForkAwesome codepoint.
fn icon_to_char(icon: BillboardIcon) -> char {
    match icon {
        BillboardIcon::Lightbulb => ForkAwesome::LIGHTBULB,
        BillboardIcon::Fire => ForkAwesome::FIRE,
    }
}

/// Rasterize a billboard icon into an RGBA pixel buffer.
///
/// The icon is rendered white on a transparent background, centered within
/// a square bitmap of the given `size`.
pub fn rasterize_icon(icon: BillboardIcon, size: u32) -> RasterizedIcon {
    let font_data = include_bytes!("../../resources/fonts/forkawesome-webfont.ttf");
    let font = match skrifa::FontRef::new(font_data) {
        Ok(f) => f,
        Err(e) => {
            log::error!("Failed to parse ForkAwesome font: {e}");
            return empty_icon(size);
        }
    };

    let glyph_char = icon_to_char(icon);

    let glyph_id = match font.charmap().map(glyph_char) {
        Some(id) => id,
        None => {
            log::error!("Glyph '{glyph_char}' not found in ForkAwesome font");
            return empty_icon(size);
        }
    };

    let size_obj = Size::new(size as f32);
    let location = LocationRef::default();

    let outline_glyph = match font.outline_glyphs().get(glyph_id) {
        Some(g) => g,
        None => {
            log::error!("No outline for glyph '{glyph_char}'");
            return empty_icon(size);
        }
    };

    let settings = DrawSettings::unhinted(size_obj, location);

    // Build the outline path with Y-flipped coordinates.
    let mut pen = OutlineToPath::new(0.0);
    if let Err(e) = outline_glyph.draw(settings, &mut pen) {
        log::error!("Failed to draw glyph outline: {e}");
        return empty_icon(size);
    }
    let path = pen.path;

    // Compute bounds with padding for antialiased edges.
    let bounds = path.bounding_box();
    let padded = vello_cpu::kurbo::Rect::new(
        bounds.x0 - 0.5,
        bounds.y0 - 0.5,
        bounds.x1 + 0.5,
        bounds.y1 + 0.5,
    );

    let glyph_w = padded.width().ceil() as u32;
    let glyph_h = padded.height().ceil() as u32;

    if glyph_w == 0 || glyph_h == 0 {
        return empty_icon(size);
    }

    // Rasterize the glyph at its natural size.
    let glyph_w_u16 = glyph_w as u16;
    let glyph_h_u16 = glyph_h as u16;
    let transform = Affine::translate((-padded.x0, -padded.y0));

    let mut ctx = RenderContext::new(glyph_w_u16, glyph_h_u16);
    ctx.set_paint(vello_cpu::peniko::color::palette::css::WHITE);
    ctx.set_transform(transform);
    ctx.fill_path(&path);
    ctx.flush();

    let mut pixmap = Pixmap::new(glyph_w_u16, glyph_h_u16);
    ctx.render_to_pixmap(&mut pixmap);
    let glyph_pixels = pixmap.data_as_u8_slice();

    // Compose the glyph onto a centered square canvas (flipped vertically for Y-up).
    let canvas_size = size as usize;
    let mut rgba = vec![0u8; canvas_size * canvas_size * 4];

    // Center the glyph within the square.
    let offset_x = (size.saturating_sub(glyph_w)) / 2;
    let offset_y = (size.saturating_sub(glyph_h)) / 2;

    for gy in 0..glyph_h as usize {
        // Flip Y: top row of glyph maps to bottom of canvas region
        let cy = (glyph_h as usize - 1 - gy) + offset_y as usize;
        if cy >= canvas_size {
            continue;
        }
        for gx in 0..glyph_w as usize {
            let cx = gx + offset_x as usize;
            if cx >= canvas_size {
                break;
            }

            let src_idx = (gy * glyph_w as usize + gx) * 4;
            let alpha = glyph_pixels[src_idx + 3];

            // Skip near-transparent pixels to avoid edge leaking
            if alpha < 4 {
                continue;
            }

            let dst_idx = (cy * canvas_size + cx) * 4;
            rgba[dst_idx] = alpha;
            rgba[dst_idx + 1] = alpha;
            rgba[dst_idx + 2] = alpha;
            rgba[dst_idx + 3] = alpha;
        }
    }

    RasterizedIcon {
        pixels: rgba,
        width: size,
        height: size,
    }
}

/// Create an empty transparent icon of the given size.
fn empty_icon(size: u32) -> RasterizedIcon {
    RasterizedIcon {
        pixels: vec![0u8; (size * size * 4) as usize],
        width: size,
        height: size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rasterize_lightbulb() {
        let icon = rasterize_icon(BillboardIcon::Lightbulb, 64);
        assert_eq!(icon.width, 64);
        assert_eq!(icon.height, 64);
        assert_eq!(icon.pixels.len(), 64 * 64 * 4);

        // The icon should have some non-transparent pixels.
        let has_pixels = icon.pixels.chunks(4).any(|px| px[3] > 0);
        assert!(has_pixels, "Lightbulb icon should have visible pixels");
    }

    #[test]
    fn test_rasterize_fire() {
        let icon = rasterize_icon(BillboardIcon::Fire, 64);
        assert_eq!(icon.width, 64);
        assert_eq!(icon.height, 64);
        assert_eq!(icon.pixels.len(), 64 * 64 * 4);

        let has_pixels = icon.pixels.chunks(4).any(|px| px[3] > 0);
        assert!(has_pixels, "Fire icon should have visible pixels");
    }

    #[test]
    fn test_rasterize_different_sizes() {
        for size in [32, 64, 128] {
            let icon = rasterize_icon(BillboardIcon::Lightbulb, size);
            assert_eq!(icon.width, size);
            assert_eq!(icon.height, size);
            assert_eq!(icon.pixels.len(), (size * size * 4) as usize);
        }
    }

    #[test]
    fn test_icon_not_clipped() {
        let icon = rasterize_icon(BillboardIcon::Lightbulb, 64);
        // Check that the edge rows/columns of the canvas are mostly transparent
        // (the icon is centered and shouldn't touch the canvas edge at typical sizes).
        let mut edge_alpha = 0u32;
        for y in 0..64 {
            for x in 0..64 {
                if y == 0 || y == 63 || x == 0 || x == 63 {
                    let idx = (y * 64 + x) * 4 + 3;
                    edge_alpha += icon.pixels[idx] as u32;
                }
            }
        }
        // At 64px, the icon glyph is much smaller than the canvas, so edges should be 0.
        assert_eq!(
            edge_alpha, 0,
            "Icon edges should be fully transparent at 64px"
        );
    }
}
