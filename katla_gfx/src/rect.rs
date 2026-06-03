//! Axis-aligned 2D rectangle in pixel coordinates.

/// Axis-aligned 2D rectangle stored as min (top-left) and max (bottom-right).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// Top-left corner (x, y).
    pub min: [f32; 2],
    /// Bottom-right corner (x + width, y + height).
    pub max: [f32; 2],
}

impl Rect {
    #[inline]
    pub fn new(min: [f32; 2], max: [f32; 2]) -> Self {
        Self { min, max }
    }

    #[inline]
    pub fn width(&self) -> f32 {
        self.max[0] - self.min[0]
    }

    #[inline]
    pub fn height(&self) -> f32 {
        self.max[1] - self.min[1]
    }
}
