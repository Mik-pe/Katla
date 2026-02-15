//! Text rendering and font handling.
//!
//! This module provides font loading, glyph caching, and text rendering
//! using the `fontdue` library for rasterization.

use katla_math::{Rect2D, Vec2};
use std::collections::HashMap;

pub use fontdue::Font;

/// A handle to a loaded font.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontId(pub u32);

impl FontId {
    /// Default/built-in font ID.
    pub const DEFAULT: FontId = FontId(0);
}

/// Font size stored as fixed-point for hashing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FontSizeKey(u32);

impl FontSizeKey {
    fn from_f32(size: f32) -> Self {
        // Store as 16.16 fixed point for hashing
        FontSizeKey((size * 65536.0) as u32)
    }
}

/// A cached glyph's render data.
#[derive(Debug, Clone)]
pub struct CachedGlyph {
    /// UV rectangle in the texture atlas (normalized 0-1).
    pub uv_rect: Rect2D,
    /// Size of the glyph in pixels.
    pub size: Vec2,
    /// Offset from the baseline to the glyph's top-left.
    pub offset: Vec2,
    /// Horizontal advance to the next character.
    pub advance: f32,
}

/// A glyph ready for placement in the atlas.
#[derive(Debug, Clone)]
struct RasterizedGlyph {
    /// The character.
    pub c: char,
    /// Font size used.
    pub size: f32,
    /// Pixel data (8-bit alpha only).
    pub pixels: Vec<u8>,
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// Horizontal offset.
    pub offset_x: f32,
    /// Vertical offset from baseline.
    pub offset_y: f32,
    /// Horizontal advance.
    pub advance: f32,
}

/// Font system managing fonts, glyph cache, and texture atlas.
pub struct FontSystem {
    /// Loaded fonts.
    fonts: HashMap<FontId, Font>,
    /// Next font ID.
    next_font_id: u32,
    /// Glyph cache: (font_id, char, size_key) -> cached glyph.
    glyph_cache: HashMap<(FontId, char, FontSizeKey), CachedGlyph>,
    /// Texture atlas width.
    atlas_width: u32,
    /// Texture atlas height.
    atlas_height: u32,
    /// Current atlas cursor X.
    atlas_cursor_x: u32,
    /// Current atlas cursor Y.
    atlas_cursor_y: u32,
    /// Height of current row in atlas.
    atlas_row_height: u32,
    /// Atlas pixel data (RGBA).
    atlas_data: Vec<u8>,
    /// Whether atlas needs rebuild.
    atlas_dirty: bool,
    /// Padding around glyphs in atlas.
    glyph_padding: u32,
}

impl FontSystem {
    /// Default atlas size.
    const DEFAULT_ATLAS_SIZE: u32 = 512;

    /// Create a new font system.
    pub fn new() -> Self {
        Self {
            fonts: HashMap::new(),
            next_font_id: 0,
            glyph_cache: HashMap::new(),
            atlas_width: Self::DEFAULT_ATLAS_SIZE,
            atlas_height: Self::DEFAULT_ATLAS_SIZE,
            atlas_cursor_x: 0,
            atlas_cursor_y: 0,
            atlas_row_height: 0,
            atlas_data: vec![0; (Self::DEFAULT_ATLAS_SIZE * Self::DEFAULT_ATLAS_SIZE * 4) as usize],
            atlas_dirty: false,
            glyph_padding: 1,
        }
    }

    /// Create a font system with a custom atlas size.
    pub fn with_atlas_size(width: u32, height: u32) -> Self {
        Self {
            atlas_width: width,
            atlas_height: height,
            atlas_data: vec![0; (width * height * 4) as usize],
            ..Self::new()
        }
    }

    /// Add a font from bytes (TTF/OTF data).
    ///
    /// Returns the font ID for use with text rendering.
    pub fn add_font(&mut self, bytes: &[u8]) -> Result<FontId, FontError> {
        let font = Font::from_bytes(bytes, fontdue::FontSettings::default())
            .map_err(|e| FontError::LoadFailed(format!("{:?}", e)))?;

        let id = FontId(self.next_font_id);
        self.next_font_id += 1;
        self.fonts.insert(id, font);

        Ok(id)
    }

    /// Add a font from bytes with a specific ID.
    pub fn add_font_with_id(&mut self, bytes: &[u8], id: FontId) -> Result<(), FontError> {
        let font = Font::from_bytes(bytes, fontdue::FontSettings::default())
            .map_err(|e| FontError::LoadFailed(format!("{:?}", e)))?;

        self.fonts.insert(id, font);
        Ok(())
    }

    /// Get a font by ID.
    pub fn get_font(&self, id: FontId) -> Option<&Font> {
        self.fonts.get(&id)
    }

    /// Rasterize a glyph and add to the atlas.
    ///
    /// Returns the cached glyph info if successful.
    pub fn get_or_rasterize(&mut self, font_id: FontId, c: char, size: f32) -> Option<CachedGlyph> {
        let size_key = FontSizeKey::from_f32(size);

        // Check cache first
        if let Some(cached) = self.glyph_cache.get(&(font_id, c, size_key)) {
            return Some(cached.clone());
        }

        let font = self.fonts.get(&font_id)?;

        // Rasterize the glyph
        let (metrics, pixels) = font.rasterize(c, size);

        let rasterized = RasterizedGlyph {
            c,
            size,
            pixels,
            width: metrics.width,
            height: metrics.height,
            offset_x: metrics.bounds.xmin as f32,
            offset_y: metrics.bounds.ymin as f32,
            advance: metrics.advance_width,
        };

        // Place in atlas
        let cached = self.place_in_atlas(&rasterized)?;

        // Cache the result
        self.glyph_cache.insert((font_id, c, size_key), cached.clone());

        Some(cached)
    }

    /// Place a rasterized glyph in the texture atlas.
    fn place_in_atlas(&mut self, glyph: &RasterizedGlyph) -> Option<CachedGlyph> {
        // Handle empty glyphs (like spaces) - they don't need atlas space
        if glyph.width == 0 || glyph.height == 0 {
            return Some(CachedGlyph {
                uv_rect: Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0)),
                size: Vec2::new(0.0, 0.0),
                offset: Vec2::new(glyph.offset_x, glyph.offset_y),
                advance: glyph.advance,
            });
        }

        let padding = self.glyph_padding;
        let glyph_w = glyph.width as u32 + padding * 2;
        let glyph_h = glyph.height as u32 + padding * 2;

        // Check if we need a new row
        if self.atlas_cursor_x + glyph_w > self.atlas_width {
            self.atlas_cursor_x = 0;
            self.atlas_cursor_y += self.atlas_row_height;
            self.atlas_row_height = 0;
        }

        // Check if we have space
        if self.atlas_cursor_y + glyph_h > self.atlas_height {
            // Atlas is full - need to resize or evict
            // For now, just fail
            log::warn!(
                "Font atlas full! Glyph '{}' ({}x{}) doesn't fit.",
                glyph.c,
                glyph_w,
                glyph_h
            );
            return None;
        }

        let x = self.atlas_cursor_x + padding;
        let y = self.atlas_cursor_y + padding;

        // Copy glyph pixels to atlas (as RGBA with white color)
        for (gy, row) in glyph.pixels.chunks(glyph.width).enumerate() {
            for (gx, &alpha) in row.iter().enumerate() {
                let px = x as usize + gx;
                let py = y as usize + gy;
                let idx = (py * self.atlas_width as usize + px) * 4;

                if idx + 3 < self.atlas_data.len() {
                    // White glyph with alpha
                    self.atlas_data[idx] = 255;
                    self.atlas_data[idx + 1] = 255;
                    self.atlas_data[idx + 2] = 255;
                    self.atlas_data[idx + 3] = alpha;
                }
            }
        }

        // Advance cursor
        self.atlas_cursor_x += glyph_w;
        self.atlas_row_height = self.atlas_row_height.max(glyph_h);
        self.atlas_dirty = true;

        // Calculate UV coordinates (normalized)
        let uv_min_x = x as f32 / self.atlas_width as f32;
        let uv_min_y = y as f32 / self.atlas_height as f32;
        let uv_max_x = (x as usize + glyph.width) as f32 / self.atlas_width as f32;
        let uv_max_y = (y as usize + glyph.height) as f32 / self.atlas_height as f32;

        Some(CachedGlyph {
            uv_rect: Rect2D::new(
                Vec2::new(uv_min_x, uv_min_y),
                Vec2::new(uv_max_x, uv_max_y),
            ),
            size: Vec2::new(glyph.width as f32, glyph.height as f32),
            offset: Vec2::new(glyph.offset_x, glyph.offset_y),
            advance: glyph.advance,
        })
    }

    /// Pre-cache common ASCII characters for a font.
    pub fn precache_ascii(&mut self, font_id: FontId, size: f32) {
        // ASCII printable range
        for c in ' '..='~' {
            self.get_or_rasterize(font_id, c, size);
        }
    }

    /// Measure text dimensions without rendering.
    pub fn measure_text(&self, font_id: FontId, text: &str, size: f32) -> Vec2 {
        let font = match self.fonts.get(&font_id) {
            Some(f) => f,
            None => return Vec2::new(0.0, 0.0),
        };

        let size_key = FontSizeKey::from_f32(size);
        let mut width = 0.0f32;
        let mut max_height = 0.0f32;

        for c in text.chars() {
            // Check cache first
            if let Some(cached) = self.glyph_cache.get(&(font_id, c, size_key)) {
                width += cached.advance;
                max_height = max_height.max(cached.size.y());
            } else {
                // Use font metrics directly
                let metrics = font.metrics(c, size);
                width += metrics.advance_width;
                max_height = max_height.max(metrics.height as f32);
            }
        }

        Vec2::new(width, max_height)
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

    /// Clear the glyph cache and atlas.
    pub fn clear_cache(&mut self) {
        self.glyph_cache.clear();
        self.atlas_cursor_x = 0;
        self.atlas_cursor_y = 0;
        self.atlas_row_height = 0;
        self.atlas_data.fill(0);
        self.atlas_dirty = true;
    }
}

impl Default for FontSystem {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors that can occur with fonts.
#[derive(Debug, Clone)]
pub enum FontError {
    /// Failed to load font.
    LoadFailed(String),
    /// Font not found.
    NotFound(FontId),
    /// Glyph not available.
    GlyphMissing(char),
    /// Atlas is full.
    AtlasFull,
}

impl std::fmt::Display for FontError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FontError::LoadFailed(msg) => write!(f, "Failed to load font: {}", msg),
            FontError::NotFound(id) => write!(f, "Font not found: {:?}", id),
            FontError::GlyphMissing(c) => write!(f, "Glyph not available: '{}'", c),
            FontError::AtlasFull => write!(f, "Font atlas is full"),
        }
    }
}

impl std::error::Error for FontError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_font_system_creation() {
        let sys = FontSystem::new();
        assert_eq!(sys.atlas_width, FontSystem::DEFAULT_ATLAS_SIZE);
        assert!(!sys.atlas_dirty);
    }

    #[test]
    fn test_font_id_default() {
        assert_eq!(FontId::DEFAULT, FontId(0));
    }
}
