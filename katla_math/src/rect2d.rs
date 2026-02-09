use crate::Vec2;

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Rect2D {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect2D {
    /// Create a new rectangle from min and max points
    #[inline]
    pub fn new(min: Vec2, max: Vec2) -> Self {
        Rect2D { min, max }
    }

    /// Create a rectangle from origin and size
    #[inline]
    pub fn from_origin_size(origin: Vec2, size: Vec2) -> Self {
        Rect2D {
            min: origin,
            max: Vec2 {
                x: origin.x + size.x,
                y: origin.y + size.y,
            },
        }
    }

    /// Create a rectangle from center and half-extents
    #[inline]
    pub fn from_center_half_extents(center: Vec2, half_extents: Vec2) -> Self {
        Rect2D {
            min: Vec2 {
                x: center.x - half_extents.x,
                y: center.y - half_extents.y,
            },
            max: Vec2 {
                x: center.x + half_extents.x,
                y: center.y + half_extents.y,
            },
        }
    }

    /// Create a rectangle from center and full size
    #[inline]
    pub fn from_center_size(center: Vec2, size: Vec2) -> Self {
        Self::from_center_half_extents(center, Vec2 { x: size.x * 0.5, y: size.y * 0.5 })
    }

    /// Get the width of the rectangle
    #[inline]
    pub fn width(&self) -> f32 {
        self.max.x - self.min.x
    }

    /// Get the height of the rectangle
    #[inline]
    pub fn height(&self) -> f32 {
        self.max.y - self.min.y
    }

    /// Get the size of the rectangle
    #[inline]
    pub fn size(&self) -> Vec2 {
        Vec2 {
            x: self.width(),
            y: self.height(),
        }
    }

    /// Get the center of the rectangle
    #[inline]
    pub fn center(&self) -> Vec2 {
        Vec2 {
            x: (self.min.x + self.max.x) * 0.5,
            y: (self.min.y + self.max.y) * 0.5,
        }
    }

    /// Get the half-extents of the rectangle
    #[inline]
    pub fn half_extents(&self) -> Vec2 {
        Vec2 {
            x: self.width() * 0.5,
            y: self.height() * 0.5,
        }
    }

    /// Check if a point is contained in the rectangle
    #[inline]
    pub fn contains(&self, point: Vec2) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
    }

    /// Check if this rectangle contains another rectangle
    #[inline]
    pub fn contains_rect(&self, other: &Rect2D) -> bool {
        self.contains(other.min) && self.contains(other.max)
    }

    /// Check if this rectangle overlaps with another rectangle
    #[inline]
    pub fn overlaps(&self, other: &Rect2D) -> bool {
        self.min.x < other.max.x
            && self.max.x > other.min.x
            && self.min.y < other.max.y
            && self.max.y > other.min.y
    }

    /// Expand the rectangle to include a point
    #[inline]
    pub fn expand_to_include(&mut self, point: Vec2) {
        if point.x < self.min.x {
            self.min.x = point.x;
        }
        if point.x > self.max.x {
            self.max.x = point.x;
        }
        if point.y < self.min.y {
            self.min.y = point.y;
        }
        if point.y > self.max.y {
            self.max.y = point.y;
        }
    }

    /// Expand the rectangle to include another rectangle
    #[inline]
    pub fn expand_to_include_rect(&mut self, other: &Rect2D) {
        self.expand_to_include(other.min);
        self.expand_to_include(other.max);
    }

    /// Get the area of the rectangle
    #[inline]
    pub fn area(&self) -> f32 {
        self.width() * self.height()
    }

    /// Get the perimeter of the rectangle
    #[inline]
    pub fn perimeter(&self) -> f32 {
        2.0 * (self.width() + self.height())
    }

    /// Check if the rectangle has zero area
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.width() <= 0.0 || self.height() <= 0.0
    }

    /// Inflate the rectangle by a given amount on all sides
    #[inline]
    pub fn inflate(&self, amount: f32) -> Rect2D {
        Rect2D {
            min: Vec2 {
                x: self.min.x - amount,
                y: self.min.y - amount,
            },
            max: Vec2 {
                x: self.max.x + amount,
                y: self.max.y + amount,
            },
        }
    }

    /// Contract the rectangle by a given amount on all sides
    #[inline]
    pub fn contract(&self, amount: f32) -> Rect2D {
        self.inflate(-amount)
    }

    /// Get the intersection of two rectangles
    #[inline]
    pub fn intersection(&self, other: &Rect2D) -> Option<Rect2D> {
        let min = Vec2 {
            x: self.min.x.max(other.min.x),
            y: self.min.y.max(other.min.y),
        };
        let max = Vec2 {
            x: self.max.x.min(other.max.x),
            y: self.max.y.min(other.max.y),
        };

        if min.x < max.x && min.y < max.y {
            Some(Rect2D { min, max })
        } else {
            None
        }
    }

    /// Get the union of two rectangles
    #[inline]
    pub fn union(&self, other: &Rect2D) -> Rect2D {
        Rect2D {
            min: Vec2 {
                x: self.min.x.min(other.min.x),
                y: self.min.y.min(other.min.y),
            },
            max: Vec2 {
                x: self.max.x.max(other.max.x),
                y: self.max.y.max(other.max.y),
            },
        }
    }

    /// Create an empty rectangle at a point
    #[inline]
    pub fn empty_at(point: Vec2) -> Rect2D {
        Rect2D {
            min: point,
            max: point,
        }
    }

    /// Create a rectangle with min at (0, 0)
    #[inline]
    pub fn from_size(size: Vec2) -> Rect2D {
        Rect2D {
            min: Vec2 { x: 0.0, y: 0.0 },
            max: size,
        }
    }

    /// Clamp a point to the rectangle bounds
    #[inline]
    pub fn clamp(&self, point: Vec2) -> Vec2 {
        Vec2 {
            x: point.x.max(self.min.x).min(self.max.x),
            y: point.y.max(self.min.y).min(self.max.y),
        }
    }

    /// Get the corners of the rectangle
    #[inline]
    pub fn corners(&self) -> [Vec2; 4] {
        [
            Vec2 { x: self.min.x, y: self.min.y },  // bottom-left
            Vec2 { x: self.max.x, y: self.min.y },  // bottom-right
            Vec2 { x: self.min.x, y: self.max.y },  // top-left
            Vec2 { x: self.max.x, y: self.max.y },  // top-right
        ]
    }

    /// Get the position (min point) of the rectangle
    #[inline]
    pub fn position(&self) -> Vec2 {
        self.min
    }

    /// Lerp between two rectangles
    #[inline]
    pub fn lerp(&self, other: &Rect2D, t: f32) -> Rect2D {
        Rect2D {
            min: self.min + (other.min - self.min) * t,
            max: self.max + (other.max - self.max) * t,
        }
    }
}

impl Default for Rect2D {
    fn default() -> Self {
        Self::new(Vec2 { x: 0.0, y: 0.0 }, Vec2 { x: 0.0, y: 0.0 })
    }
}
