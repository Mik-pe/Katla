//! Text rendering and font handling.
//!
//! This module provides font loading, glyph caching, and text rendering
//! using the `ab_glyph` library for rasterization.

use ab_glyph::{Font, FontRef, Glyph, PxScale, ScaleFont};
use katla_math::{Rect2D, Vec2};
use std::collections::HashMap;

/// A handle to a loaded font.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontId(pub u32);

impl FontId {
    /// Default/built-in font ID.
    pub const DEFAULT: FontId = FontId(0);
    /// Icon font ID (for ForkAwesome or similar icon fonts).
    pub const ICON: FontId = FontId(1);
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

/// Scale factor stored as fixed-point for hashing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ScaleFactorKey(u32);

impl ScaleFactorKey {
    fn from_f32(scale: f32) -> Self {
        // Store as 8.24 fixed point for hashing
        ScaleFactorKey((scale * 16777216.0) as u32)
    }
}

/// A cached glyph's render data.
#[derive(Debug, Clone)]
pub struct CachedGlyph {
    /// UV rectangle in the texture atlas (normalized 0-1).
    pub uv_rect: Rect2D,
    /// Size of the glyph in pixels.
    pub size: Vec2,
    /// Horizontal offset from cursor position to glyph's left edge (left side bearing).
    pub offset_x: f32,
    /// Distance from baseline to top of glyph bitmap in screen coords (y-down).
    /// This is positive and represents how far up from baseline the top of the glyph is.
    pub top_offset: f32,
    /// Font ascender for baseline alignment.
    pub ascender: f32,
    /// Horizontal advance to the next character.
    pub advance: f32,
}

/// A glyph ready for placement in the atlas.
#[derive(Debug, Clone)]
struct RasterizedGlyph {
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

/// Font system managing fonts, glyph cache, and texture atlas.
pub struct FontSystem {
    /// Loaded fonts.
    fonts: HashMap<FontId, FontRef<'static>>,
    /// Next font ID.
    next_font_id: u32,
    /// Glyph cache: (font_id, char, size_key, scale_key) -> cached glyph.
    glyph_cache: HashMap<(FontId, char, FontSizeKey, ScaleFactorKey), CachedGlyph>,
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
        let mut atlas_data = vec![0; (Self::DEFAULT_ATLAS_SIZE * Self::DEFAULT_ATLAS_SIZE * 4) as usize];

        // Reserve first pixel as white for solid color rendering
        // UV (0,0) will sample this white pixel, so vertex color passes through
        atlas_data[0] = 255; // R
        atlas_data[1] = 255; // G
        atlas_data[2] = 255; // B
        atlas_data[3] = 255; // A

        Self {
            fonts: HashMap::new(),
            next_font_id: 0,
            glyph_cache: HashMap::new(),
            atlas_width: Self::DEFAULT_ATLAS_SIZE,
            atlas_height: Self::DEFAULT_ATLAS_SIZE,
            atlas_cursor_x: 1, // Start after white pixel
            atlas_cursor_y: 0,
            atlas_row_height: 0,
            atlas_data,
            atlas_dirty: true, // Mark dirty so renderer uploads the white pixel
            glyph_padding: 1,
        }
    }

    /// Create a font system with a custom atlas size.
    pub fn with_atlas_size(width: u32, height: u32) -> Self {
        let mut atlas_data = vec![0; (width * height * 4) as usize];

        // Reserve first pixel as white for solid color rendering
        atlas_data[0] = 255;
        atlas_data[1] = 255;
        atlas_data[2] = 255;
        atlas_data[3] = 255;

        Self {
            atlas_width: width,
            atlas_height: height,
            atlas_data,
            atlas_cursor_x: 1, // Start after white pixel
            atlas_dirty: true,
            ..Self::new()
        }
    }

    /// Add a font from bytes (TTF/OTF data).
    ///
    /// Returns the font ID for use with text rendering.
    pub fn add_font(&mut self, bytes: &[u8]) -> Result<FontId, FontError> {
        // FontRef doesn't own the bytes, so we need to leak them for 'static lifetime.
        // This is safe because fonts are typically loaded once and live for the
        // duration of the application.
        let bytes: &'static [u8] = Box::leak(bytes.to_vec().into_boxed_slice());

        let font = FontRef::try_from_slice(bytes)
            .map_err(|e| FontError::LoadFailed(format!("{:?}", e)))?;

        let id = FontId(self.next_font_id);
        self.next_font_id += 1;
        self.fonts.insert(id, font);

        Ok(id)
    }

    /// Add a font from bytes with a specific ID.
    pub fn add_font_with_id(&mut self, bytes: &[u8], id: FontId) -> Result<(), FontError> {
        let bytes: &'static [u8] = Box::leak(bytes.to_vec().into_boxed_slice());

        let font = FontRef::try_from_slice(bytes)
            .map_err(|e| FontError::LoadFailed(format!("{:?}", e)))?;

        self.fonts.insert(id, font);
        Ok(())
    }

    /// Get a font by ID.
    pub fn get_font(&self, id: FontId) -> Option<&FontRef<'static>> {
        self.fonts.get(&id)
    }

    /// Rasterize a glyph and add to the atlas.
    ///
    /// Returns the cached glyph info if successful.
    ///
    /// # Arguments
    /// * `font_id` - The font to use
    /// * `c` - The character to rasterize
    /// * `logical_size` - Font size in logical pixels
    /// * `scale_factor` - DPI scale factor (physical pixels per logical pixel)
    pub fn get_or_rasterize(
        &mut self,
        font_id: FontId,
        c: char,
        logical_size: f32,
        scale_factor: f32,
    ) -> Option<CachedGlyph> {
        let size_key = FontSizeKey::from_f32(logical_size);
        let scale_key = ScaleFactorKey::from_f32(scale_factor);

        // Check cache first
        if let Some(cached) = self.glyph_cache.get(&(font_id, c, size_key, scale_key)) {
            return Some(cached.clone());
        }

        let font = self.fonts.get(&font_id)?;

        // Calculate physical pixel size for rasterization
        let physical_size = logical_size * scale_factor;

        // Rasterize the glyph using ab_glyph at physical resolution
        let scaled_font = font.as_scaled(PxScale::from(physical_size));

        // Get glyph ID for character
        let glyph_id = font.glyph_id(c);

        // Get font metrics for baseline alignment (physical pixels)
        let ascender = scaled_font.ascent();

        // Calculate advance width (physical pixels)
        let advance = scaled_font.h_advance(glyph_id);

        // Get outline glyph (may be None for whitespace, control chars)
        let glyph = Glyph {
            id: glyph_id,
            scale: PxScale::from(physical_size),
            position: ab_glyph::point(0.0, 0.0),
        };

        let outlined = match font.outline_glyph(glyph) {
            Some(o) => o,
            None => {
                // No outline (whitespace, control chars) - still cache with advance width
                let cached = CachedGlyph {
                    uv_rect: Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0)),
                    size: Vec2::new(0.0, 0.0),
                    offset_x: 0.0,
                    top_offset: 0.0,
                    ascender: ascender / scale_factor,
                    advance: advance / scale_factor,
                };
                self.glyph_cache.insert((font_id, c, size_key, scale_key), cached.clone());
                return Some(cached);
            }
        };

        // Get pixel bounds (in physical pixels)
        let bounds = outlined.px_bounds();

        // Allocate pixel buffer
        let width = bounds.width().ceil() as usize;
        let height = bounds.height().ceil() as usize;

        // Handle empty glyph bounds (shouldn't happen if outline exists, but be safe)
        if width == 0 || height == 0 {
            let cached = CachedGlyph {
                uv_rect: Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0)),
                size: Vec2::new(0.0, 0.0),
                offset_x: 0.0,
                top_offset: 0.0,
                ascender: ascender / scale_factor,
                advance: advance / scale_factor,
            };
            self.glyph_cache.insert((font_id, c, size_key, scale_key), cached.clone());
            return Some(cached);
        }

        // Draw glyph to pixel buffer
        let mut pixels = vec![0u8; width * height];
        outlined.draw(|x, y, coverage| {
            let px = x as usize;
            let py = y as usize;
            if px < width && py < height {
                pixels[py * width + px] = (coverage * 255.0) as u8;
            }
        });

        // Convert physical pixel metrics to logical pixels for UI positioning
        //
        // ab_glyph coordinate system (y-UP):
        // - Glyph position (0,0) is at baseline
        // - bounds.min.y is the TOP of the glyph relative to baseline
        // - For glyphs extending above baseline (like 'A'), bounds.min.y is NEGATIVE
        //
        // Screen coordinate system (y-DOWN):
        // - top_offset = distance from baseline UP to top of glyph
        // - Since bounds.min.y is negative when above baseline: top_offset = -bounds.min.y
        let top_offset = -bounds.min.y / scale_factor;

        let rasterized = RasterizedGlyph {
            c,
            pixels,
            width,
            height,
            // ab_glyph's bounds.min.x is the left edge offset from cursor (left side bearing)
            offset_x: bounds.min.x / scale_factor,
            // Distance from baseline to top of glyph (positive in screen y-down coords)
            top_offset,
            ascender: ascender / scale_factor,
            advance: advance / scale_factor,
        };

        // Place in atlas (uses physical pixels for crisp rendering)
        let cached = self.place_in_atlas(&rasterized, scale_factor)?;

        // Cache the result
        self.glyph_cache.insert((font_id, c, size_key, scale_key), cached.clone());

        Some(cached)
    }

    /// Place a rasterized glyph in the texture atlas.
    ///
    /// # Arguments
    /// * `glyph` - The rasterized glyph (width/height in physical pixels, metrics in logical pixels)
    /// * `scale_factor` - DPI scale factor for converting physical size to logical
    fn place_in_atlas(&mut self, glyph: &RasterizedGlyph, scale_factor: f32) -> Option<CachedGlyph> {
        // Handle empty glyphs (like spaces) - they don't need atlas space
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

        // Calculate UV coordinates (normalized - scale-independent)
        let uv_min_x = x as f32 / self.atlas_width as f32;
        let uv_min_y = y as f32 / self.atlas_height as f32;
        let uv_max_x = (x as usize + glyph.width) as f32 / self.atlas_width as f32;
        let uv_max_y = (y as usize + glyph.height) as f32 / self.atlas_height as f32;

        // Convert physical pixel size to logical pixels for UI positioning
        let logical_width = glyph.width as f32 / scale_factor;
        let logical_height = glyph.height as f32 / scale_factor;

        Some(CachedGlyph {
            uv_rect: Rect2D::new(
                Vec2::new(uv_min_x, uv_min_y),
                Vec2::new(uv_max_x, uv_max_y),
            ),
            size: Vec2::new(logical_width, logical_height),
            offset_x: glyph.offset_x,
            top_offset: glyph.top_offset,
            ascender: glyph.ascender,
            advance: glyph.advance,
        })
    }

    /// Pre-cache common ASCII characters for a font.
    pub fn precache_ascii(&mut self, font_id: FontId, size: f32, scale_factor: f32) {
        // ASCII printable range
        for c in ' '..='~' {
            self.get_or_rasterize(font_id, c, size, scale_factor);
        }
    }

    /// Pre-cache common icons for an icon font.
    ///
    /// This rasterizes frequently used icons at the given size to avoid
    /// runtime hitches when rendering icons for the first time.
    pub fn precache_icons(&mut self, font_id: FontId, size: f32, scale_factor: f32, icons: &[char]) {
        for &icon in icons {
            self.get_or_rasterize(font_id, icon, size, scale_factor);
        }
    }

    /// Get font metrics for a given size.
    ///
    /// Returns (ascent, descent, line_gap) in logical pixels.
    /// These values are needed for proper text baseline positioning.
    pub fn get_font_metrics(&self, font_id: FontId, size: f32, scale_factor: f32) -> Option<(f32, f32, f32)> {
        let font = self.fonts.get(&font_id)?;
        let physical_size = size * scale_factor;
        let scaled_font = font.as_scaled(PxScale::from(physical_size));

        // Convert from physical to logical pixels
        Some((
            scaled_font.ascent() / scale_factor,
            scaled_font.descent() / scale_factor,
            scaled_font.line_gap() / scale_factor,
        ))
    }

    /// Measure text dimensions without rendering.
    ///
    /// Returns dimensions in logical pixels.
    pub fn measure_text(&self, font_id: FontId, text: &str, size: f32, scale_factor: f32) -> Vec2 {
        let font = match self.fonts.get(&font_id) {
            Some(f) => f,
            None => return Vec2::new(0.0, 0.0),
        };

        let size_key = FontSizeKey::from_f32(size);
        let scale_key = ScaleFactorKey::from_f32(scale_factor);

        // Use physical size for font scaling, then convert back to logical
        let physical_size = size * scale_factor;
        let scaled_font = font.as_scaled(PxScale::from(physical_size));

        let mut width = 0.0f32;
        let mut max_height = 0.0f32;

        for c in text.chars() {
            // Check cache first (metrics are stored in logical pixels)
            if let Some(cached) = self.glyph_cache.get(&(font_id, c, size_key, scale_key)) {
                width += cached.advance;
                max_height = max_height.max(cached.size.y());
            } else {
                // Use font metrics directly (convert physical to logical)
                let glyph_id = font.glyph_id(c);
                width += scaled_font.h_advance(glyph_id) / scale_factor;
                max_height = max_height.max(scaled_font.height() / scale_factor);
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
        self.atlas_cursor_x = 1; // Start after white pixel
        self.atlas_cursor_y = 0;
        self.atlas_row_height = 0;
        self.atlas_data.fill(0);

        // Restore white pixel at (0,0) for solid color rendering
        self.atlas_data[0] = 255;
        self.atlas_data[1] = 255;
        self.atlas_data[2] = 255;
        self.atlas_data[3] = 255;

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
        // Atlas is dirty initially because white pixel at (0,0) needs upload
        assert!(sys.atlas_dirty);
    }

    #[test]
    fn test_font_id_default() {
        assert_eq!(FontId::DEFAULT, FontId(0));
    }
}
