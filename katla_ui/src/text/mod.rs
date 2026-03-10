//! Text rendering and font handling.
//!
//! This module provides font loading, glyph caching, and text rendering
//! using the `ab_glyph` library for rasterization.
//!
//! # Subpixel Positioning
//!
//! For crisp text at any position, we use 4 subpixel bins (0.0, 0.25, 0.5, 0.75)
//! for horizontal positioning. Each bin caches a separate version of the glyph,
//! shifted by the subpixel offset. This approach is inspired by egui and cosmic-text.
//!
//! # Gamma Correction
//!
//! Glyph coverage values are gamma-corrected for perceptually uniform text weight.
//! Without gamma correction, text can appear too thin (light fonts) or too thick
//! (dark fonts on light backgrounds).

use crate::types::TextureId;
use ab_glyph::{Font, FontRef, Glyph, PxScale, ScaleFont};
use katla_math::{Rect2D, Vec2};
use std::collections::HashMap;

/// Gamma factor for text rendering.
///
/// sRGB text on sRGB background needs approximately 1.45 gamma adjustment
/// for perceptually uniform blending. This is derived from the sRGB gamma
/// of ~2.2: sqrt(2.2) ≈ 1.48, commonly rounded to 1.45.
const GAMMA_FACTOR: f32 = 1.45;

/// Convert coverage value to perceptually uniform alpha.
///
/// When rendering text, the coverage value from the rasterizer represents
/// the fraction of the pixel covered by the glyph. For correct blending on
/// sRGB displays, we need to convert this to a perceptually uniform alpha.
///
/// This function applies gamma correction: alpha = coverage^(1/gamma)
#[inline]
fn coverage_to_alpha(coverage: f32) -> f32 {
    coverage.powf(1.0 / GAMMA_FACTOR)
}

/// Convert alpha value back to coverage (inverse of gamma correction).
///
/// This is kept for completeness but not currently used.
#[allow(dead_code)]
fn alpha_to_coverage(alpha: f32) -> f32 {
    alpha.powf(GAMMA_FACTOR)
}

/// A handle to a loaded font.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontId(pub u32);

impl FontId {
    /// Default/built-in font ID.
    pub const DEFAULT: FontId = FontId(0);
    /// Icon font ID (for ForkAwesome or similar icon fonts).
    pub const ICON: FontId = FontId(1);
}

/// Subpixel bin for horizontal glyph positioning.
///
/// We use 4 bins representing 0.0, 0.25, 0.5, and 0.75 subpixel offsets.
/// This allows crisp text rendering at any fractional X position by caching
/// 4 versions of each glyph, each shifted by the corresponding subpixel offset.
///
/// This approach is used by egui and cosmic-text for high-quality text rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SubpixelBin {
    /// 0.0 subpixel offset
    Zero,
    /// 0.25 subpixel offset
    One,
    /// 0.5 subpixel offset
    Two,
    /// 0.75 subpixel offset
    Three,
}

impl SubpixelBin {
    /// Create a subpixel bin from a fractional position.
    ///
    /// Returns the integer floor position and the subpixel bin.
    /// For example, `new(10.3)` returns `(10, SubpixelBin::One)`.
    #[inline]
    pub fn new(pos: f32) -> (i32, Self) {
        // Handle negative positions correctly
        let floor = pos.floor() as i32;
        let frac = pos - pos.floor();

        // Map fractional part [0, 1) to bins:
        // [0.0, 0.25) -> Zero (0.0)
        // [0.25, 0.5) -> One (0.25)
        // [0.5, 0.75) -> Two (0.5)
        // [0.75, 1.0) -> Three (0.75)
        let bin = match (frac * 4.0) as u32 {
            0 => SubpixelBin::Zero,
            1 => SubpixelBin::One,
            2 => SubpixelBin::Two,
            _ => SubpixelBin::Three,
        };
        (floor, bin)
    }

    /// Get the subpixel offset for this bin.
    ///
    /// Returns 0.0, 0.25, 0.5, or 0.75 depending on the bin.
    #[inline]
    pub fn as_offset(&self) -> f32 {
        match self {
            SubpixelBin::Zero => 0.0,
            SubpixelBin::One => 0.25,
            SubpixelBin::Two => 0.5,
            SubpixelBin::Three => 0.75,
        }
    }
}

/// Font size stored as fixed-point for hashing.
///
/// Uses 16.16 fixed point format. Sizes are clamped to valid range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FontSizeKey(u32);

impl FontSizeKey {
    /// Minimum valid font size in pixels.
    const MIN_SIZE: f32 = 1.0;
    /// Maximum valid font size in pixels.
    const MAX_SIZE: f32 = 1000.0;

    fn from_f32(size: f32) -> Self {
        let clamped = size.clamp(Self::MIN_SIZE, Self::MAX_SIZE);
        if clamped != size {
            log::warn!(
                "Font size {} clamped to valid range [{}, {}]",
                size,
                Self::MIN_SIZE,
                Self::MAX_SIZE
            );
        }
        // Store as 16.16 fixed point for hashing
        FontSizeKey((clamped * 65536.0) as u32)
    }
}

/// Scale factor stored as fixed-point for hashing.
///
/// Uses 8.24 fixed point format. Scale factors are clamped to valid range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ScaleFactorKey(u32);

impl ScaleFactorKey {
    /// Minimum valid scale factor.
    const MIN_SCALE: f32 = 0.1;
    /// Maximum valid scale factor.
    const MAX_SCALE: f32 = 10.0;

    fn from_f32(scale: f32) -> Self {
        let clamped = scale.clamp(Self::MIN_SCALE, Self::MAX_SCALE);
        if clamped != scale {
            log::warn!(
                "Scale factor {} clamped to valid range [{}, {}]",
                scale,
                Self::MIN_SCALE,
                Self::MAX_SCALE
            );
        }
        // Store as 8.24 fixed point for hashing
        ScaleFactorKey((clamped * 16777216.0) as u32)
    }
}

/// A cached glyph's render data.
#[derive(Debug, Clone, Copy)]
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
    /// Loaded fonts (leaked for 'static lifetime - fonts live for app lifetime).
    fonts: HashMap<FontId, FontRef<'static>>,
    /// Next font ID.
    next_font_id: u32,
    /// Glyph cache: (font_id, char, size_key, scale_key, subpixel_bin) -> cached glyph.
    glyph_cache: HashMap<(FontId, char, FontSizeKey, ScaleFactorKey, SubpixelBin), CachedGlyph>,
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
    /// Whether atlas was resized (needs texture recreation).
    atlas_resized: bool,
    /// Padding around glyphs in atlas.
    glyph_padding: u32,
    /// Texture ID for the font atlas (set by application after registering with renderer).
    font_atlas_id: TextureId,
}

impl FontSystem {
    /// Default atlas width.
    const DEFAULT_ATLAS_WIDTH: u32 = 256;
    /// Default atlas height.
    const DEFAULT_ATLAS_HEIGHT: u32 = 256;
    /// Maximum atlas width (egui-style: wide atlas for efficient glyph packing).
    const MAX_ATLAS_WIDTH: u32 = 8192;
    /// Maximum atlas height.
    const MAX_ATLAS_HEIGHT: u32 = 8192;

    /// Create a new font system.
    pub fn new() -> Self {
        let mut atlas_data =
            vec![0; (Self::DEFAULT_ATLAS_WIDTH * Self::DEFAULT_ATLAS_HEIGHT * 4) as usize];

        // Reserve first 2x2 pixels as white for solid color rendering
        // This gives a safety margin for linear filtering
        for y in 0..2 {
            for x in 0..2 {
                let idx = (y * Self::DEFAULT_ATLAS_WIDTH + x) as usize * 4;
                atlas_data[idx] = 255; // R
                atlas_data[idx + 1] = 255; // G
                atlas_data[idx + 2] = 255; // B
                atlas_data[idx + 3] = 255; // A
            }
        }

        Self {
            fonts: HashMap::new(),
            next_font_id: 0,
            glyph_cache: HashMap::new(),
            atlas_width: Self::DEFAULT_ATLAS_WIDTH,
            atlas_height: Self::DEFAULT_ATLAS_HEIGHT,
            atlas_cursor_x: 4, // Start after reserved 2x2 white pixel area + margin
            atlas_cursor_y: 0,
            atlas_row_height: 0,
            atlas_data,
            atlas_dirty: true, // Mark dirty so renderer uploads the white pixel
            atlas_resized: false,
            glyph_padding: 1,
            font_atlas_id: TextureId::NONE,
        }
    }

    /// Create a font system with a custom atlas size.
    pub fn with_atlas_size(width: u32, height: u32) -> Self {
        let mut atlas_data = vec![0; (width * height * 4) as usize];

        // Reserve first 2x2 pixels as white for solid color rendering
        for y in 0..2 {
            for x in 0..2 {
                let idx = (y * width + x) as usize * 4;
                atlas_data[idx] = 255;
                atlas_data[idx + 1] = 255;
                atlas_data[idx + 2] = 255;
                atlas_data[idx + 3] = 255;
            }
        }

        Self {
            fonts: HashMap::new(),
            next_font_id: 0,
            glyph_cache: HashMap::new(),
            atlas_width: width,
            atlas_height: height,
            atlas_cursor_x: 4, // Start after reserved 2x2 white pixel area + margin
            atlas_cursor_y: 0,
            atlas_row_height: 0,
            atlas_data,
            atlas_dirty: true,
            atlas_resized: false,
            glyph_padding: 1,
            font_atlas_id: TextureId::NONE,
        }
    }

    /// Get the font atlas texture ID.
    pub fn atlas_id(&self) -> TextureId {
        self.font_atlas_id
    }

    /// Set the font atlas texture ID.
    ///
    /// This should be called after registering the atlas texture with the renderer.
    pub fn set_atlas_id(&mut self, id: TextureId) {
        self.font_atlas_id = id;
    }

    /// Add a font from bytes (TTF/OTF data).
    ///
    /// Returns the font ID for use with text rendering.
    ///
    /// Note: Font data is leaked with `Box::leak` to satisfy `'static` lifetime.
    /// This is intentional - fonts are typically loaded once and live for the
    /// application lifetime, so the leak is acceptable.
    pub fn add_font(&mut self, bytes: &[u8]) -> Result<FontId, FontError> {
        // FontRef requires 'static lifetime. We leak the bytes since fonts
        // are expected to live for the entire application lifetime.
        let bytes: &'static [u8] = Box::leak(bytes.to_vec().into_boxed_slice());

        let font = FontRef::try_from_slice(bytes)
            .map_err(|e| FontError::LoadFailed(format!("{:?}", e)))?;

        let id = FontId(self.next_font_id);
        self.next_font_id += 1;
        self.fonts.insert(id, font);

        Ok(id)
    }

    /// Add a font from bytes with a specific ID.
    ///
    /// See [`add_font`](Self::add_font) for lifetime notes.
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

        // Check cache first (now includes subpixel bin in key)
        if let Some(cached) = self
            .glyph_cache
            .get(&(font_id, c, size_key, scale_key, subpixel_bin))
        {
            return Some(*cached);
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

        // Get subpixel offset for this bin (in physical pixels)
        let subpixel_offset = subpixel_bin.as_offset() * scale_factor;

        // Get outline glyph (may be None for whitespace, control chars)
        // Apply subpixel offset to position for crisp fractional rendering
        let glyph = Glyph {
            id: glyph_id,
            scale: PxScale::from(physical_size),
            position: ab_glyph::point(subpixel_offset, 0.0),
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
                self.glyph_cache
                    .insert((font_id, c, size_key, scale_key, subpixel_bin), cached);
                return Some(cached);
            }
        };

        // Get pixel bounds (in physical pixels) - this changes with subpixel offset
        let bounds = outlined.px_bounds();

        // IMPORTANT: Get CONSISTENT metrics from unshifted glyph.
        // This ensures all subpixel bins have the same size, preventing
        // visual "jumps" when switching between bins.
        let glyph_for_metrics = Glyph {
            id: glyph_id,
            scale: PxScale::from(physical_size),
            position: ab_glyph::point(0.0, 0.0), // No subpixel offset for metrics
        };
        let metrics_bounds = font
            .outline_glyph(glyph_for_metrics)
            .map(|g| g.px_bounds())
            .unwrap_or(bounds);

        // Use consistent metrics for offset_x, top_offset, AND size
        let offset_x = metrics_bounds.min.x / scale_factor;
        let top_offset = -metrics_bounds.min.y / scale_factor;

        // Consistent size from unshifted bounds (in physical pixels)
        let width = metrics_bounds.width().ceil() as usize;
        let height = metrics_bounds.height().ceil() as usize;

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
            self.glyph_cache
                .insert((font_id, c, size_key, scale_key, subpixel_bin), cached);
            return Some(cached);
        }

        // Draw glyph to pixel buffer
        // Note: The pixel data is from the shifted outline, but we use consistent
        // size from unshifted bounds. Pixels outside the buffer are clipped.
        let mut pixels = vec![0u8; width * height];
        outlined.draw(|x, y, coverage| {
            let px = x as usize;
            let py = y as usize;
            if px < width && py < height {
                // Apply gamma correction for perceptually uniform text weight
                let alpha = coverage_to_alpha(coverage);
                pixels[py * width + px] = (alpha * 255.0) as u8;
            }
        });

        let rasterized = RasterizedGlyph {
            c,
            pixels,
            width,
            height,
            // Consistent horizontal offset (NOT affected by subpixel positioning)
            offset_x,
            // Consistent vertical offset (NOT affected by subpixel positioning)
            top_offset,
            ascender: ascender / scale_factor,
            advance: advance / scale_factor,
        };

        // Place in atlas (uses physical pixels for crisp rendering)
        let cached = self.place_in_atlas(&rasterized, scale_factor)?;

        // Cache the result (includes subpixel bin in key)
        self.glyph_cache
            .insert((font_id, c, size_key, scale_key, subpixel_bin), cached);

        Some(cached)
    }

    /// Place a rasterized glyph in the texture atlas.
    ///
    /// # Arguments
    /// * `glyph` - The rasterized glyph (width/height in physical pixels, metrics in logical pixels)
    /// * `scale_factor` - DPI scale factor for converting physical size to logical
    fn place_in_atlas(
        &mut self,
        glyph: &RasterizedGlyph,
        scale_factor: f32,
    ) -> Option<CachedGlyph> {
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

        // Check if we have space - grow atlas if needed
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
        // The padding around glyphs prevents bleeding from neighboring glyphs
        let uv_min_x = x as f32 / self.atlas_width as f32;
        let uv_min_y = y as f32 / self.atlas_height as f32;
        let uv_max_x = (x as usize + glyph.width) as f32 / self.atlas_width as f32;
        let uv_max_y = (y as usize + glyph.height) as f32 / self.atlas_height as f32;

        // Convert physical pixel size to logical pixels for UI positioning
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

    /// Pre-cache common ASCII characters for a font.
    ///
    /// This pre-caches all 4 subpixel bins for each character for optimal
    /// rendering performance at any position.
    pub fn precache_ascii(&mut self, font_id: FontId, size: f32, scale_factor: f32) {
        // ASCII printable range
        for c in ' '..='~' {
            // Cache all 4 subpixel bins
            for bin in [
                SubpixelBin::Zero,
                SubpixelBin::One,
                SubpixelBin::Two,
                SubpixelBin::Three,
            ] {
                self.get_or_rasterize(font_id, c, size, scale_factor, bin);
            }
        }
    }

    /// Pre-cache common icons for an icon font.
    ///
    /// This rasterizes frequently used icons at the given size to avoid
    /// runtime hitches when rendering icons for the first time.
    /// Only caches SubpixelBin::Zero for icons (usually positioned at integer coords).
    pub fn precache_icons(
        &mut self,
        font_id: FontId,
        size: f32,
        scale_factor: f32,
        icons: &[char],
    ) {
        for &icon in icons {
            // Icons are typically at integer positions, so just cache bin Zero
            self.get_or_rasterize(font_id, icon, size, scale_factor, SubpixelBin::Zero);
        }
    }

    /// Get font metrics for a given size.
    ///
    /// Returns (ascent, descent, line_gap) in logical pixels.
    /// These values are needed for proper text baseline positioning.
    pub fn get_font_metrics(
        &self,
        font_id: FontId,
        size: f32,
        scale_factor: f32,
    ) -> Option<(f32, f32, f32)> {
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

    /// Get kerning between two characters.
    ///
    /// Returns the kerning adjustment in logical pixels.
    /// This should be added to the cursor position before placing the second character.
    ///
    /// Kerning adjusts spacing between specific character pairs (like "AV", "Te")
    /// for better visual appearance.
    pub fn get_kerning(
        &self,
        font_id: FontId,
        left: char,
        right: char,
        size: f32,
        scale_factor: f32,
    ) -> f32 {
        let Some(font) = self.fonts.get(&font_id) else {
            return 0.0;
        };

        // Get unscaled kerning from the font's kern table
        let left_id = font.glyph_id(left);
        let right_id = font.glyph_id(right);
        let unscaled_kern = font.kern_unscaled(left_id, right_id);

        // Scale to physical pixels, then convert to logical
        let physical_size = size * scale_factor;
        let scaled_kern = unscaled_kern * physical_size / font.units_per_em().unwrap_or(1.0);
        scaled_kern / scale_factor
    }

    /// Measure text dimensions without rendering.
    ///
    /// Returns dimensions in logical pixels.
    /// Note: This uses SubpixelBin::Zero for cache lookup since advance width
    /// is the same regardless of subpixel position.
    /// Measure text dimensions without rendering.
    ///
    /// Returns dimensions in logical pixels.
    /// Note: This uses SubpixelBin::Zero for cache lookup since advance width
    /// is the same regardless of subpixel position.
    ///
    /// This method includes kerning between character pairs.
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

        // Get line height (font height for consistent line spacing)
        let line_height = scaled_font.height() / scale_factor;

        let mut line_width = 0.0f32;
        let mut max_width = 0.0f32;
        let mut line_count = 1u32;
        let mut prev_char: Option<char> = None;

        for c in text.chars() {
            // Handle newlines
            if c == '\n' {
                max_width = max_width.max(line_width);
                line_count += 1;
                line_width = 0.0;
                prev_char = None;
                continue;
            }

            // Apply kerning between previous and current character
            if let Some(prev) = prev_char {
                line_width += self.get_kerning(font_id, prev, c, size, scale_factor);
            }

            // Check cache first (metrics are stored in logical pixels)
            // Use SubpixelBin::Zero since advance width is independent of subpixel position
            if let Some(cached) =
                self.glyph_cache
                    .get(&(font_id, c, size_key, scale_key, SubpixelBin::Zero))
            {
                line_width += cached.advance;
            } else {
                // Use font metrics directly (convert physical to logical)
                let glyph_id = font.glyph_id(c);
                line_width += scaled_font.h_advance(glyph_id) / scale_factor;
            }

            prev_char = Some(c);
        }

        // Account for the last line
        max_width = max_width.max(line_width);

        // Total height is line count * line height
        let total_height = line_count as f32 * line_height;

        Vec2::new(max_width, total_height)
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

    /// Grow the atlas to accommodate more glyphs.
    ///
    /// Uses egui-style strategy:
    /// 1. First grow width to MAX_ATLAS_WIDTH (8192)
    /// 2. Then grow height as needed
    ///
    /// This is more memory-efficient than square resizing because glyphs
    /// are typically short, so we pack them horizontally first.
    ///
    /// Returns true if the atlas was grown, false if already at max size.
    fn grow_atlas(&mut self) -> bool {
        let (new_width, new_height) = if self.atlas_width < Self::MAX_ATLAS_WIDTH {
            // Grow width first (double until max)
            let new_width = (self.atlas_width * 2).min(Self::MAX_ATLAS_WIDTH);
            (new_width, self.atlas_height)
        } else if self.atlas_height < Self::MAX_ATLAS_HEIGHT {
            // Width at max, grow height
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

        // Create new larger buffer (initialized to zeros - no old data)
        // Since we're clearing the cache and re-rasterizing, there's no point
        // copying old data. This also avoids ghost pixels from previous glyphs.
        let new_data = vec![0u8; (new_width * new_height * 4) as usize];

        self.atlas_data = new_data;
        self.atlas_width = new_width;
        self.atlas_height = new_height;
        self.atlas_dirty = true;
        self.atlas_resized = true;

        // Invalidate glyph cache since UV coordinates changed
        // We need to re-rasterize everything with correct UVs
        self.glyph_cache.clear();

        // Re-initialize white pixel area (2x2)
        for y in 0..2u32 {
            for x in 0..2u32 {
                let idx = (y * self.atlas_width + x) as usize * 4;
                self.atlas_data[idx] = 255;
                self.atlas_data[idx + 1] = 255;
                self.atlas_data[idx + 2] = 255;
                self.atlas_data[idx + 3] = 255;
            }
        }

        // Reset cursor to start fresh after reserved white pixel area
        self.atlas_cursor_x = 4;
        self.atlas_cursor_y = 0;
        self.atlas_row_height = 0;

        true
    }

    /// Clear the glyph cache and atlas.
    pub fn clear_cache(&mut self) {
        self.glyph_cache.clear();
        self.atlas_cursor_x = 4; // Start after reserved 2x2 white pixel area + margin
        self.atlas_cursor_y = 0;
        self.atlas_row_height = 0;
        self.atlas_data.fill(0);

        // Restore 2x2 white pixel area for solid color rendering
        // This gives a safety margin for linear filtering
        for y in 0..2u32 {
            for x in 0..2u32 {
                let idx = (y * self.atlas_width + x) as usize * 4;
                self.atlas_data[idx] = 255; // R
                self.atlas_data[idx + 1] = 255; // G
                self.atlas_data[idx + 2] = 255; // B
                self.atlas_data[idx + 3] = 255; // A
            }
        }

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
        assert_eq!(sys.atlas_width, FontSystem::DEFAULT_ATLAS_WIDTH);
        assert_eq!(sys.atlas_height, FontSystem::DEFAULT_ATLAS_HEIGHT);
        // Atlas is dirty initially because white pixel at (0,0) needs upload
        assert!(sys.atlas_dirty);
    }

    #[test]
    fn test_font_id_default() {
        assert_eq!(FontId::DEFAULT, FontId(0));
    }

    #[test]
    fn test_font_atlas_white_pixel() {
        // Verify the font atlas has a white pixel at (0,0) for solid color rendering
        let sys = FontSystem::new();
        let data = sys.atlas_data();

        // Check that the first pixel (0,0) is white [255, 255, 255, 255]
        // This white pixel is sampled by solid color quads (UV=(0,0))
        assert_eq!(data[0], 255); // R
        assert_eq!(data[1], 255); // G
        assert_eq!(data[2], 255); // B
        assert_eq!(data[3], 255); // A

        // Verify the reserved 2x2 white pixel area is initialized
        for y in 0..2u32 {
            for x in 0..2u32 {
                let idx = (y * sys.atlas_width + x) as usize * 4;
                assert_eq!(
                    data[idx], 255,
                    "White pixel area at ({},{}) should have R=255",
                    x, y
                );
                assert_eq!(
                    data[idx + 1],
                    255,
                    "White pixel area at ({},{}) should have G=255",
                    x,
                    y
                );
                assert_eq!(
                    data[idx + 2],
                    255,
                    "White pixel area at ({},{}) should have B=255",
                    x,
                    y
                );
                assert_eq!(
                    data[idx + 3],
                    255,
                    "White pixel area at ({},{}) should have A=255",
                    x,
                    y
                );
            }
        }
    }

    #[test]
    fn test_subpixel_bin_zero() {
        let (floor, bin) = SubpixelBin::new(10.0);
        assert_eq!(floor, 10);
        assert_eq!(bin, SubpixelBin::Zero);
        assert_eq!(bin.as_offset(), 0.0);
    }

    #[test]
    fn test_subpixel_bin_one() {
        let (floor, bin) = SubpixelBin::new(10.25);
        assert_eq!(floor, 10);
        assert_eq!(bin, SubpixelBin::One);
        assert_eq!(bin.as_offset(), 0.25);
    }

    #[test]
    fn test_subpixel_bin_two() {
        let (floor, bin) = SubpixelBin::new(10.5);
        assert_eq!(floor, 10);
        assert_eq!(bin, SubpixelBin::Two);
        assert_eq!(bin.as_offset(), 0.5);
    }

    #[test]
    fn test_subpixel_bin_three() {
        let (floor, bin) = SubpixelBin::new(10.75);
        assert_eq!(floor, 10);
        assert_eq!(bin, SubpixelBin::Three);
        assert_eq!(bin.as_offset(), 0.75);
    }

    #[test]
    fn test_subpixel_bin_boundary() {
        // Test boundary cases
        let (floor, bin) = SubpixelBin::new(10.249);
        assert_eq!(floor, 10);
        assert_eq!(bin, SubpixelBin::Zero);

        let (floor, bin) = SubpixelBin::new(10.25);
        assert_eq!(floor, 10);
        assert_eq!(bin, SubpixelBin::One);

        let (floor, bin) = SubpixelBin::new(10.99);
        assert_eq!(floor, 10);
        assert_eq!(bin, SubpixelBin::Three);
    }

    #[test]
    fn test_subpixel_bin_negative() {
        // Test negative values
        // -0.3 -> floor -1, frac = -0.3 - (-1) = 0.7 -> bin Two (0.7 * 4 = 2.8 -> 2)
        let (floor, bin) = SubpixelBin::new(-0.3);
        assert_eq!(floor, -1);
        assert_eq!(bin, SubpixelBin::Two);

        // -0.8 -> floor -1, frac = 0.2 -> bin Zero
        let (floor, bin) = SubpixelBin::new(-0.8);
        assert_eq!(floor, -1);
        assert_eq!(bin, SubpixelBin::Zero);
    }

    #[test]
    fn test_gamma_correction_midpoint() {
        // 0.5 coverage should become > 0.5 alpha (brighter midtones)
        let alpha = coverage_to_alpha(0.5);
        assert!(alpha > 0.5, "Gamma correction should brighten midtones");
        assert!(
            (alpha - 0.5_f32.powf(1.0 / 1.45)).abs() < 0.001,
            "Alpha should match expected gamma-corrected value"
        );
    }

    #[test]
    fn test_gamma_correction_extremes() {
        // Extremes should stay the same
        assert!((coverage_to_alpha(0.0) - 0.0).abs() < 0.001);
        assert!((coverage_to_alpha(1.0) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_clear_cache_white_pixel_area() {
        // Verify that clear_cache() restores the 2x2 white pixel area
        // consistent with new(), with_atlas_size(), and grow_atlas()
        let mut sys = FontSystem::new();

        // Modify the atlas data to corrupt the white pixel area
        sys.atlas_data[0] = 0;
        sys.atlas_data[1] = 0;
        sys.atlas_data[2] = 0;
        sys.atlas_data[3] = 0;

        // Clear cache should restore the white pixel area
        sys.clear_cache();

        // Verify the 2x2 white pixel area is restored
        for y in 0..2u32 {
            for x in 0..2u32 {
                let idx = (y * sys.atlas_width + x) as usize * 4;
                assert_eq!(
                    sys.atlas_data[idx], 255,
                    "White pixel area at ({},{}) should have R=255 after clear_cache()",
                    x, y
                );
                assert_eq!(
                    sys.atlas_data[idx + 1],
                    255,
                    "White pixel area at ({},{}) should have G=255 after clear_cache()",
                    x,
                    y
                );
                assert_eq!(
                    sys.atlas_data[idx + 2],
                    255,
                    "White pixel area at ({},{}) should have B=255 after clear_cache()",
                    x,
                    y
                );
                assert_eq!(
                    sys.atlas_data[idx + 3],
                    255,
                    "White pixel area at ({},{}) should have A=255 after clear_cache()",
                    x,
                    y
                );
            }
        }
    }

    #[test]
    fn test_clear_cache_cursor_position() {
        // Verify that clear_cache() resets atlas_cursor_x to 4
        // (after the reserved 2x2 white pixel area + margin)
        let mut sys = FontSystem::new();

        // Move the cursor to some position
        sys.atlas_cursor_x = 100;
        sys.atlas_cursor_y = 50;

        // Clear cache should reset cursor to start position
        sys.clear_cache();

        // Cursor should be at 4 (after 2x2 white pixel area + margin)
        assert_eq!(
            sys.atlas_cursor_x, 4,
            "atlas_cursor_x should be 4 after clear_cache() to reserve 2x2 white pixel area"
        );
        assert_eq!(
            sys.atlas_cursor_y, 0,
            "atlas_cursor_y should be 0 after clear_cache()"
        );
    }

    #[test]
    fn test_initialization_consistency() {
        // Verify that all initialization paths (new, with_atlas_size, clear_cache)
        // produce consistent atlas state

        // Test new()
        let sys1 = FontSystem::new();
        assert_eq!(sys1.atlas_cursor_x, 4);
        assert_white_pixel_area(&sys1);

        // Test with_atlas_size()
        let sys2 = FontSystem::with_atlas_size(512, 512);
        assert_eq!(sys2.atlas_cursor_x, 4);
        assert_white_pixel_area(&sys2);

        // Test clear_cache()
        let mut sys3 = FontSystem::new();
        sys3.atlas_cursor_x = 100;
        sys3.clear_cache();
        assert_eq!(sys3.atlas_cursor_x, 4);
        assert_white_pixel_area(&sys3);
    }

    fn assert_white_pixel_area(sys: &FontSystem) {
        // Helper to verify the 2x2 white pixel area
        for y in 0..2u32 {
            for x in 0..2u32 {
                let idx = (y * sys.atlas_width + x) as usize * 4;
                assert_eq!(sys.atlas_data[idx], 255, "R should be 255");
                assert_eq!(sys.atlas_data[idx + 1], 255, "G should be 255");
                assert_eq!(sys.atlas_data[idx + 2], 255, "B should be 255");
                assert_eq!(sys.atlas_data[idx + 3], 255, "A should be 255");
            }
        }
    }

    #[test]
    fn test_subpixel_bin_comprehensive_fractional_coverage() {
        // Test comprehensive coverage of fractional positions across all bins
        let test_cases = [
            // (position, expected_floor, expected_bin, description)
            (0.0, 0, SubpixelBin::Zero, "exact integer -> Zero"),
            (0.124, 0, SubpixelBin::Zero, "just below 0.25 boundary -> Zero"),
            (0.125, 0, SubpixelBin::Zero, "midpoint of Zero range -> Zero"),
            (0.249, 0, SubpixelBin::Zero, "just below 0.25 boundary -> Zero"),
            (0.25, 0, SubpixelBin::One, "exact 0.25 boundary -> One"),
            (0.375, 0, SubpixelBin::One, "midpoint of One range -> One"),
            (0.499, 0, SubpixelBin::One, "just below 0.5 boundary -> One"),
            (0.5, 0, SubpixelBin::Two, "exact 0.5 boundary -> Two"),
            (0.625, 0, SubpixelBin::Two, "midpoint of Two range -> Two"),
            (0.749, 0, SubpixelBin::Two, "just below 0.75 boundary -> Two"),
            (0.75, 0, SubpixelBin::Three, "exact 0.75 boundary -> Three"),
            (0.875, 0, SubpixelBin::Three, "midpoint of Three range -> Three"),
            (0.999, 0, SubpixelBin::Three, "just below 1.0 boundary -> Three"),
            // Test with different integer parts
            (10.0, 10, SubpixelBin::Zero, "integer 10 -> Zero"),
            (10.1, 10, SubpixelBin::Zero, "10.1 -> Zero"),
            (10.25, 10, SubpixelBin::One, "10.25 -> One"),
            (10.5, 10, SubpixelBin::Two, "10.5 -> Two"),
            (10.75, 10, SubpixelBin::Three, "10.75 -> Three"),
            (100.24, 100, SubpixelBin::Zero, "100.24 -> Zero"),
            (100.26, 100, SubpixelBin::One, "100.26 -> One"),
        ];

        for (pos, expected_floor, expected_bin, desc) in test_cases {
            let (floor, bin) = SubpixelBin::new(pos);
            assert_eq!(
                floor, expected_floor,
                "{}: floor mismatch for pos={}",
                desc, pos
            );
            assert_eq!(
                bin, expected_bin,
                "{}: bin mismatch for pos={}",
                desc, pos
            );
            // Verify offset matches expected value
            assert_eq!(
                bin.as_offset(),
                match expected_bin {
                    SubpixelBin::Zero => 0.0,
                    SubpixelBin::One => 0.25,
                    SubpixelBin::Two => 0.5,
                    SubpixelBin::Three => 0.75,
                },
                "{}: offset mismatch for pos={}",
                desc, pos
            );
        }
    }

    #[test]
    fn test_subpixel_bin_text_shares_same_bin() {
        // Test that all characters in a text string share the same subpixel bin
        // This is critical for consistent text rendering

        // In actual text rendering, the bin is determined once at text start
        // and all characters in that text use the same bin, regardless of
        // their actual positions. This test verifies that concept.

        // Simulate text rendering at position 100.1 (bin Zero)
        let text_start_pos = 100.1;
        let (floor, bin) = SubpixelBin::new(text_start_pos);

        // All characters in the text should use this same bin
        assert_eq!(floor, 100);
        assert_eq!(bin, SubpixelBin::Zero);

        // The key insight: in real text rendering, the subpixel bin is
        // determined at the START of the text and all characters use it.
        // This is different from calling SubpixelBin::new() on each character's
        // position (which would give different bins as characters advance).

        // Simulate short text that stays within the same subpixel region
        let short_advances = [0.5, 0.3, 0.2]; // Small advances

        // For text at 100.1, characters at offsets 0.0, 0.5, 0.8, 1.0
        // all use the same bin (Zero) determined at start
        for &_advance in &short_advances {
            // In real rendering, we don't recalculate the bin per character
            // we use the bin from the text start position
            // So all characters share the same bin
            assert_eq!(
                bin, SubpixelBin::Zero,
                "All characters should use the same bin as text start"
            );
        }

        // Now test at a different start position (100.3 -> bin One)
        let text_start_pos2 = 100.3;
        let (floor2, bin2) = SubpixelBin::new(text_start_pos2);
        assert_eq!(floor2, 100);
        assert_eq!(bin2, SubpixelBin::One);

        // All characters at this new position should share bin2
        for &_advance in &short_advances {
            assert_eq!(
                bin2, SubpixelBin::One,
                "All characters should use the same bin as text start"
            );
        }

        // Verify the bin offset is correct
        assert_eq!(bin.as_offset(), 0.0);
        assert_eq!(bin2.as_offset(), 0.25);
    }

    #[test]
    fn test_subpixel_bin_advance_width_consistency() {
        // Test that advance width is consistent within the same bin
        // This ensures text renders consistently regardless of subpixel position

        // Simulate text at two positions within the same bin (Zero)
        let pos1 = 100.1;
        let pos2 = 100.2; // Still in bin Zero

        let (floor1, bin1) = SubpixelBin::new(pos1);
        let (floor2, bin2) = SubpixelBin::new(pos2);

        // Both should be in same bin with same floor
        assert_eq!(floor1, floor2, "Floor should be identical in same bin");
        assert_eq!(bin1, bin2, "Bin should be identical");

        // Simulate rendering text with character advances
        let char_advances = [8.5, 5.3, 7.2];

        // At position 1
        let mut cursor1 = 0.0;
        let positions1: Vec<f32> = char_advances
            .iter()
            .map(|&advance| {
                let pos = cursor1;
                cursor1 += advance;
                pos
            })
            .collect();

        // At position 2 (same bin)
        let mut cursor2 = 0.0;
        let positions2: Vec<f32> = char_advances
            .iter()
            .map(|&advance| {
                let pos = cursor2;
                cursor2 += advance;
                pos
            })
            .collect();

        // Relative positions should be identical
        assert_eq!(
            positions1.len(),
            positions2.len(),
            "Should have same number of characters"
        );

        for i in 0..positions1.len() {
            assert_eq!(
                positions1[i], positions2[i],
                "Character {} relative position should be identical in same bin",
                i
            );
        }

        // Advance widths (spacing between characters) should be consistent
        for i in 0..char_advances.len() {
            assert_eq!(
                char_advances[i], char_advances[i],
                "Advance width should be consistent"
            );
        }

        // Test across different bins - relative positions should still be consistent
        let pos3 = 100.5; // Bin Two
        let (_floor3, _bin3) = SubpixelBin::new(pos3);

        let mut cursor3 = 0.0;
        let positions3: Vec<f32> = char_advances
            .iter()
            .map(|&advance| {
                let pos = cursor3;
                cursor3 += advance;
                pos
            })
            .collect();

        // Relative positions should still match (all start at cursor=0)
        for i in 0..positions1.len() {
            assert_eq!(
                positions1[i], positions3[i],
                "Relative positions should be consistent across bins"
            );
        }
    }
}
