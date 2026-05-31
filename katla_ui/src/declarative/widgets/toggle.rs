use std::any::Any;

use katla_math::{Rect2D, Vec2};
use taffy::{Dimension, Size, Style};

use crate::context::UiContext;
use crate::input::mouse_button;

use super::super::animation::AnimationState;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, StateId, ViewId};
use super::super::widget::{DrawInteraction, InputContext, InputResult, MeasureFn, Widget};

pub(crate) struct Toggle {
    pub label: String,
    pub value_id: StateId,
}

impl Widget for Toggle {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<Toggle>().is_some() {
            DiffAction::Update
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self, measure: MeasureFn<'_>) -> Style {
        let text_size = measure(&self.label, None);
        Style {
            size: Size {
                width: Dimension::Length(text_size.x() + 28.0),
                height: Dimension::Length(text_size.y() + 8.0),
            },
            ..Style::default()
        }
    }

    fn handle_input(
        &self,
        ctx: &mut InputContext<'_>,
        state: &mut StateArena,
        bounds: Rect2D,
        _children: &[ViewId],
    ) -> InputResult {
        if bounds.contains(ctx.mouse_pos) && ctx.input.mouse_clicked(mouse_button::LEFT) {
            let current: bool = state.get(self.value_id).unwrap_or(false);
            state.set(self.value_id, !current);
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
        _interaction: &DrawInteraction,
        _view_id: ViewId,
        _children_bounds: &[Rect2D],
    ) {
        let checked: bool = state.get(self.value_id).unwrap_or(false);

        let bg_color = if checked {
            ctx.style().selectable_selected
        } else {
            ctx.style().button_normal
        };
        let bg_color = animation.apply_to_color(bg_color);
        let radius = animation.apply_to_corner_radius(ctx.style().button_rounding);
        ctx.draw_rounded_rect(bounds, bg_color, radius);

        let indicator_size = bounds.height() * 0.5;
        let indicator_center = if checked {
            Vec2::new(bounds.max.x() - indicator_size, bounds.center().y())
        } else {
            Vec2::new(bounds.min.x() + indicator_size, bounds.center().y())
        };
        ctx.draw_circle(
            indicator_center,
            indicator_size * 0.5,
            animation.apply_to_color(ctx.style().text_color),
        );

        if !self.label.is_empty() {
            let font_size = ctx.style().font_size;
            let text_size = ctx.measure_text(&self.label, font_size);
            let text_pos = Vec2::new(
                bounds.min.x() + ctx.style().item_inner_spacing,
                bounds.center().y() - text_size.y() * 0.5,
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
    use crate::declarative::actions::ActionStream;
    use crate::declarative::build::CallbackTable;
    use crate::declarative::constructors::text;
    use crate::declarative::state::ViewId;
    use crate::input::UiInputState;

    fn make_state_id(arena: &mut StateArena) -> StateId {
        let vid = ViewId::from(slotmap::KeyData::from_ffi(1));
        arena.get_or_create(vid, false)
    }

    #[test]
    fn test_toggle_flips_boolean() {
        let mut state = StateArena::new();
        let sid = make_state_id(&mut state);

        let toggle = Toggle {
            label: "enable".into(),
            value_id: sid,
        };

        let mut input = UiInputState::new();
        input.set_mouse_pos(Vec2::new(50.0, 10.0));
        input.set_mouse_button(mouse_button::LEFT, true);

        let bounds = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(100.0, 20.0));
        let mut callbacks = CallbackTable::new();
        let mut actions = ActionStream::new();

        let mut ctx = InputContext {
            input: &input,
            mouse_pos: Vec2::new(50.0, 10.0),
            callbacks: &mut callbacks,
            actions: &mut actions,
            view_id: ViewId::from(slotmap::KeyData::from_ffi(0)),
            active_id: None,
        };

        let result = toggle.handle_input(&mut ctx, &mut state, bounds, &[]);
        assert_eq!(result, InputResult::Consumed);

        let value: bool = state.get(sid).unwrap_or(false);
        assert!(value);
    }

    #[test]
    fn test_toggle_diff() {
        let mut state = StateArena::new();
        let sid = make_state_id(&mut state);

        let a = Toggle {
            label: "a".into(),
            value_id: sid,
        };
        let b = Toggle {
            label: "b".into(),
            value_id: sid,
        };
        assert_eq!(b.diff_against(&a), DiffAction::Update);

        let other = text("hello");
        assert_eq!(a.diff_against(&other), DiffAction::Replace);
    }
}
