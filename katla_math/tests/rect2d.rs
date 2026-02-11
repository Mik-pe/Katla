use katla_math::Rect2D;
use katla_math::Vec2;

#[test]
fn test_rect2d_new() {
    let min = Vec2::new(0.0, 0.0);
    let max = Vec2::new(10.0, 20.0);
    let rect = Rect2D::new(min, max);

    assert_eq!(rect.min.x(), 0.0);
    assert_eq!(rect.min.y(), 0.0);
    assert_eq!(rect.max.x(), 10.0);
    assert_eq!(rect.max.y(), 20.0);
}

#[test]
fn test_rect2d_from_origin_size() {
    let origin = Vec2::new(5.0, 10.0);
    let size = Vec2::new(15.0, 25.0);
    let rect = Rect2D::from_origin_size(origin, size);

    assert_eq!(rect.min.x(), 5.0);
    assert_eq!(rect.min.y(), 10.0);
    assert_eq!(rect.max.x(), 20.0); // 5 + 15
    assert_eq!(rect.max.y(), 35.0); // 10 + 25
}

#[test]
fn test_rect2d_from_center_half_extents() {
    let center = Vec2::new(10.0, 20.0);
    let half_extents = Vec2::new(5.0, 10.0);
    let rect = Rect2D::from_center_half_extents(center, half_extents);

    assert_eq!(rect.min.x(), 5.0); // 10 - 5
    assert_eq!(rect.min.y(), 10.0); // 20 - 10
    assert_eq!(rect.max.x(), 15.0); // 10 + 5
    assert_eq!(rect.max.y(), 30.0); // 20 + 10
}

#[test]
fn test_rect2d_from_center_size() {
    let center = Vec2::new(10.0, 20.0);
    let size = Vec2::new(10.0, 20.0);
    let rect = Rect2D::from_center_size(center, size);

    assert_eq!(rect.min.x(), 5.0); // 10 - 5
    assert_eq!(rect.min.y(), 10.0); // 20 - 10
    assert_eq!(rect.max.x(), 15.0); // 10 + 5
    assert_eq!(rect.max.y(), 30.0); // 20 + 10
}

#[test]
fn test_rect2d_width() {
    let rect = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 20.0));
    assert_eq!(rect.width(), 10.0);
}

#[test]
fn test_rect2d_height() {
    let rect = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 20.0));
    assert_eq!(rect.height(), 20.0);
}

#[test]
fn test_rect2d_size() {
    let rect = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 20.0));
    let size = rect.size();
    assert_eq!(size.x(), 10.0);
    assert_eq!(size.y(), 20.0);
}

#[test]
fn test_rect2d_center() {
    let rect = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 20.0));
    let center = rect.center();
    assert_eq!(center.x(), 5.0);
    assert_eq!(center.y(), 10.0);
}

#[test]
fn test_rect2d_half_extents() {
    let rect = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 20.0));
    let half_extents = rect.half_extents();
    assert_eq!(half_extents.x(), 5.0);
    assert_eq!(half_extents.y(), 10.0);
}

#[test]
fn test_rect2d_contains() {
    let rect = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 20.0));

    // Point inside
    assert!(rect.contains(Vec2::new(5.0, 10.0)));
    // Point on boundary
    assert!(rect.contains(Vec2::new(0.0, 0.0)));
    assert!(rect.contains(Vec2::new(10.0, 20.0)));
    // Point outside
    assert!(!rect.contains(Vec2::new(-1.0, 10.0)));
    assert!(!rect.contains(Vec2::new(11.0, 10.0)));
}

#[test]
fn test_rect2d_contains_rect() {
    let rect1 = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0));
    let rect2 = Rect2D::new(Vec2::new(2.0, 2.0), Vec2::new(8.0, 8.0));
    let rect3 = Rect2D::new(Vec2::new(-5.0, -5.0), Vec2::new(5.0, 5.0));

    assert!(rect1.contains_rect(&rect2));
    assert!(!rect1.contains_rect(&rect3));
}

#[test]
fn test_rect2d_overlaps() {
    let rect1 = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0));
    let rect2 = Rect2D::new(Vec2::new(5.0, 5.0), Vec2::new(15.0, 15.0));
    let rect3 = Rect2D::new(Vec2::new(15.0, 15.0), Vec2::new(25.0, 25.0));

    assert!(rect1.overlaps(&rect2));
    assert!(!rect1.overlaps(&rect3));
}

#[test]
fn test_rect2d_area() {
    let rect = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 20.0));
    assert_eq!(rect.area(), 200.0); // 10 * 20
}

#[test]
fn test_rect2d_perimeter() {
    let rect = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 20.0));
    assert_eq!(rect.perimeter(), 60.0); // 2 * (10 + 20)
}

#[test]
fn test_rect2d_is_empty() {
    let valid_rect = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 20.0));
    let zero_width = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 20.0));
    let zero_height = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0));
    let inverted = Rect2D::new(Vec2::new(10.0, 20.0), Vec2::new(0.0, 0.0));

    assert!(!valid_rect.is_empty());
    assert!(zero_width.is_empty());
    assert!(zero_height.is_empty());
    assert!(inverted.is_empty());
}

#[test]
fn test_rect2d_inflate() {
    let rect = Rect2D::new(Vec2::new(10.0, 20.0), Vec2::new(20.0, 30.0));
    let inflated = rect.inflate(5.0);

    assert_eq!(inflated.min.x(), 5.0); // 10 - 5
    assert_eq!(inflated.min.y(), 15.0); // 20 - 5
    assert_eq!(inflated.max.x(), 25.0); // 20 + 5
    assert_eq!(inflated.max.y(), 35.0); // 30 + 5
}

#[test]
fn test_rect2d_contract() {
    let rect = Rect2D::new(Vec2::new(10.0, 20.0), Vec2::new(20.0, 30.0));
    let contracted = rect.contract(5.0);

    assert_eq!(contracted.min.x(), 15.0); // 10 + 5
    assert_eq!(contracted.min.y(), 25.0); // 20 + 5
    assert_eq!(contracted.max.x(), 15.0); // 20 - 5
    assert_eq!(contracted.max.y(), 25.0); // 30 - 5
}

#[test]
fn test_rect2d_intersection() {
    let rect1 = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0));
    let rect2 = Rect2D::new(Vec2::new(5.0, 5.0), Vec2::new(15.0, 15.0));
    let rect3 = Rect2D::new(Vec2::new(20.0, 20.0), Vec2::new(30.0, 30.0));

    let intersection1 = rect1.intersection(&rect2);
    assert!(intersection1.is_some());
    let result = intersection1.unwrap();
    assert_eq!(result.min.x(), 5.0);
    assert_eq!(result.min.y(), 5.0);
    assert_eq!(result.max.x(), 10.0);
    assert_eq!(result.max.y(), 10.0);

    let intersection2 = rect1.intersection(&rect3);
    assert!(intersection2.is_none());
}

#[test]
fn test_rect2d_union() {
    let rect1 = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0));
    let rect2 = Rect2D::new(Vec2::new(5.0, 5.0), Vec2::new(15.0, 15.0));
    let union = rect1.union(&rect2);

    assert_eq!(union.min.x(), 0.0);
    assert_eq!(union.min.y(), 0.0);
    assert_eq!(union.max.x(), 15.0);
    assert_eq!(union.max.y(), 15.0);
}

#[test]
fn test_rect2d_empty_at() {
    let point = Vec2::new(5.0, 10.0);
    let rect = Rect2D::empty_at(point);

    assert_eq!(rect.min.x(), 5.0);
    assert_eq!(rect.min.y(), 10.0);
    assert_eq!(rect.max.x(), 5.0);
    assert_eq!(rect.max.y(), 10.0);
    assert!(rect.is_empty());
}

#[test]
fn test_rect2d_from_size() {
    let size = Vec2::new(10.0, 20.0);
    let rect = Rect2D::from_size(size);

    assert_eq!(rect.min.x(), 0.0);
    assert_eq!(rect.min.y(), 0.0);
    assert_eq!(rect.max.x(), 10.0);
    assert_eq!(rect.max.y(), 20.0);
}

#[test]
fn test_rect2d_clamp() {
    let rect = Rect2D::new(Vec2::new(10.0, 20.0), Vec2::new(20.0, 30.0));

    // Point inside
    assert_eq!(rect.clamp(Vec2::new(15.0, 25.0)), Vec2::new(15.0, 25.0));
    // Point above
    assert_eq!(rect.clamp(Vec2::new(15.0, 35.0)), Vec2::new(15.0, 30.0));
    // Point below
    assert_eq!(rect.clamp(Vec2::new(15.0, 15.0)), Vec2::new(15.0, 20.0));
    // Point left
    assert_eq!(rect.clamp(Vec2::new(5.0, 25.0)), Vec2::new(10.0, 25.0));
    // Point right
    assert_eq!(rect.clamp(Vec2::new(25.0, 25.0)), Vec2::new(20.0, 25.0));
}

#[test]
fn test_rect2d_corners() {
    let rect = Rect2D::new(Vec2::new(10.0, 20.0), Vec2::new(20.0, 30.0));
    let corners = rect.corners();

    assert_eq!(corners[0].x(), 10.0);
    assert_eq!(corners[0].y(), 20.0); // bottom-left
    assert_eq!(corners[1].x(), 20.0);
    assert_eq!(corners[1].y(), 20.0); // bottom-right
    assert_eq!(corners[2].x(), 10.0);
    assert_eq!(corners[2].y(), 30.0); // top-left
    assert_eq!(corners[3].x(), 20.0);
    assert_eq!(corners[3].y(), 30.0); // top-right
}

#[test]
fn test_rect2d_position() {
    let rect = Rect2D::new(Vec2::new(5.0, 10.0), Vec2::new(15.0, 25.0));
    let pos = rect.position();

    assert_eq!(pos.x(), 5.0);
    assert_eq!(pos.y(), 10.0);
}

#[test]
fn test_rect2d_lerp() {
    let rect1 = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0));
    let rect2 = Rect2D::new(Vec2::new(10.0, 10.0), Vec2::new(30.0, 30.0));

    // t=0 should give rect1
    let result = rect1.lerp(&rect2, 0.0);
    assert_eq!(result.min.x(), 0.0);
    assert_eq!(result.min.y(), 0.0);
    assert_eq!(result.max.x(), 10.0);
    assert_eq!(result.max.y(), 10.0);

    // t=1 should give rect2
    let result = rect1.lerp(&rect2, 1.0);
    assert_eq!(result.min.x(), 10.0);
    assert_eq!(result.min.y(), 10.0);
    assert_eq!(result.max.x(), 30.0);
    assert_eq!(result.max.y(), 30.0);

    // t=0.5 should give midpoint
    let result = rect1.lerp(&rect2, 0.5);
    assert_eq!(result.min.x(), 5.0);
    assert_eq!(result.min.y(), 5.0);
    assert_eq!(result.max.x(), 20.0);
    assert_eq!(result.max.y(), 20.0);
}

#[test]
fn test_rect2d_default() {
    let rect = Rect2D::default();
    assert_eq!(rect.min.x(), 0.0);
    assert_eq!(rect.min.y(), 0.0);
    assert_eq!(rect.max.x(), 0.0);
    assert_eq!(rect.max.y(), 0.0);
}

#[test]
fn test_rect2d_partial_eq() {
    let rect1 = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 20.0));
    let rect2 = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 20.0));
    let rect3 = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 15.0));

    assert_eq!(rect1, rect2);
    assert_ne!(rect1, rect3);
}

#[test]
fn test_rect2d_expand_to_include() {
    let mut rect = Rect2D::new(Vec2::new(5.0, 10.0), Vec2::new(15.0, 20.0));
    let point = Vec2::new(0.0, 5.0);
    rect.expand_to_include(point);

    assert_eq!(rect.min.x(), 0.0); // Expanded to include point
    assert_eq!(rect.min.y(), 5.0); // Expanded to include point
    assert_eq!(rect.max.x(), 15.0); // Unchanged
    assert_eq!(rect.max.y(), 20.0); // Unchanged
}

#[test]
fn test_rect2d_expand_to_include_rect() {
    let mut rect1 = Rect2D::new(Vec2::new(5.0, 10.0), Vec2::new(15.0, 20.0));
    let rect2 = Rect2D::new(Vec2::new(0.0, 5.0), Vec2::new(25.0, 25.0));
    rect1.expand_to_include_rect(&rect2);

    assert_eq!(rect1.min.x(), 0.0); // Expanded to include rect2
    assert_eq!(rect1.min.y(), 5.0); // Expanded to include rect2
    assert_eq!(rect1.max.x(), 25.0); // Expanded to include rect2
    assert_eq!(rect1.max.y(), 25.0); // Expanded to include rect2
}
