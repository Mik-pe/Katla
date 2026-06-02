use std::any::Any;

use katla_math::{Color, Rect2D, Vec2};
use taffy::{Dimension, Size, Style};

use crate::context::UiContext;
use crate::input::mouse_button;

use super::super::animation::AnimationState;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, StateId, ViewId};
use super::super::widget::{DrawInfo, InputContext, InputResult, MeasureFn, Widget};

pub struct ColorPicker {
    pub label: String,
    pub value_id: StateId,
}

impl Widget for ColorPicker {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<Self>().is_some() {
            DiffAction::Update
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, measure: MeasureFn<'_>) -> Style {
        let text_size = measure(&self.label, None);
        Style {
            size: Size {
                width: Dimension::Length((text_size.x() + 40.0).max(100.0)),
                height: Dimension::Length(text_size.y() + 12.0),
            },
            ..Style::default()
        }
    }

    fn handle_input(
        &self,
        ctx: &mut InputContext<'_>,
        _state: &mut StateArena,
        bounds: Rect2D,
        _children: &[ViewId],
    ) -> InputResult {
        if bounds.contains(ctx.mouse_pos) && ctx.input.mouse_clicked(mouse_button::LEFT) {
            return InputResult::Consumed;
        }

        InputResult::Ignore
    }

    fn draw(
        &self,
        ctx: &mut UiContext,
        state: &StateArena,
        bounds: Rect2D,
        animation: &AnimationState,
        _children: &[ViewId],
        _info: &DrawInfo,
    ) {
        let color: Color = state.get(self.value_id).unwrap_or_default();

        let swatch_size = bounds.height() - 4.0;
        let swatch_bounds = Rect2D::from_origin_size(
            Vec2::new(bounds.min.x() + 2.0, bounds.min.y() + 2.0),
            Vec2::new(swatch_size, swatch_size),
        );
        ctx.draw_rounded_rect(swatch_bounds, color, 2.0);

        if !self.label.is_empty() {
            let font_size = ctx.style().font_size;
            let text_pos = Vec2::new(
                swatch_bounds.max.x() + ctx.style().item_inner_spacing,
                bounds.center().y() - font_size * 0.5,
            );
            ctx.draw_text(
                &self.label,
                text_pos,
                animation.apply_to_color(ctx.style().text_color),
                font_size,
            );
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn interactive(&self) -> bool {
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::declarative::diff::DiffAction;
    use crate::declarative::widget::{InputResult, Widget};
    use crate::declarative::widgets::progress::Progress;
    use crate::input::UiInputState;

    fn make_color_picker() -> ColorPicker {
        ColorPicker {
            label: "Color".into(),
            value_id: StateId::test_id(),
        }
    }

    #[test]
    fn test_color_picker_click_consumed() {
        let picker = make_color_picker();
        let mut state = StateArena::new();
        state.set(picker.value_id, Color::new(1.0, 0.0, 0.0, 1.0));

        let mut input = UiInputState::new();
        input.mouse_pos = Vec2::new(50.0, 10.0);
        input.set_mouse_button_with_time(mouse_button::LEFT, true, 1.0);

        let bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(200.0, 24.0));

        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: input.mouse_pos,
            callbacks: &mut callbacks,
            actions: &mut actions,
            view_id: ViewId::from(slotmap::KeyData::from_ffi(0)),
            active_id: None,
            focused_id: None,
        };

        let result = picker.handle_input(&mut ctx, &mut state, bounds, &[]);
        assert_eq!(result, InputResult::Consumed);
    }

    #[test]
    fn test_color_picker_click_outside_ignored() {
        let picker = make_color_picker();
        let mut state = StateArena::new();

        let mut input = UiInputState::new();
        input.mouse_pos = Vec2::new(300.0, 10.0);
        input.set_mouse_button_with_time(mouse_button::LEFT, true, 1.0);

        let bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(200.0, 24.0));

        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: input.mouse_pos,
            callbacks: &mut callbacks,
            actions: &mut actions,
            view_id: ViewId::from(slotmap::KeyData::from_ffi(0)),
            active_id: None,
            focused_id: None,
        };

        let result = picker.handle_input(&mut ctx, &mut state, bounds, &[]);
        assert_eq!(result, InputResult::Ignore);
    }

    #[test]
    fn test_color_picker_diff() {
        let a = make_color_picker();
        let b = make_color_picker();
        assert_eq!(b.diff_against(&a), DiffAction::Update);

        let progress = Progress {
            value: 0.5,
            range: 0.0..=1.0,
            fill_color: None,
            label: None,
        };
        assert_eq!(a.diff_against(&progress), DiffAction::Replace);
    }

    #[test]
    fn test_color_picker_focusable() {
        let picker = make_color_picker();
        assert!(picker.focusable());
    }
}
