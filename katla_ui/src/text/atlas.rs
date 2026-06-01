use super::*;

use katla_math::{Rect2D, Vec2};

use crate::text::rasterization::RasterizedGlyph;

impl super::FontSystem {
    /// Place a rasterized glyph in the texture atlas using etagere shelf packing.
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
        let glyph_w = glyph.width as i32 + (padding * 2) as i32;
        let glyph_h = glyph.height as i32 + (padding * 2) as i32;

        let allocation = self
            .atlas_allocator
            .allocate(etagere::size2(glyph_w, glyph_h));

        let allocation = match allocation {
            Some(alloc) => alloc,
            None => {
                if !self.grow_atlas() {
                    log::warn!(
                        "Font atlas full at max size! Glyph '{}' ({}x{}) doesn't fit.",
                        glyph.c,
                        glyph_w,
                        glyph_h
                    );
                    return None;
                }
                match self
                    .atlas_allocator
                    .allocate(etagere::size2(glyph_w, glyph_h))
                {
                    Some(alloc) => alloc,
                    None => {
                        log::warn!(
                            "Font atlas allocation failed after growth for glyph '{}' ({}x{})",
                            glyph.c,
                            glyph_w,
                            glyph_h
                        );
                        return None;
                    }
                }
            }
        };

        let rect = allocation.rectangle;
        let x = rect.min.x as u32 + padding;
        let y = rect.min.y as u32 + padding;

        for (gy, row) in glyph.pixels.chunks(glyph.width).enumerate() {
            for (gx, &alpha) in row.iter().enumerate() {
                let px = x as usize + gx;
                let py = y as usize + gy;
                let idx = py * self.atlas_width as usize + px;

                if idx < self.atlas_data.len() {
                    self.atlas_data[idx] = alpha;
                }
            }
        }

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
    /// Doubles width first (up to MAX_ATLAS_WIDTH), then doubles height.
    /// After growth, the glyph cache is cleared and all glyphs will be re-rasterized on demand.
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
        self.atlas_data = vec![0u8; pixel_count];
        self.atlas_width = new_width;
        self.atlas_height = new_height;
        self.atlas_dirty = true;
        self.atlas_resized = true;

        self.atlas_allocator = etagere::BucketedAtlasAllocator::new(etagere::size2(
            new_width as i32,
            new_height as i32,
        ));

        self.glyph_cache.clear();

        true
    }

    /// Clear the glyph cache and atlas.
    pub fn clear_cache(&mut self) {
        self.glyph_cache.clear();
        self.atlas_data.fill(0);
        self.atlas_allocator.clear();
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

    /// Get atlas pixel data (R8 alpha-only).
    #[inline]
    pub fn atlas_data(&self) -> &[u8] {
        &self.atlas_data
    }

    /// Get atlas pixel data expanded to RGBA for GPU upload.
    ///
    /// Converts R8 alpha-only data to RGBA by setting RGB to 255 (white)
    /// and using the alpha channel for coverage. This maintains compatibility
    /// with the current RGBA GPU texture format.
    pub fn atlas_data_rgba(&self) -> Vec<u8> {
        let pixel_count = (self.atlas_width * self.atlas_height) as usize;
        let mut rgba = Vec::with_capacity(pixel_count * 4);
        for &alpha in &self.atlas_data {
            rgba.push(255);
            rgba.push(255);
            rgba.push(255);
            rgba.push(alpha);
        }
        rgba
    }
}
