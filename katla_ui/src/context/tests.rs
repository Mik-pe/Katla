use katla_math::Vec2;

use super::UiContext;

#[test]
fn test_declarative_input_consumption_accumulates() {
    let mut ctx = UiContext::new();

    ctx.set_declarative_input_consumed(true);
    ctx.set_declarative_input_consumed(false);

    assert!(ctx.is_input_consumed_by_declarative());
}

#[test]
fn test_begin_resets_declarative_input_consumption() {
    let mut ctx = UiContext::new();
    ctx.set_declarative_input_consumed(true);

    ctx.begin(Vec2::new(1280.0, 720.0), 1.0);

    assert!(!ctx.is_input_consumed_by_declarative());
}

#[test]
fn test_glyph_quads_use_physical_pixel_origins_at_fractional_layout_positions() {
    for scale in [1.0, 1.5, 2.0, 3.0] {
        let mut ctx = UiContext::new();
        ctx.fonts_mut()
            .add_font(include_bytes!(
                "../../../resources/fonts/roboto-regular.ttf"
            ))
            .unwrap();
        ctx.begin(Vec2::new(300.0, 100.0), scale);
        ctx.draw_text(
            "Katla äö",
            Vec2::new(10.37, 7.61),
            katla_math::Color::WHITE,
            12.0,
        );
        let list = ctx.end();
        assert!(!list.instances().is_empty());
        for glyph in list.instances() {
            for coordinate in glyph.position {
                let physical = coordinate * scale;
                assert!((physical - physical.round()).abs() < 0.0001);
            }
        }
    }
}
