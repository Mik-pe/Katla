use super::*;

use katla_math::{Rect2D, Vec2};

use crate::text::rasterization::RasterizedGlyph;

impl super::FontSystem {
    /// Place a rasterized glyph in the texture atlas.
    ///
    /// # Arguments
    /// * `glyph` - The rasterized glyph (width/height in physical pixels, metrics in logical pixels)
    /// * `scale_factor` - DPI scale factor for converting physical size to logical
    pub(super) fn place_in_atlas(
        &mut self,
        glyph: &RasterizedGlyph,
        scale_factor: f32,
    ) -> Option<CachedGlyph> {
        if glyph.width == 0 || glyph.height == 0 {
            return Some(CachedGlyph {
                uv_rect: Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0)),
                size: Vec2::new(0.0, 0.0),
                offset_x: glyph.offset_x,
                top_offset: glyph.top_offset,
                ascender: glyph.ascender,
                advance: glyph.advance,
            });
        }

        let padding = self.glyph_padding;
        let glyph_w = glyph.width as u32 + padding * 2;
        let glyph_h = glyph.height as u32 + padding * 2;

        if self.atlas_cursor_x + glyph_w > self.atlas_width {
            self.atlas_cursor_x = 0;
            self.atlas_cursor_y += self.atlas_row_height;
            self.atlas_row_height = 0;
        }

        if self.atlas_cursor_y + glyph_h > self.atlas_height && !self.grow_atlas() {
            log::warn!(
                "Font atlas full at max size! Glyph '{}' ({}x{}) doesn't fit.",
                glyph.c,
                glyph_w,
                glyph_h
            );
            return None;
        }

        let x = self.atlas_cursor_x + padding;
        let y = self.atlas_cursor_y + padding;

        for (gy, row) in glyph.pixels.chunks(glyph.width).enumerate() {
            for (gx, &alpha) in row.iter().enumerate() {
                let px = x as usize + gx;
                let py = y as usize + gy;
                let idx = (py * self.atlas_width as usize + px) * 4;

                if idx + 3 < self.atlas_data.len() {
                    self.atlas_data[idx] = 255;
                    self.atlas_data[idx + 1] = 255;
                    self.atlas_data[idx + 2] = 255;
                    self.atlas_data[idx + 3] = alpha;
                }
            }
        }

        self.atlas_cursor_x += glyph_w;
        self.atlas_row_height = self.atlas_row_height.max(glyph_h);
        self.atlas_dirty = true;

        let uv_min_x = x as f32 / self.atlas_width as f32;
        let uv_min_y = y as f32 / self.atlas_height as f32;
        let uv_max_x = (x as usize + glyph.width) as f32 / self.atlas_width as f32;
        let uv_max_y = (y as usize + glyph.height) as f32 / self.atlas_height as f32;

        let logical_width = glyph.width as f32 / scale_factor;
        let logical_height = glyph.height as f32 / scale_factor;

        Some(CachedGlyph {
            uv_rect: Rect2D::new(Vec2::new(uv_min_x, uv_min_y), Vec2::new(uv_max_x, uv_max_y)),
            size: Vec2::new(logical_width, logical_height),
            offset_x: glyph.offset_x,
            top_offset: glyph.top_offset,
            ascender: glyph.ascender,
            advance: glyph.advance,
        })
    }

    /// Grow the atlas to accommodate more glyphs.
    ///
    /// Uses egui-style strategy:
    /// 1. First grow width to MAX_ATLAS_WIDTH (8192)
    /// 2. Then grow height as needed
    ///
    /// Returns true if the atlas was grown, false if already at max size.
    pub(super) fn grow_atlas(&mut self) -> bool {
        let (new_width, new_height) = if self.atlas_width < Self::MAX_ATLAS_WIDTH {
            let new_width = (self.atlas_width * 2).min(Self::MAX_ATLAS_WIDTH);
            (new_width, self.atlas_height)
        } else if self.atlas_height < Self::MAX_ATLAS_HEIGHT {
            let new_height = (self.atlas_height * 2).min(Self::MAX_ATLAS_HEIGHT);
            (self.atlas_width, new_height)
        } else {
            log::warn!(
                "Font atlas at maximum size {}x{}, cannot grow further",
                self.atlas_width,
                self.atlas_height
            );
            return false;
        };

        log::info!(
            "Growing font atlas from {}x{} to {}x{}",
            self.atlas_width,
            self.atlas_height,
            new_width,
            new_height
        );

        let pixel_count = (new_width * new_height) as usize;
        let mut new_data = vec![255u8; pixel_count * 4];
        for i in 0..pixel_count {
            new_data[i * 4 + 3] = 0;
        }

        self.atlas_data = new_data;
        self.atlas_width = new_width;
        self.atlas_height = new_height;
        self.atlas_dirty = true;
        self.atlas_resized = true;

        self.glyph_cache.clear();

        self.atlas_cursor_x = 0;
        self.atlas_cursor_y = 0;
        self.atlas_row_height = 0;

        true
    }

    /// Clear the glyph cache and atlas.
    pub fn clear_cache(&mut self) {
        self.glyph_cache.clear();
        self.atlas_cursor_x = 0;
        self.atlas_cursor_y = 0;
        self.atlas_row_height = 0;
        self.atlas_data.fill(0);
    }

    /// Check if the atlas needs to be uploaded to GPU.
    #[inline]
    pub fn atlas_needs_update(&self) -> bool {
        self.atlas_dirty
    }

    /// Mark atlas as updated after GPU upload.
    #[inline]
    pub fn mark_atlas_updated(&mut self) {
        self.atlas_dirty = false;
    }

    /// Check if the atlas was resized (requires texture recreation).
    #[inline]
    pub fn atlas_was_resized(&self) -> bool {
        self.atlas_resized
    }

    /// Clear the resized flag after recreating the texture.
    #[inline]
    pub fn clear_atlas_resized(&mut self) {
        self.atlas_resized = false;
    }

    /// Get atlas dimensions.
    #[inline]
    pub fn atlas_size(&self) -> (u32, u32) {
        (self.atlas_width, self.atlas_height)
    }

    /// Get atlas pixel data (RGBA).
    #[inline]
    pub fn atlas_data(&self) -> &[u8] {
        &self.atlas_data
    }
}
