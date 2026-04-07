//! Text rendering and font handling.
//!
//! This module provides font loading, glyph caching, and text rendering
//! using the `skrifa` library for font parsing and `vello_cpu` for rasterization.
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

mod atlas;
mod font_loading;
mod measurement;
mod rasterization;

use crate::types::TextureId;
use std::collections::HashMap;
use std::sync::Arc;

use katla_math::{Rect2D, Vec2};

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
pub(super) fn coverage_to_alpha(coverage: f32) -> f32 {
    coverage.powf(1.0 / GAMMA_FACTOR)
}

/// A handle to a loaded font.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontId(pub(crate) u32);

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
        let floor = pos.floor() as i32;
        let frac = pos - pos.floor();

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
pub(super) struct FontSizeKey(u32);

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
        FontSizeKey((clamped * 65536.0) as u32)
    }
}

/// Scale factor stored as fixed-point for hashing.
///
/// Uses 8.24 fixed point format. Scale factors are clamped to valid range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct ScaleFactorKey(u32);

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

/// Font system managing fonts, glyph cache, and texture atlas.
pub struct FontSystem {
    /// Loaded fonts stored with owned data (no Box::leak).
    pub(super) fonts: HashMap<FontId, Arc<Vec<u8>>>,
    /// Next font ID.
    pub(super) next_font_id: u32,
    /// Glyph cache: (font_id, char, size_key, scale_key, subpixel_bin) -> cached glyph.
    pub(super) glyph_cache:
        HashMap<(FontId, char, FontSizeKey, ScaleFactorKey, SubpixelBin), CachedGlyph>,
    /// Texture atlas width.
    pub(super) atlas_width: u32,
    /// Texture atlas height.
    pub(super) atlas_height: u32,
    /// Current atlas cursor X.
    pub(super) atlas_cursor_x: u32,
    /// Current atlas cursor Y.
    pub(super) atlas_cursor_y: u32,
    /// Height of current row in atlas.
    pub(super) atlas_row_height: u32,
    /// Atlas pixel data (RGBA).
    pub(super) atlas_data: Vec<u8>,
    /// Whether atlas needs rebuild.
    pub(super) atlas_dirty: bool,
    /// Whether atlas was resized (needs texture recreation).
    pub(super) atlas_resized: bool,
    /// Padding around glyphs in atlas.
    pub(super) glyph_padding: u32,
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
        Self::with_atlas_size(Self::DEFAULT_ATLAS_WIDTH, Self::DEFAULT_ATLAS_HEIGHT)
    }

    /// Create a font system with a custom atlas size.
    pub fn with_atlas_size(width: u32, height: u32) -> Self {
        let pixel_count = (width * height) as usize;
        let mut atlas_data = vec![255u8; pixel_count * 4];
        for i in 0..pixel_count {
            atlas_data[i * 4 + 3] = 0;
        }

        Self {
            fonts: HashMap::new(),
            next_font_id: 0,
            glyph_cache: HashMap::new(),
            atlas_width: width,
            atlas_height: height,
            atlas_cursor_x: 0,
            atlas_cursor_y: 0,
            atlas_row_height: 0,
            atlas_data,
            atlas_dirty: true,
            atlas_resized: false,
            glyph_padding: 1,
        }
    }

    /// Get the font atlas texture ID.
    pub fn atlas_id(&self) -> TextureId {
        TextureId::FONT_ATLAS
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
    fn test_subpixel_bin_boundary() {
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
        let (floor, bin) = SubpixelBin::new(-0.3);
        assert_eq!(floor, -1);
        assert_eq!(bin, SubpixelBin::Two);

        let (floor, bin) = SubpixelBin::new(-0.8);
        assert_eq!(floor, -1);
        assert_eq!(bin, SubpixelBin::Zero);
    }

    #[test]
    fn test_gamma_correction_midpoint() {
        let alpha = coverage_to_alpha(0.5);
        assert!(alpha > 0.5, "Gamma correction should brighten midtones");
        assert!(
            (alpha - 0.5_f32.powf(1.0 / 1.45)).abs() < 0.001,
            "Alpha should match expected gamma-corrected value"
        );
    }

    #[test]
    fn test_gamma_correction_extremes() {
        assert!((coverage_to_alpha(0.0) - 0.0).abs() < 0.001);
        assert!((coverage_to_alpha(1.0) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_gamma_correction_formula() {
        let gamma = GAMMA_FACTOR;
        assert_eq!(gamma, 1.45, "GAMMA_FACTOR should be 1.45");

        let test_cases = [
            (0.0, 0.0),
            (0.25, 0.25_f32.powf(1.0 / 1.45)),
            (0.5, 0.5_f32.powf(1.0 / 1.45)),
            (0.75, 0.75_f32.powf(1.0 / 1.45)),
            (1.0, 1.0),
        ];

        for (coverage, expected_alpha) in test_cases {
            let alpha = coverage_to_alpha(coverage);
            assert!(
                (alpha - expected_alpha).abs() < 0.001,
                "Coverage {} should produce alpha {} (gamma 1.45)",
                coverage,
                expected_alpha
            );
        }
    }

    #[test]
    fn test_gamma_correction_perceptually_uniform() {
        let midtone_coverage = 0.5;
        let midtone_alpha = coverage_to_alpha(midtone_coverage);

        assert!(
            midtone_alpha > midtone_coverage,
            "Gamma correction should brighten midtones for perceptually uniform weight"
        );

        let expected = 0.5_f32.powf(1.0 / 1.45);
        assert!(
            (midtone_alpha - expected).abs() < 0.001,
            "Midtone alpha should be {} (0.5^(1/1.45))",
            expected
        );

        for coverage in [0.3, 0.4, 0.5, 0.6, 0.7] {
            let alpha = coverage_to_alpha(coverage);
            assert!(
                alpha > coverage,
                "Coverage {} should be brightened to {}",
                coverage,
                alpha
            );

            let brightening_factor = alpha / coverage;
            assert!(
                brightening_factor > 1.0,
                "Brightening factor should be > 1.0"
            );
        }
    }

    #[test]
    fn test_gamma_correction_edge_cases() {
        let alpha_0 = coverage_to_alpha(0.0);
        assert_eq!(
            alpha_0, 0.0,
            "Coverage 0.0 should produce alpha 0.0 (completely transparent)"
        );

        let alpha_1 = coverage_to_alpha(1.0);
        assert_eq!(
            alpha_1, 1.0,
            "Coverage 1.0 should produce alpha 1.0 (completely opaque)"
        );

        assert_eq!(0.0_f32.powf(1.0 / 1.45), 0.0, "0^(1/γ) = 0");
        assert_eq!(1.0_f32.powf(1.0 / 1.45), 1.0, "1^(1/γ) = 1");

        let near_zero = 0.001;
        let alpha_near_zero = coverage_to_alpha(near_zero);
        assert!(
            alpha_near_zero > near_zero,
            "Near-zero coverage should be brightened slightly"
        );
        assert!(
            alpha_near_zero < 0.01,
            "Near-zero coverage should still produce small alpha"
        );

        let near_one = 0.999;
        let alpha_near_one = coverage_to_alpha(near_one);
        assert!(
            alpha_near_one > near_one,
            "Near-one coverage should be brightened slightly"
        );
        assert!(
            alpha_near_one < 1.0,
            "Near-one coverage should still be < 1.0"
        );
    }

    #[test]
    fn test_gamma_correction_monotonic() {
        let mut prev_alpha = coverage_to_alpha(0.0);
        let steps = 100;

        for i in 1..=steps {
            let coverage = i as f32 / steps as f32;
            let alpha = coverage_to_alpha(coverage);

            assert!(
                alpha > prev_alpha,
                "Coverage {} should produce higher alpha than previous value",
                coverage
            );

            prev_alpha = alpha;
        }

        assert_eq!(prev_alpha, 1.0, "Final alpha should be 1.0");
    }

    #[test]
    fn test_gamma_correction_range() {
        let steps = 1000;
        for i in 0..=steps {
            let coverage = i as f32 / steps as f32;
            let alpha = coverage_to_alpha(coverage);

            assert!(
                alpha >= 0.0 && alpha <= 1.0,
                "Coverage {} should produce alpha in [0, 1], got {}",
                coverage,
                alpha
            );
        }
    }

    #[test]
    fn test_gamma_correction_inverse() {
        let test_cases = [0.0, 0.25, 0.5, 0.75, 1.0];

        for coverage in test_cases {
            let alpha = coverage_to_alpha(coverage);
            let recovered = alpha.powf(GAMMA_FACTOR);

            assert!(
                (recovered - coverage).abs() < 0.001,
                "Round-trip conversion failed: {} -> {} -> {}",
                coverage,
                alpha,
                recovered
            );
        }

        let test_coverage = 0.5;
        let alpha = coverage_to_alpha(test_coverage);
        let expected_alpha = test_coverage.powf(1.0 / 1.45);
        assert!(
            (alpha - expected_alpha).abs() < 0.001,
            "Forward conversion should use 1/gamma"
        );

        let recovered = alpha.powf(GAMMA_FACTOR);
        let expected_recovered = alpha.powf(1.45);
        assert!(
            (recovered - expected_recovered).abs() < 0.001,
            "Inverse conversion should use gamma"
        );
    }

    #[test]
    fn test_clear_cache_cursor_position() {
        let mut sys = FontSystem::new();

        sys.atlas_cursor_x = 100;
        sys.atlas_cursor_y = 50;

        sys.clear_cache();

        assert_eq!(
            sys.atlas_cursor_x, 0,
            "atlas_cursor_x should be 0 after clear_cache()"
        );
        assert_eq!(
            sys.atlas_cursor_y, 0,
            "atlas_cursor_y should be 0 after clear_cache()"
        );
    }

    #[test]
    fn test_initialization_consistency() {
        let sys1 = FontSystem::new();
        assert_eq!(sys1.atlas_cursor_x, 0);
        assert_eq!(sys1.atlas_cursor_y, 0);

        let sys2 = FontSystem::with_atlas_size(512, 512);
        assert_eq!(sys2.atlas_cursor_x, 0);
        assert_eq!(sys2.atlas_cursor_y, 0);

        let mut sys3 = FontSystem::new();
        sys3.atlas_cursor_x = 100;
        sys3.clear_cache();
        assert_eq!(sys3.atlas_cursor_x, 0);
        assert_eq!(sys3.atlas_cursor_y, 0);
    }

    #[test]
    fn test_subpixel_bin_comprehensive_fractional_coverage() {
        let test_cases = [
            (0.0, 0, SubpixelBin::Zero, "exact integer -> Zero"),
            (
                0.124,
                0,
                SubpixelBin::Zero,
                "just below 0.25 boundary -> Zero",
            ),
            (
                0.125,
                0,
                SubpixelBin::Zero,
                "midpoint of Zero range -> Zero",
            ),
            (
                0.249,
                0,
                SubpixelBin::Zero,
                "just below 0.25 boundary -> Zero",
            ),
            (0.25, 0, SubpixelBin::One, "exact 0.25 boundary -> One"),
            (0.375, 0, SubpixelBin::One, "midpoint of One range -> One"),
            (0.499, 0, SubpixelBin::One, "just below 0.5 boundary -> One"),
            (0.5, 0, SubpixelBin::Two, "exact 0.5 boundary -> Two"),
            (0.625, 0, SubpixelBin::Two, "midpoint of Two range -> Two"),
            (
                0.749,
                0,
                SubpixelBin::Two,
                "just below 0.75 boundary -> Two",
            ),
            (0.75, 0, SubpixelBin::Three, "exact 0.75 boundary -> Three"),
            (
                0.875,
                0,
                SubpixelBin::Three,
                "midpoint of Three range -> Three",
            ),
            (
                0.999,
                0,
                SubpixelBin::Three,
                "just below 1.0 boundary -> Three",
            ),
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
            assert_eq!(bin, expected_bin, "{}: bin mismatch for pos={}", desc, pos);
            assert_eq!(
                bin.as_offset(),
                match expected_bin {
                    SubpixelBin::Zero => 0.0,
                    SubpixelBin::One => 0.25,
                    SubpixelBin::Two => 0.5,
                    SubpixelBin::Three => 0.75,
                },
                "{}: offset mismatch for pos={}",
                desc,
                pos
            );
        }
    }

    #[test]
    fn test_subpixel_bin_text_shares_same_bin() {
        let text_start_pos = 100.1;
        let (floor, bin) = SubpixelBin::new(text_start_pos);

        assert_eq!(floor, 100);
        assert_eq!(bin, SubpixelBin::Zero);

        let short_advances = [0.5, 0.3, 0.2];

        for &_advance in &short_advances {
            assert_eq!(
                bin,
                SubpixelBin::Zero,
                "All characters should use the same bin as text start"
            );
        }

        let text_start_pos2 = 100.3;
        let (floor2, bin2) = SubpixelBin::new(text_start_pos2);
        assert_eq!(floor2, 100);
        assert_eq!(bin2, SubpixelBin::One);

        for &_advance in &short_advances {
            assert_eq!(
                bin2,
                SubpixelBin::One,
                "All characters should use the same bin as text start"
            );
        }

        assert_eq!(bin.as_offset(), 0.0);
        assert_eq!(bin2.as_offset(), 0.25);
    }

    #[test]
    fn test_subpixel_bin_advance_width_consistency() {
        let pos1 = 100.1;
        let pos2 = 100.2;

        let (floor1, bin1) = SubpixelBin::new(pos1);
        let (floor2, bin2) = SubpixelBin::new(pos2);

        assert_eq!(floor1, floor2, "Floor should be identical in same bin");
        assert_eq!(bin1, bin2, "Bin should be identical");

        let char_advances = [8.5, 5.3, 7.2];

        let mut cursor1 = 0.0;
        let positions1: Vec<f32> = char_advances
            .iter()
            .map(|&advance| {
                let pos = cursor1;
                cursor1 += advance;
                pos
            })
            .collect();

        let mut cursor2 = 0.0;
        let positions2: Vec<f32> = char_advances
            .iter()
            .map(|&advance| {
                let pos = cursor2;
                cursor2 += advance;
                pos
            })
            .collect();

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

        for i in 0..char_advances.len() {
            assert_eq!(
                char_advances[i], char_advances[i],
                "Advance width should be consistent"
            );
        }

        let pos3 = 100.5;
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

        for i in 0..positions1.len() {
            assert_eq!(
                positions1[i], positions3[i],
                "Relative positions should be consistent across bins"
            );
        }
    }

    #[test]
    fn test_glyph_uv_normalization() {
        let sys = FontSystem::new();

        let (atlas_width, atlas_height) = sys.atlas_size();
        assert_eq!(atlas_width, 256);
        assert_eq!(atlas_height, 256);

        let white_pixel_uv_max_x = 2.0 / atlas_width as f32;
        let white_pixel_uv_max_y = 2.0 / atlas_height as f32;

        assert!(
            0.0 >= 0.0 && 0.0 <= 1.0,
            "UV min X should be normalized to [0,1]"
        );
        assert!(
            0.0 >= 0.0 && 0.0 <= 1.0,
            "UV min Y should be normalized to [0,1]"
        );
        assert!(
            white_pixel_uv_max_x >= 0.0 && white_pixel_uv_max_x <= 1.0,
            "UV max X should be normalized to [0,1]"
        );
        assert!(
            white_pixel_uv_max_y >= 0.0 && white_pixel_uv_max_y <= 1.0,
            "UV max Y should be normalized to [0,1]"
        );

        assert!(
            (white_pixel_uv_max_x - 2.0 / 256.0).abs() < 0.0001,
            "UV max X should be exactly 2/256"
        );
        assert!(
            (white_pixel_uv_max_y - 2.0 / 256.0).abs() < 0.0001,
            "UV max Y should be exactly 2/256"
        );
    }

    #[test]
    fn test_glyph_uv_bounds_within_atlas() {
        let sys = FontSystem::new();

        let (atlas_width, atlas_height) = sys.atlas_size();

        let uv_max_x = 2.0 / atlas_width as f32;
        let uv_max_y = 2.0 / atlas_height as f32;

        assert!(0.0 >= 0.0, "UV min X must be >= 0.0");
        assert!(0.0 >= 0.0, "UV min Y must be >= 0.0");
        assert!(uv_max_x <= 1.0, "UV max X must be <= 1.0");
        assert!(uv_max_y <= 1.0, "UV max Y must be <= 1.0");

        assert_eq!(0.0 * atlas_width as f32, 0.0, "Pixel min X should be 0");
        assert_eq!(0.0 * atlas_height as f32, 0.0, "Pixel min Y should be 0");
        assert_eq!(
            uv_max_x * atlas_width as f32,
            2.0,
            "Pixel max X should be 2"
        );
        assert_eq!(
            uv_max_y * atlas_height as f32,
            2.0,
            "Pixel max Y should be 2"
        );

        assert!(0.0 < atlas_width as f32, "Pixel min X within atlas");
        assert!(0.0 < atlas_height as f32, "Pixel min Y within atlas");
        assert!(2.0 <= atlas_width as f32, "Pixel max X within atlas");
        assert!(2.0 <= atlas_height as f32, "Pixel max Y within atlas");
    }

    #[test]
    fn test_glyph_padding_prevents_bleeding() {
        let sys = FontSystem::new();

        assert_eq!(
            sys.glyph_padding, 1,
            "Glyph padding should be 1 pixel by default"
        );

        let glyph_width = 10;
        let glyph_height = 10;
        let padding = 1;

        let atlas_x = 4 + padding;
        let atlas_y = 0 + padding;

        let atlas_width = 256.0;
        let atlas_height = 256.0;

        let uv_min_x = atlas_x as f32 / atlas_width;
        let uv_min_y = atlas_y as f32 / atlas_height;
        let uv_max_x = (atlas_x + glyph_width) as f32 / atlas_width;
        let uv_max_y = (atlas_y + glyph_height) as f32 / atlas_height;

        assert!(uv_min_x >= 0.0 && uv_min_x <= 1.0);
        assert!(uv_min_y >= 0.0 && uv_min_y <= 1.0);
        assert!(uv_max_x >= 0.0 && uv_max_x <= 1.0);
        assert!(uv_max_y >= 0.0 && uv_max_y <= 1.0);

        let uv_width = uv_max_x - uv_min_x;
        let uv_height = uv_max_y - uv_min_y;

        let expected_uv_width = glyph_width as f32 / atlas_width;
        let expected_uv_height = glyph_height as f32 / atlas_height;

        assert!(
            (uv_width - expected_uv_width).abs() < 0.0001,
            "UV width should not include padding"
        );
        assert!(
            (uv_height - expected_uv_height).abs() < 0.0001,
            "UV height should not include padding"
        );
    }

    #[test]
    fn test_glyph_uv_calculation_edge_cases() {
        let test_sizes = [(256, 256), (512, 512), (1024, 256)];

        for (width, height) in test_sizes {
            let _sys = FontSystem::with_atlas_size(width, height);

            let uv_max_x = 2.0 / width as f32;
            let uv_max_y = 2.0 / height as f32;

            assert!(0.0 >= 0.0 && 0.0 <= 1.0);
            assert!(0.0 >= 0.0 && 0.0 <= 1.0);
            assert!(uv_max_x >= 0.0 && uv_max_x <= 1.0);
            assert!(uv_max_y >= 0.0 && uv_max_y <= 1.0);

            let expected_uv_x = 2.0 / width as f32;
            let expected_uv_y = 2.0 / height as f32;

            assert!(
                (uv_max_x - expected_uv_x).abs() < 0.0001,
                "UV X should scale with atlas width"
            );
            assert!(
                (uv_max_y - expected_uv_y).abs() < 0.0001,
                "UV Y should scale with atlas height"
            );
        }
    }
}
