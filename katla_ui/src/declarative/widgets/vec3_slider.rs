use std::any::Any;
use std::ops::RangeInclusive;

use katla_math::{Color, Rect2D, Vec2};
use taffy::{Dimension, Size, Style};

use crate::context::UiContext;
use crate::input::mouse_button;

use super::super::animation::AnimationState;
use super::super::diff::DiffAction;
use super::super::state::{StateArena, StateId, ViewId};
use super::super::widget::{DrawInteraction, InputContext, InputResult, MeasureFn, Widget};

pub(crate) struct Vec3Slider {
    pub label: String,
    pub value_ids: [StateId; 3],
    pub range: RangeInclusive<f32>,
    pub axis_labels: [String; 3],
    pub axis_colors: [Color; 3],
    pub precision: usize,
}

impl Widget for Vec3Slider {
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
                width: Dimension::Length((text_size.x() + 120.0).max(200.0)),
                height: Dimension::Length(text_size.y() * 3.0 + 20.0),
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
        if !bounds.contains(ctx.mouse_pos) {
            return InputResult::Ignore;
        }

        if !ctx.input.mouse_pressed[mouse_button::LEFT] {
            return InputResult::Ignore;
        }

        let row_height = bounds.height() / 3.0;
        let axis = ((ctx.mouse_pos.y() - bounds.min.y()) / row_height).clamp(0.0, 2.99) as usize;

        let axis_label_width = 20.0;
        let track_x = bounds.min.x() + axis_label_width;
        let track_width = bounds.max.x() - track_x - 40.0;
        let t = if track_width > 0.0 {
            ((ctx.mouse_pos.x() - track_x) / track_width).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let new_val = *self.range.start() + t * (*self.range.end() - *self.range.start());
        state.set(self.value_ids[axis], new_val);

        InputResult::Consumed
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
        let font_size = ctx.style().font_size;
        let text_color = animation.apply_to_color(ctx.style().text_color);
        let row_height = bounds.height() / 3.0;
        let axis_label_width = 20.0;
        let value_text_width = 40.0;

        for i in 0..3 {
            let row_y = bounds.min.y() + row_height * i as f32;
            let row_bounds = Rect2D::from_origin_size(
                Vec2::new(bounds.min.x(), row_y),
                Vec2::new(bounds.width(), row_height),
            );

            let axis_label = &self.axis_labels[i];
            let axis_color = self.axis_colors[i];
            let axis_label_size = ctx.measure_text(axis_label, font_size);
            let axis_label_y = row_bounds.center().y() - axis_label_size.y() * 0.5;
            ctx.draw_text(
                axis_label,
                Vec2::new(row_bounds.min.x(), axis_label_y),
                axis_color,
                font_size,
            );

            let value: f32 = state.get(self.value_ids[i]).unwrap_or_default();
            let t = if *self.range.end() > *self.range.start() {
                (value - *self.range.start()) / (*self.range.end() - *self.range.start())
            } else {
                0.0
            };

            let track_x = row_bounds.min.x() + axis_label_width;
            let track_end = row_bounds.max.x() - value_text_width;
            let track_width = (track_end - track_x).max(0.0);
            let track_height = ctx.style().slider_track_height;
            let track_center_y = row_bounds.center().y();

            let track_bounds = Rect2D::from_center_size(
                Vec2::new(track_x + track_width * 0.5, track_center_y),
                Vec2::new(track_width, track_height),
            );
            ctx.draw_rounded_rect(track_bounds, ctx.style().slider_track, track_height * 0.5);

            let fill_width = t * track_width;
            if fill_width > 0.0 {
                let fill_bounds =
                    Rect2D::from_origin_size(track_bounds.min, Vec2::new(fill_width, track_height));
                ctx.draw_rounded_rect(fill_bounds, ctx.style().slider_grab, track_height * 0.5);
            }

            let grab_center_x = track_x + t * track_width;
            let grab_center = Vec2::new(grab_center_x, track_center_y);
            let grab_radius = ctx.style().slider_grab_size * 0.5;
            ctx.draw_circle(grab_center, grab_radius, ctx.style().slider_grab);

            let value_text = format!("{:.1$}", value, self.precision);
            let text_size = ctx.measure_text(&value_text, font_size);
            let value_x = row_bounds.max.x() - text_size.x();
            let value_y = row_bounds.center().y() - text_size.y() * 0.5;
            ctx.draw_text(
                &value_text,
                Vec2::new(value_x, value_y),
                text_color,
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

    fn make_vec3_slider() -> Vec3Slider {
        Vec3Slider {
            label: "Position".into(),
            value_ids: [StateId::test_id(); 3],
            range: -1.0..=1.0,
            axis_labels: ["X".into(), "Y".into(), "Z".into()],
            axis_colors: [
                Color::new(1.0, 0.0, 0.0, 1.0),
                Color::new(0.0, 1.0, 0.0, 1.0),
                Color::new(0.0, 0.0, 1.0, 1.0),
            ],
            precision: 2,
        }
    }

    #[test]
    fn test_vec3_slider_diff() {
        let a = make_vec3_slider();
        let b = make_vec3_slider();
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
    fn test_vec3_slider_focusable() {
        let slider = make_vec3_slider();
        assert!(slider.focusable());
    }

    #[test]
    fn test_vec3_slider_layout_default() {
        let slider = make_vec3_slider();
        let style = slider.layout_style(&crate::declarative::layout::measure_text_descriptor);
        let default_width = taffy::Dimension::Length(0.0);
        assert!(style.size.width != default_width);
    }

    #[test]
    fn test_vec3_slider_handle_input_clicks_axis() {
        let slider = make_vec3_slider();
        let mut state = StateArena::new();

        let mut input = UiInputState::new();
        input.mouse_pos = Vec2::new(60.0, 5.0);
        input.mouse_pressed[mouse_button::LEFT] = true;

        let bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(200.0, 60.0));

        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: input.mouse_pos,
            callbacks: &mut callbacks,
            actions: &mut actions,
            view_id: ViewId::from(slotmap::KeyData::from_ffi(0)),
            active_id: None,
        };

        let result = slider.handle_input(&mut ctx, &mut state, bounds, &[]);
        assert_eq!(result, InputResult::Consumed);
    }

    #[test]
    fn test_vec3_slider_handle_input_outside_bounds() {
        let slider = make_vec3_slider();
        let mut state = StateArena::new();

        let mut input = UiInputState::new();
        input.mouse_pos = Vec2::new(300.0, 5.0);
        input.mouse_pressed[mouse_button::LEFT] = true;

        let bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(200.0, 60.0));

        let mut callbacks = crate::declarative::build::CallbackTable::new();
        let mut actions = crate::declarative::actions::ActionStream::new();
        let mut ctx = InputContext {
            input: &input,
            mouse_pos: input.mouse_pos,
            callbacks: &mut callbacks,
            actions: &mut actions,
            view_id: ViewId::from(slotmap::KeyData::from_ffi(0)),
            active_id: None,
        };

        let result = slider.handle_input(&mut ctx, &mut state, bounds, &[]);
        assert_eq!(result, InputResult::Ignore);
    }
}
