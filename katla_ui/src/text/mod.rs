//! Text rendering and font handling.
//!
//! This module provides font loading, glyph caching, and text rendering.
//!
//! # Status
//!
//! Currently a placeholder. Full implementation will include:
//! - Font loading with `fontdue`
//! - Glyph cache with texture atlas
//! - Text measurement and layout

use katla_math::{Rect2D, Vec2};

/// Placeholder for font system.
pub struct FontSystem {
    // TODO: Implement with fontdue
}

impl FontSystem {
    /// Create a new font system.
    pub fn new() -> Self {
        Self {}
    }

    /// Measure text dimensions (placeholder).
    pub fn measure_text(&self, text: &str, size: f32) -> Vec2 {
        // Rough estimate: 0.5x width per character
        Vec2::new(text.len() as f32 * size * 0.5, size)
    }
}

impl Default for FontSystem {
    fn default() -> Self {
        Self::new()
    }
}

/// A glyph in the cache.
#[derive(Debug, Clone)]
pub struct CachedGlyph {
    /// UV rectangle in the texture atlas.
    pub uv_rect: Rect2D,
    /// Size of the glyph in pixels.
    pub size: Vec2,
    /// Offset from the baseline.
    pub offset: Vec2,
    /// Advance width to the next character.
    pub advance: f32,
}
