use core::ops::{Add, AddAssign, Index, Mul, MulAssign, Sub, SubAssign};

/// A color represented by red, green, blue, and alpha components.
///
/// All components are in the range [0.0, 1.0]. Values outside this range
/// may be used for certain operations (like HDR rendering) but should be
/// clamped before being used for final output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    /// Creates a new Color from RGBA components.
    ///
    /// All values should be in the range [0.0, 1.0].
    #[inline]
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Creates a new Color from RGB components with alpha = 1.0.
    ///
    /// All values should be in the range [0.0, 1.0].
    #[inline]
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Creates a new Color from u8 components (0-255).
    ///
    /// Alpha is set to 1.0 (255).
    #[inline]
    pub fn from_u8(r: u8, g: u8, b: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: 1.0,
        }
    }

    /// Creates a new Color from u8 RGBA components (0-255).
    #[inline]
    pub fn from_u8_rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    /// Creates a new Color from a 24-bit RGB hex value (0xRRGGBB).
    ///
    /// # Examples
    /// ```
    /// use katla_math::Color;
    /// let red = Color::from_rgb_hex(0xFF0000);
    /// let green = Color::from_rgb_hex(0x00FF00);
    /// let blue = Color::from_rgb_hex(0x0000FF);
    /// ```
    #[inline]
    pub fn from_rgb_hex(hex: u32) -> Self {
        Self {
            r: ((hex >> 16) & 0xFF) as f32 / 255.0,
            g: ((hex >> 8) & 0xFF) as f32 / 255.0,
            b: (hex & 0xFF) as f32 / 255.0,
            a: 1.0,
        }
    }

    /// Creates a new Color from a 32-bit RGBA hex value (0xRRGGBBAA).
    ///
    /// # Examples
    /// ```
    /// use katla_math::Color;
    /// let red = Color::from_rgba_hex(0xFF0000FF);
    /// let transparent_red = Color::from_rgba_hex(0xFF000080);
    /// ```
    #[inline]
    pub fn from_rgba_hex(hex: u32) -> Self {
        Self {
            r: ((hex >> 24) & 0xFF) as f32 / 255.0,
            g: ((hex >> 16) & 0xFF) as f32 / 255.0,
            b: ((hex >> 8) & 0xFF) as f32 / 255.0,
            a: (hex & 0xFF) as f32 / 255.0,
        }
    }

    /// Converts the Color to a byte array [r, g, b, a] with values in 0-255.
    ///
    /// Useful for texture creation and image output.
    #[inline]
    pub fn to_bytes(&self) -> [u8; 4] {
        [
            (self.r.clamp(0.0, 1.0) * 255.0).round() as u8,
            (self.g.clamp(0.0, 1.0) * 255.0).round() as u8,
            (self.b.clamp(0.0, 1.0) * 255.0).round() as u8,
            (self.a.clamp(0.0, 1.0) * 255.0).round() as u8,
        ]
    }

    /// Converts the Color to an array [r, g, b, a] with values in [0.0, 1.0].
    ///
    /// Useful for shader uniforms.
    #[inline]
    pub fn to_array(&self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Converts the Color to a Vec4.
    #[inline]
    pub fn to_vec4(&self) -> crate::Vec4 {
        crate::Vec4::new(self.r, self.g, self.b, self.a)
    }

    /// Creates a Color from a Vec4.
    #[inline]
    pub fn from_vec4(v: crate::Vec4) -> Self {
        Self {
            r: v[0],
            g: v[1],
            b: v[2],
            a: v[3],
        }
    }

    /// Linear interpolation between two colors.
    ///
    /// # Examples
    /// ```
    /// use katla_math::Color;
    /// let result = Color::lerp(Color::RED, Color::BLUE, 0.5);
    /// ```
    #[inline]
    pub fn lerp(a: Color, b: Color, t: f32) -> Self {
        Self {
            r: a.r + (b.r - a.r) * t,
            g: a.g + (b.g - a.g) * t,
            b: a.b + (b.b - a.b) * t,
            a: a.a + (b.a - a.a) * t,
        }
    }

    /// Adjusts the brightness of the color by multiplying RGB by a factor.
    ///
    /// Alpha is preserved.
    #[inline]
    pub fn brightness(&self, factor: f32) -> Color {
        Color {
            r: self.r * factor,
            g: self.g * factor,
            b: self.b * factor,
            a: self.a,
        }
    }

    /// Returns a new color with the specified alpha value.
    ///
    /// RGB components are preserved.
    #[inline]
    pub fn with_alpha(&self, alpha: f32) -> Color {
        Color {
            r: self.r,
            g: self.g,
            b: self.b,
            a: alpha,
        }
    }

    /// Adjusts the saturation of the color.
    ///
    /// Factor of 0.0 produces grayscale, 1.0 preserves original,
    /// values > 1.0 increase saturation.
    #[inline]
    pub fn saturate(&self, factor: f32) -> Color {
        let gray = self.r * 0.299 + self.g * 0.587 + self.b * 0.114;
        Color {
            r: gray + (self.r - gray) * factor,
            g: gray + (self.g - gray) * factor,
            b: gray + (self.b - gray) * factor,
            a: self.a,
        }
    }

    /// Converts the color to HSV color space.
    #[inline]
    pub fn to_hsv(&self) -> HSV {
        let max = self.r.max(self.g).max(self.b);
        let min = self.r.min(self.g).min(self.b);
        let delta = max - min;

        let h = if delta == 0.0 {
            0.0
        } else if max == self.r {
            60.0 * (((self.g - self.b) / delta) % 6.0)
        } else if max == self.g {
            60.0 * (((self.b - self.r) / delta) + 2.0)
        } else {
            60.0 * (((self.r - self.g) / delta) + 4.0)
        };

        let h = if h < 0.0 { h + 360.0 } else { h };

        let s = if max == 0.0 { 0.0 } else { delta / max };

        HSV { h, s, v: max }
    }

    /// Creates a color from HSV color space.
    #[inline]
    pub fn from_hsv(hsv: HSV) -> Self {
        let c = hsv.v * hsv.s;
        let x = c * (1.0 - ((hsv.h / 60.0) % 2.0 - 1.0).abs());
        let m = hsv.v - c;

        let (r, g, b) = if hsv.h < 60.0 {
            (c, x, 0.0)
        } else if hsv.h < 120.0 {
            (x, c, 0.0)
        } else if hsv.h < 180.0 {
            (0.0, c, x)
        } else if hsv.h < 240.0 {
            (0.0, x, c)
        } else if hsv.h < 300.0 {
            (x, 0.0, c)
        } else {
            (c, 0.0, x)
        };

        Color {
            r: r + m,
            g: g + m,
            b: b + m,
            a: 1.0,
        }
    }

    /// Converts from sRGB to linear color space.
    ///
    /// Use this when reading colors from sRGB sources and performing
    /// color calculations (like blending) in linear space.
    #[inline]
    pub fn to_linear(&self) -> Color {
        Color {
            r: self.srgb_to_linear(self.r),
            g: self.srgb_to_linear(self.g),
            b: self.srgb_to_linear(self.b),
            a: self.a,
        }
    }

    /// Converts from linear to sRGB color space.
    ///
    /// Use this to convert linear colors back to sRGB for display.
    #[inline]
    pub fn to_srgb(&self) -> Color {
        Color {
            r: self.linear_to_srgb(self.r),
            g: self.linear_to_srgb(self.g),
            b: self.linear_to_srgb(self.b),
            a: self.a,
        }
    }

    #[inline]
    fn srgb_to_linear(&self, c: f32) -> f32 {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }

    #[inline]
    fn linear_to_srgb(&self, c: f32) -> f32 {
        if c <= 0.0031308 {
            c * 12.92
        } else {
            (1.055 * c.powf(1.0 / 2.4)) - 0.055
        }
    }

    /// Returns a clamped version of this color with all components in [0.0, 1.0].
    #[inline]
    pub fn clamped(&self) -> Color {
        Color {
            r: self.r.clamp(0.0, 1.0),
            g: self.g.clamp(0.0, 1.0),
            b: self.b.clamp(0.0, 1.0),
            a: self.a.clamp(0.0, 1.0),
        }
    }

    /// Checks if all components are in the valid range [0.0, 1.0].
    #[inline]
    pub fn is_valid(&self) -> bool {
        self.r >= 0.0
            && self.r <= 1.0
            && self.g >= 0.0
            && self.g <= 1.0
            && self.b >= 0.0
            && self.b <= 1.0
            && self.a >= 0.0
            && self.a <= 1.0
    }

    /// Converts to a clear color value array for Vulkan.
    #[inline]
    pub fn to_clearcolor_value(&self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    // Named color constants

    pub const BLACK: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    pub const WHITE: Color = Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    /// Special alpha value that signals "opaque image" mode in shaders.
    /// When a vertex has this alpha value, the shader forces output alpha to 1.0
    /// regardless of texture alpha. Used for viewport and thumbnail rendering.
    pub const OPAQUE_IMAGE_ALPHA: f32 = -1.0;
    /// Opaque white color for rendering images without blending.
    /// The negative alpha signals the shader to force output alpha = 1.0.
    /// Use this for viewport, thumbnails, and other textures that should not blend.
    pub const OPAQUE_IMAGE: Color = Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: Self::OPAQUE_IMAGE_ALPHA,
    };
    pub const RED: Color = Color {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    pub const GREEN: Color = Color {
        r: 0.0,
        g: 1.0,
        b: 0.0,
        a: 1.0,
    };
    pub const BLUE: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    };
    pub const YELLOW: Color = Color {
        r: 1.0,
        g: 1.0,
        b: 0.0,
        a: 1.0,
    };
    pub const CYAN: Color = Color {
        r: 0.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    pub const MAGENTA: Color = Color {
        r: 1.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    };
    pub const TRANSPARENT: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };
}

impl Default for Color {
    #[inline]
    fn default() -> Self {
        Color::WHITE
    }
}

impl Index<usize> for Color {
    type Output = f32;

    #[inline]
    fn index(&self, index: usize) -> &f32 {
        match index {
            0 => &self.r,
            1 => &self.g,
            2 => &self.b,
            3 => &self.a,
            _ => panic!("Index out of bounds for Color"),
        }
    }
}

impl Add for Color {
    type Output = Color;

    #[inline]
    fn add(self, rhs: Self) -> Self::Output {
        Color {
            r: self.r + rhs.r,
            g: self.g + rhs.g,
            b: self.b + rhs.b,
            a: self.a + rhs.a,
        }
    }
}

impl Sub for Color {
    type Output = Color;

    #[inline]
    fn sub(self, rhs: Self) -> Self::Output {
        Color {
            r: self.r - rhs.r,
            g: self.g - rhs.g,
            b: self.b - rhs.b,
            a: self.a - rhs.a,
        }
    }
}

impl Mul<f32> for Color {
    type Output = Color;

    #[inline]
    fn mul(self, rhs: f32) -> Self::Output {
        Color {
            r: self.r * rhs,
            g: self.g * rhs,
            b: self.b * rhs,
            a: self.a * rhs,
        }
    }
}

impl Mul for Color {
    type Output = Color;

    #[inline]
    fn mul(self, rhs: Self) -> Self::Output {
        Color {
            r: self.r * rhs.r,
            g: self.g * rhs.g,
            b: self.b * rhs.b,
            a: self.a * rhs.a,
        }
    }
}

impl AddAssign for Color {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.r += rhs.r;
        self.g += rhs.g;
        self.b += rhs.b;
        self.a += rhs.a;
    }
}

impl SubAssign for Color {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.r -= rhs.r;
        self.g -= rhs.g;
        self.b -= rhs.b;
        self.a -= rhs.a;
    }
}

// Implement MulAssign<f32> for convenience
impl MulAssign<f32> for Color {
    #[inline]
    fn mul_assign(&mut self, rhs: f32) {
        self.r *= rhs;
        self.g *= rhs;
        self.b *= rhs;
        self.a *= rhs;
    }
}

impl From<Color> for crate::Vec4 {
    #[inline]
    fn from(color: Color) -> Self {
        crate::Vec4::new(color.r, color.g, color.b, color.a)
    }
}

impl From<crate::Vec4> for Color {
    #[inline]
    fn from(v: crate::Vec4) -> Self {
        Color {
            r: v[0],
            g: v[1],
            b: v[2],
            a: v[3],
        }
    }
}

/// Color represented in HSV (Hue, Saturation, Value) color space.
///
/// - h: Hue angle in degrees [0.0, 360.0)
/// - s: Saturation [0.0, 1.0]
/// - v: Value/Brightness [0.0, 1.0]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HSV {
    pub h: f32,
    pub s: f32,
    pub v: f32,
}

impl HSV {
    /// Creates a new HSV color.
    ///
    /// # Arguments
    /// * `h` - Hue in degrees [0.0, 360.0)
    /// * `s` - Saturation [0.0, 1.0]
    /// * `v` - Value/Brightness [0.0, 1.0]
    #[inline]
    pub const fn new(h: f32, s: f32, v: f32) -> Self {
        Self { h, s, v }
    }
}
