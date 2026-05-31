use super::*;

#[test]
fn test_id_generation() {
    let mut ctx = UiContext::new();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);

    // Each call generates a unique ID due to counter increment
    let id1 = ctx.generate_id("test");
    let id2 = ctx.generate_id("test");
    let id3 = ctx.generate_id("other");

    // Same label produces DIFFERENT IDs (counter ensures uniqueness)
    assert_ne!(id1, id2, "same label should get different IDs");
    // Different labels also produce different IDs
    assert_ne!(id1, id3, "different labels should get different IDs");

    ctx.end();
}

#[test]
fn test_id_generation_consistent_across_frames() {
    let mut ctx = UiContext::new();

    // Frame 1
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    let frame1_id1 = ctx.generate_id("button");
    let frame1_id2 = ctx.generate_id("button");
    ctx.end();

    // Frame 2 - same call order should produce same IDs
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    let frame2_id1 = ctx.generate_id("button");
    let frame2_id2 = ctx.generate_id("button");
    ctx.end();

    // IDs should be consistent across frames (important for state persistence)
    assert_eq!(frame1_id1, frame2_id1, "first button ID should be stable");
    assert_eq!(frame1_id2, frame2_id2, "second button ID should be stable");
}
