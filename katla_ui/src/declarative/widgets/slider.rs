use std::any::Any;
use std::ops::RangeInclusive;

use katla_math::{Color, Rect2D, Vec2};
use taffy::Style;

use crate::context::UiContext;
use crate::input::mouse_button;

use super::super::animation::AnimationState;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, StateId, ViewId};
use super::super::widget::{InputContext, InputResult, Widget};

pub(crate) struct Slider {
    pub label: String,
    pub value_id: StateId,
    pub range: RangeInclusive<f32>,
    pub show_value: bool,
    pub precision: usize,
}

impl Widget for Slider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn diff_against(&self, prev: &dyn Widget) -> DiffAction {
        if prev.as_any().downcast_ref::<Slider>().is_some() {
            DiffAction::Update
        } else {
            DiffAction::Replace
        }
    }

    fn layout_style(&self) -> Style {
        Style::default()
    }

    fn handle_input(
        &self,
        ctx: &mut InputContext<'_>,
        state: &mut StateArena,
        bounds: Rect2D,
        _children: &[ViewId],
    ) -> InputResult {
        if bounds.contains(ctx.mouse_pos) && ctx.input.mouse_down[mouse_button::LEFT] {
            let t = if bounds.width() > 0.0 {
                ((ctx.mouse_pos.x() - bounds.min.x()) / bounds.width()).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let value = *self.range.start() + t * (*self.range.end() - *self.range.start());
            state.set(self.value_id, value);
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
    ) {
        let value: f32 = state.get(self.value_id).unwrap_or_default();
        let t = if *self.range.end() != *self.range.start() {
            ((value - *self.range.start()) / (*self.range.end() - *self.range.start()))
                .clamp(0.0, 1.0)
        } else {
            0.0
        };

        let track_height = ctx.style().slider_track_height;
        let track_bounds =
            Rect2D::from_center_size(bounds.center(), Vec2::new(bounds.width(), track_height));
        ctx.draw_rounded_rect(track_bounds, ctx.style().slider_track, track_height * 0.5);

        let fill_width = t * bounds.width();
        if fill_width > 0.0 {
            let fill_bounds =
                Rect2D::from_origin_size(track_bounds.min, Vec2::new(fill_width, track_height));
            ctx.draw_rounded_rect(
                fill_bounds,
                animation.apply_to_color(ctx.style().slider_grab),
                track_height * 0.5,
            );
        }

        let grab_center_x = bounds.min.x() + t * bounds.width();
        let grab_center = Vec2::new(grab_center_x, bounds.center().y());
        let grab_radius = ctx.style().slider_grab_size * 0.5;
        ctx.draw_circle(
            Vec2::new(grab_center.x(), grab_center.y() + 1.0),
            grab_radius,
            Color::new(0.0, 0.0, 0.0, 0.3),
        );
        ctx.draw_circle(grab_center, grab_radius, ctx.style().slider_grab);

        if !self.label.is_empty() {
            ctx.draw_text(
                &self.label,
                bounds.min,
                animation.apply_to_color(ctx.style().text_color),
                ctx.style().font_size,
            );
        }

        if self.show_value {
            let value_text = format!("{:.1$}", value, self.precision);
            let text_size = ctx.measure_text(&value_text, ctx.style().font_size);
            let text_pos = Vec2::new(
                bounds.center().x() - text_size.x() * 0.5,
                bounds.center().y() - text_size.y() * 0.5,
            );
            ctx.draw_text(
                &value_text,
                text_pos,
                animation.apply_to_color(ctx.style().text_color),
                ctx.style().font_size,
            );
        }
    }

    fn focusable(&self) -> bool {
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
    use crate::declarative::widget::DescriptorWidget;
    use crate::input::UiInputState;

    fn make_state_id(arena: &mut StateArena) -> StateId {
        let vid = ViewId::from(slotmap::KeyData::from_ffi(1));
        arena.get_or_create(vid, 0.0f32)
    }

    #[test]
    fn test_slider_drag_updates_value() {
        let mut state = StateArena::new();
        let sid = make_state_id(&mut state);

        let slider = Slider {
            label: "vol".into(),
            value_id: sid,
            range: 0.0..=1.0,
            show_value: false,
            precision: 2,
        };

        let mut input = UiInputState::new();
        input.set_mouse_pos(Vec2::new(75.0, 10.0));
        input.set_mouse_button(mouse_button::LEFT, true);

        let bounds = Rect2D::new(Vec2::new(0.0, 0.0), Vec2::new(100.0, 20.0));
        let mut callbacks = CallbackTable::new();
        let mut actions = ActionStream::new();

        let mut ctx = InputContext {
            input: &input,
            mouse_pos: Vec2::new(75.0, 10.0),
            callbacks: &mut callbacks,
            actions: &mut actions,
        };

        let result = slider.handle_input(&mut ctx, &mut state, bounds, &[]);
        assert_eq!(result, InputResult::Consumed);

        let updated: f32 = state.get(sid).unwrap_or(0.0);
        assert!((updated - 0.75).abs() < 0.01);
    }

    #[test]
    fn test_slider_diff() {
        let mut state = StateArena::new();
        let sid = make_state_id(&mut state);

        let a = Slider {
            label: "a".into(),
            value_id: sid,
            range: 0.0..=1.0,
            show_value: false,
            precision: 2,
        };
        let b = Slider {
            label: "b".into(),
            value_id: sid,
            range: 0.0..=1.0,
            show_value: true,
            precision: 1,
        };
        assert_eq!(b.diff_against(&a), DiffAction::Update);

        let other = DescriptorWidget::new(text("hello"));
        assert_eq!(a.diff_against(&other), DiffAction::Replace);
    }
}
