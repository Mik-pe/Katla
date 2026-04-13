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
            max: Vec2::new(origin.x() + size.x(), origin.y() + size.y()),
        }
    }

    /// Create a rectangle from center and half-extents
    #[inline]
    pub fn from_center_half_extents(center: Vec2, half_extents: Vec2) -> Self {
        Rect2D {
            min: Vec2::new(center.x() - half_extents.x(), center.y() - half_extents.y()),
            max: Vec2::new(center.x() + half_extents.x(), center.y() + half_extents.y()),
        }
    }

    /// Create a rectangle from center and full size
    #[inline]
    pub fn from_center_size(center: Vec2, size: Vec2) -> Self {
        Self::from_center_half_extents(center, Vec2::new(size.x() * 0.5, size.y() * 0.5))
    }

    /// Get the width of the rectangle
    #[inline]
    pub fn width(&self) -> f32 {
        self.max.x() - self.min.x()
    }

    /// Get the height of the rectangle
    #[inline]
    pub fn height(&self) -> f32 {
        self.max.y() - self.min.y()
    }

    /// Get the size of the rectangle
    #[inline]
    pub fn size(&self) -> Vec2 {
        Vec2::new(self.width(), self.height())
    }

    /// Get the center of the rectangle
    #[inline]
    pub fn center(&self) -> Vec2 {
        Vec2::new(
            (self.min.x() + self.max.x()) * 0.5,
            (self.min.y() + self.max.y()) * 0.5,
        )
    }

    /// Get the half-extents of the rectangle
    #[inline]
    pub fn half_extents(&self) -> Vec2 {
        Vec2::new(self.width() * 0.5, self.height() * 0.5)
    }

    /// Check if a point is contained in the rectangle
    #[inline]
    pub fn contains(&self, point: Vec2) -> bool {
        point.x() >= self.min.x()
            && point.x() <= self.max.x()
            && point.y() >= self.min.y()
            && point.y() <= self.max.y()
    }

    /// Check if this rectangle contains another rectangle
    #[inline]
    pub fn contains_rect(&self, other: &Rect2D) -> bool {
        self.contains(other.min) && self.contains(other.max)
    }

    /// Check if this rectangle overlaps with another rectangle
    #[inline]
    pub fn overlaps(&self, other: &Rect2D) -> bool {
        self.min.x() < other.max.x()
            && self.max.x() > other.min.x()
            && self.min.y() < other.max.y()
            && self.max.y() > other.min.y()
    }

    /// Expand the rectangle to include a point
    #[inline]
    pub fn expand_to_include(&mut self, point: Vec2) {
        if point.x() < self.min.x() {
            self.min = Vec2::new(point.x(), self.min.y());
        }
        if point.x() > self.max.x() {
            self.max = Vec2::new(point.x(), self.max.y());
        }
        if point.y() < self.min.y() {
            self.min = Vec2::new(self.min.x(), point.y());
        }
        if point.y() > self.max.y() {
            self.max = Vec2::new(self.max.x(), point.y());
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
            min: Vec2::new(self.min.x() - amount, self.min.y() - amount),
            max: Vec2::new(self.max.x() + amount, self.max.y() + amount),
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
        let min = Vec2::new(
            self.min.x().max(other.min.x()),
            self.min.y().max(other.min.y()),
        );
        let max = Vec2::new(
            self.max.x().min(other.max.x()),
            self.max.y().min(other.max.y()),
        );

        if min.x() < max.x() && min.y() < max.y() {
            Some(Rect2D { min, max })
        } else {
            None
        }
    }

    /// Get the union of two rectangles
    #[inline]
    pub fn union(&self, other: &Rect2D) -> Rect2D {
        Rect2D {
            min: Vec2::new(
                self.min.x().min(other.min.x()),
                self.min.y().min(other.min.y()),
            ),
            max: Vec2::new(
                self.max.x().max(other.max.x()),
                self.max.y().max(other.max.y()),
            ),
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
            min: Vec2::new(0.0, 0.0),
            max: size,
        }
    }

    /// Clamp a point to the rectangle bounds
    #[inline]
    pub fn clamp(&self, point: Vec2) -> Vec2 {
        Vec2::new(
            point.x().max(self.min.x()).min(self.max.x()),
            point.y().max(self.min.y()).min(self.max.y()),
        )
    }

    /// Get the corners of the rectangle
    #[inline]
    pub fn corners(&self) -> [Vec2; 4] {
        [
            Vec2::new(self.min.x(), self.min.y()), // bottom-left
            Vec2::new(self.max.x(), self.min.y()), // bottom-right
            Vec2::new(self.min.x(), self.max.y()), // top-left
            Vec2::new(self.max.x(), self.max.y()), // top-right
        ]
    }

    /// Get the position (min point) of the rectangle
    #[inline]
    pub fn position(&self) -> Vec2 {
        self.min
    }

    #[inline]
    pub fn to_clip_array(&self) -> [f32; 4] {
        [self.min.x(), self.min.y(), self.width(), self.height()]
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
        Self::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOL: f32 = 1e-5;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < TOL
    }

    #[test]
    fn test_rect2d_new() {
        let r = Rect2D::new(Vec2::new(1.0, 2.0), Vec2::new(5.0, 6.0));
        assert!(approx_eq(r.min.x(), 1.0));
        assert!(approx_eq(r.min.y(), 2.0));
        assert!(approx_eq(r.max.x(), 5.0));
        assert!(approx_eq(r.max.y(), 6.0));
    }

    #[test]
    fn test_rect2d_from_origin_size() {
        let r = Rect2D::from_origin_size(Vec2::new(1.0, 2.0), Vec2::new(3.0, 4.0));
        assert!(approx_eq(r.min.x(), 1.0));
        assert!(approx_eq(r.min.y(), 2.0));
        assert!(approx_eq(r.max.x(), 4.0));
        assert!(approx_eq(r.max.y(), 6.0));
    }

    #[test]
    fn test_rect2d_width_height() {
        let r = Rect2D::new(Vec2::new(1.0, 2.0), Vec2::new(5.0, 8.0));
        assert!(approx_eq(r.width(), 4.0));
        assert!(approx_eq(r.height(), 6.0));
    }

    #[test]
    fn test_rect2d_center() {
        let r = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(4.0, 6.0));
        let c = r.center();
        assert!(approx_eq(c.x(), 2.0));
        assert!(approx_eq(c.y(), 3.0));
    }

    #[test]
    fn test_rect2d_contains() {
        let r = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0));
        assert!(r.contains(Vec2::new(5.0, 5.0)));
        assert!(r.contains(Vec2::new(0.0, 0.0)));
        assert!(r.contains(Vec2::new(10.0, 10.0)));
        assert!(!r.contains(Vec2::new(11.0, 5.0)));
        assert!(!r.contains(Vec2::new(5.0, -1.0)));
    }

    #[test]
    fn test_rect2d_contains_rect() {
        let outer = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0));
        let inner = Rect2D::new(Vec2::new(2.0, 2.0), Vec2::new(8.0, 8.0));
        let partial = Rect2D::new(Vec2::new(5.0, 5.0), Vec2::new(15.0, 15.0));
        let outside = Rect2D::new(Vec2::new(11.0, 11.0), Vec2::new(20.0, 20.0));
        assert!(outer.contains_rect(&inner));
        assert!(!outer.contains_rect(&partial));
        assert!(!outer.contains_rect(&outside));
    }

    #[test]
    fn test_rect2d_overlaps() {
        let a = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0));
        let b = Rect2D::new(Vec2::new(5.0, 5.0), Vec2::new(15.0, 15.0));
        let c = Rect2D::new(Vec2::new(11.0, 11.0), Vec2::new(20.0, 20.0));
        assert!(a.overlaps(&b));
        assert!(!a.overlaps(&c));
    }

    #[test]
    fn test_rect2d_union() {
        let a = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(5.0, 5.0));
        let b = Rect2D::new(Vec2::new(3.0, 3.0), Vec2::new(10.0, 10.0));
        let u = a.union(&b);
        assert!(approx_eq(u.min.x(), 0.0));
        assert!(approx_eq(u.min.y(), 0.0));
        assert!(approx_eq(u.max.x(), 10.0));
        assert!(approx_eq(u.max.y(), 10.0));
        assert!(u.contains_rect(&a));
        assert!(u.contains_rect(&b));
    }

    #[test]
    fn test_rect2d_intersection() {
        let a = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(5.0, 5.0));
        let b = Rect2D::new(Vec2::new(3.0, 3.0), Vec2::new(10.0, 10.0));
        let inter = a.intersection(&b).unwrap();
        assert!(approx_eq(inter.min.x(), 3.0));
        assert!(approx_eq(inter.min.y(), 3.0));
        assert!(approx_eq(inter.max.x(), 5.0));
        assert!(approx_eq(inter.max.y(), 5.0));

        let disjoint = Rect2D::new(Vec2::new(10.0, 10.0), Vec2::new(20.0, 20.0));
        assert!(a.intersection(&disjoint).is_none());
    }

    #[test]
    fn test_rect2d_inflate_contract() {
        let r = Rect2D::new(Vec2::new(5.0, 5.0), Vec2::new(10.0, 10.0));
        let inflated = r.inflate(2.0);
        assert!(approx_eq(inflated.min.x(), 3.0));
        assert!(approx_eq(inflated.max.x(), 12.0));
        assert!(approx_eq(inflated.width(), 9.0));

        let contracted = r.contract(1.0);
        assert!(approx_eq(contracted.min.x(), 6.0));
        assert!(approx_eq(contracted.max.x(), 9.0));
    }

    #[test]
    fn test_rect2d_clamp() {
        let r = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0));
        let clamped = r.clamp(Vec2::new(-5.0, 15.0));
        assert!(approx_eq(clamped.x(), 0.0));
        assert!(approx_eq(clamped.y(), 10.0));
    }

    #[test]
    fn test_rect2d_area_perimeter() {
        let r = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(5.0, 10.0));
        assert!(approx_eq(r.area(), 50.0));
        assert!(approx_eq(r.perimeter(), 30.0));
    }

    #[test]
    fn test_rect2d_is_empty() {
        assert!(Rect2D::default().is_empty());
        assert!(!Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0)).is_empty());
        assert!(Rect2D::new(Vec2::new(2.0, 2.0), Vec2::new(2.0, 5.0)).is_empty());
    }

    #[test]
    fn test_rect2d_lerp() {
        let a = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0));
        let b = Rect2D::new(Vec2::new(10.0, 10.0), Vec2::new(20.0, 20.0));

        let at_zero = a.lerp(&b, 0.0);
        assert!(approx_eq(at_zero.min.x(), a.min.x()));
        assert!(approx_eq(at_zero.max.x(), a.max.x()));

        let at_one = a.lerp(&b, 1.0);
        assert!(approx_eq(at_one.min.x(), b.min.x()));
        assert!(approx_eq(at_one.max.x(), b.max.x()));

        let at_half = a.lerp(&b, 0.5);
        assert!(approx_eq(at_half.min.x(), 5.0));
        assert!(approx_eq(at_half.max.x(), 15.0));
    }
}
