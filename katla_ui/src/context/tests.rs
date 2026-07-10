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
