//! Katla-native 2D size type.
//!
//! This module provides a `Size2D` type that replaces `ash::vk::Extent2D` in the
//! public API. This ensures the katla_gfx API doesn't leak Vulkan types.

/// A 2D size in pixels/texels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Size2D {
    pub width: u32,
    pub height: u32,
}

impl Size2D {
    /// Create a new 2D size.
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// Calculate the area (width * height).
    pub fn area(&self) -> u32 {
        self.width * self.height
    }

    /// Calculate the aspect ratio (width / height).
    /// Returns 0.0 if height is 0.
    pub fn aspect_ratio(&self) -> f32 {
        if self.height == 0 {
            0.0
        } else {
            self.width as f32 / self.height as f32
        }
    }
}

impl Default for Size2D {
    fn default() -> Self {
        Self {
            width: 512,
            height: 512,
        }
    }
}

#[cfg(feature = "vulkan")]
impl From<Size2D> for ash::vk::Extent2D {
    fn from(size: Size2D) -> Self {
        ash::vk::Extent2D {
            width: size.width,
            height: size.height,
        }
    }
}

#[cfg(feature = "vulkan")]
impl From<ash::vk::Extent2D> for Size2D {
    fn from(extent: ash::vk::Extent2D) -> Self {
        Self {
            width: extent.width,
            height: extent.height,
        }
    }
}

#[cfg(all(test, feature = "vulkan"))]
mod tests {
    use super::*;

    #[test]
    fn test_size2d_new() {
        let size = Size2D::new(800, 600);
        assert_eq!(size.width, 800);
        assert_eq!(size.height, 600);
    }

    #[test]
    fn test_size2d_area() {
        let size = Size2D::new(800, 600);
        assert_eq!(size.area(), 480000);
    }

    #[test]
    fn test_size2d_aspect_ratio() {
        let size = Size2D::new(800, 600);
        let ratio = size.aspect_ratio();
        assert!((ratio - 1.3333334).abs() < 0.0001);
    }

    #[test]
    fn test_size2d_aspect_ratio_zero_height() {
        let size = Size2D::new(800, 0);
        assert_eq!(size.aspect_ratio(), 0.0);
    }

    #[test]
    fn test_size2d_default() {
        let size = Size2D::default();
        assert_eq!(size.width, 512);
        assert_eq!(size.height, 512);
    }

    #[test]
    fn test_size2d_to_vk_extent2d() {
        let size = Size2D::new(800, 600);
        let extent: ash::vk::Extent2D = size.into();
        assert_eq!(extent.width, 800);
        assert_eq!(extent.height, 600);
    }

    #[test]
    fn test_size2d_from_vk_extent2d() {
        let extent = ash::vk::Extent2D {
            width: 800,
            height: 600,
        };
        let size: Size2D = extent.into();
        assert_eq!(size.width, 800);
        assert_eq!(size.height, 600);
    }
}
